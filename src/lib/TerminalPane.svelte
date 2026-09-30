<script lang="ts">
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { SearchAddon } from "@xterm/addon-search";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import "@xterm/xterm/css/xterm.css";
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import {
    COPY_TOAST_DISMISS_MS,
    SELECTION_DEBOUNCE_MS,
    copyTextToClipboard,
    shouldAutoCopy,
    truncatePreview,
  } from "./clipboard";
  import { findTabByPane } from "./layout";
  import {
    acquireSession,
    attachResultFor,
    closeSession,
    dropSession,
    noteAttachResult,
    type SessionLease,
  } from "./ptySessions";
  import {
    TerminalAttachment,
    fetchReplay,
    fetchSnapshot,
    snapshotFailurePlan,
    type AttachResult,
    type DaemonStatus,
    type TerminalExit,
    type TerminalOutput,
    type TerminalSnapshot,
  } from "./terminalLifecycle";
  import { store } from "./store.svelte";
  import { withAlpha, type AppTheme } from "./themes";
  import { toasts } from "./toasts.svelte.ts";

  interface Props {
    /** Stable pane node id: reattaches to the surviving PTY across remounts. */
    sessionKey: string;
    cwd?: string;
    theme: AppTheme;
    fontSize: number;
    /** Background opacity 0..1. */
    opacity: number;
    /** Bumped to move keyboard focus into this terminal. */
    focusToken: number;
    /** Bumped to open the find bar. */
    findToken: number;
    /** Scrollback lines kept. */
    scrollback: number;
    onExit?: () => void;
    onSpawn?: (live: { id: number; epoch: number; incarnation: number }) => void;
    onDispose?: (liveId: number) => void;
  }
  let {
    sessionKey,
    cwd,
    theme,
    fontSize,
    opacity,
    focusToken,
    findToken,
    scrollback,
    onExit,
    onSpawn,
    onDispose,
  }: Props = $props();

  let container: HTMLDivElement | undefined = $state();
  let terminal: Terminal | null = null;
  let refit: (() => void) | null = null;
  let searchAddon: SearchAddon | null = null;
  let finding = $state(false);
  let findText = $state("");
  /** Unavailable outcome (saved data shown, retry offered). */
  let unavailable = $state<string | null>(null);
  /** Live-session notice (e.g. conversation fallback) with retry. */
  let notice = $state<string | null>(null);
  /** Daemon transport lost; input held until reattach. */
  let disconnected = $state(false);
  /** Carries an unavailable outcome through lease rejection. */
  class AttachFailed extends Error {
    result: AttachResult;
    constructor(result: AttachResult) {
      super(result.error ?? "terminal unavailable");
      this.result = result;
    }
  }
  let retryAction: (() => void) | null = null;
  let noticeRetryAction: (() => void) | null = null;

  function focusInput(el: HTMLInputElement): void {
    el.focus();
    el.select();
  }

  function closeFind(): void {
    searchAddon?.clearDecorations();
    findText = "";
    finding = false;
    terminal?.focus();
  }

  function findStep(step: 1 | -1): void {
    if (!findText) return;
    if (step > 0) searchAddon?.findNext(findText);
    else searchAddon?.findPrevious(findText);
  }

  $effect(() => {
    const palette = theme.terminal;
    const alpha = opacity;
    if (terminal) {
      terminal.options.theme = {
        ...palette,
        background: withAlpha(palette.background, alpha),
      };
    }
  });

  $effect(() => {
    const px = fontSize;
    if (terminal) {
      terminal.options.fontSize = px;
      // Refitting reflows and reports the new grid so the PTY resizes too.
      refit?.();
    }
  });

  $effect(() => {
    // Programmatic pane switches bump the token; 0 is the untouched state.
    if (focusToken > 0) terminal?.focus();
  });

  $effect(() => {
    if (findToken > 0) finding = true;
  });

  $effect(() => {
    const lines = scrollback;
    if (terminal) terminal.options.scrollback = lines;
  });

  onMount(() => {
    const term = new Terminal({
      cursorBlink: true,
      fontSize,
      scrollback,
      fontFamily: "Menlo, Consolas, 'Courier New', monospace",
      theme: {
        ...theme.terminal,
        background: withAlpha(theme.terminal.background, opacity),
      },
    });
    terminal = term;
    const fit = new FitAddon();
    term.loadAddon(fit);
    searchAddon = new SearchAddon();
    term.loadAddon(searchAddon);
    term.loadAddon(
      new WebLinksAddon((_event, uri) => {
        openUrl(uri).catch(console.error);
      }),
    );
    term.open(container!);

    let paneId: number | null = null;
    let paneEpoch = 0;
    let paneIncarnation = 0;
    let connected = true;
    let disposed = false;
    let generation = 0;
    const unlistens: UnlistenFn[] = [];
    let attachment = new TerminalAttachment();
    let lease: SessionLease | null = null;
    let exited = false;

    const handleExit = (success: boolean, code: number | null) => {
      if (exited) return;
      exited = true;
      if (lease) dropSession(sessionKey, lease);
      term.write(
        `\r\n[process exited${success ? "" : ` (code ${code})`}]\r\n`,
      );
      onExit?.();
    };

    // Select-to-copy: any selection auto-copies to the system clipboard with
    // a short toast. Selection events fire continuously mid-drag, so debounce;
    // mouseup copies immediately for snappy feedback.
    let lastCopied = "";
    let copyTimer: ReturnType<typeof setTimeout> | null = null;
    const doAutoCopy = async (): Promise<void> => {
      copyTimer = null;
      if (disposed) return;
      const selection = term.getSelection();
      if (selection.length === 0) {
        // Selection cleared: allow re-copying the same text next time.
        lastCopied = "";
        return;
      }
      if (!shouldAutoCopy(selection, lastCopied)) return;
      lastCopied = selection;
      const ok = await copyTextToClipboard(selection);
      if (!disposed && ok) {
        toasts.push("Copied to clipboard", truncatePreview(selection), "", {
          dismissMs: COPY_TOAST_DISMISS_MS,
          kind: "copy",
        });
      }
    };
    const scheduleAutoCopy = (): void => {
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => void doAutoCopy(), SELECTION_DEBOUNCE_MS);
    };
    const selectionDispose = term.onSelectionChange(scheduleAutoCopy);
    const host = container!;
    const onMouseUp = (): void => {
      if (copyTimer) {
        clearTimeout(copyTimer);
        copyTimer = null;
      }
      void doAutoCopy();
    };
    host.addEventListener("mouseup", onMouseUp);

    // Hidden panes (inactive tabs/workspaces) have zero size: skip fitting
    // until visible. The ResizeObserver fires on show.
    let lastResize: { id: number; cols: number; rows: number } | null = null;
    const ensureFit = () => {
      if (
        disposed ||
        !container ||
        container.clientWidth < 10 ||
        container.clientHeight < 10
      )
        return;
      fit.fit();
      if (paneId !== null && (lastResize?.id !== paneId ||
        lastResize.cols !== term.cols || lastResize.rows !== term.rows)) {
        const size = { id: paneId, cols: term.cols, rows: term.rows };
        lastResize = size;
        invoke("pty_resize", {
          ...size,
          epoch: paneEpoch,
          incarnation: paneIncarnation,
        }).catch((error) => {
          if (lastResize === size) lastResize = null;
          console.error(error);
        });
      }
    };
    ensureFit();
    refit = ensureFit;

    const resizeObserver = new ResizeObserver(ensureFit);
    resizeObserver.observe(container!);

    const killPane = (id: number): Promise<unknown> => {
      if (id <= 0) return Promise.resolve();
      return invoke("pty_kill", {
        id,
        epoch: paneEpoch,
        incarnation: paneIncarnation,
      }).catch((error) => {
        // Removal races layout-driven closure; a missing pane is the goal.
        if (String(error).includes("no such pane")) return;
        throw error;
      });
    };

    const showUnavailable = (message: string, retry: () => void): void => {
      unavailable = message;
      retryAction = retry;
    };

    let snapshotFailures = 0;

    const reattach = (fresh: boolean): void => {
      if (lease) dropSession(sessionKey, lease);
      lease = null;
      paneId = null;
      exited = false;
      unavailable = null;
      notice = null;
      if (fresh) snapshotFailures = 0;
      attachment = new TerminalAttachment();
      term.clear();
      generation += 1;
      void attachFlow(generation);
    };

    /** One daemon attach; runs once per lease, result shared via the map. */
    const doAttach = async (): Promise<number> => {
      const argv = store.commandForSpawn(sessionKey);
      const result = await invoke<AttachResult>("pty_spawn", {
        key: sessionKey,
        shell: argv?.[0] ?? null,
        cwd: cwd ?? null,
        args: argv?.slice(1) ?? null,
        cols: Math.max(term.cols, 2),
        rows: Math.max(term.rows, 1),
        agentRecovery: store.agentRecovery,
      });
      noteAttachResult(sessionKey, result);
      if (!result.ok || result.attached === "unavailable") {
        throw new AttachFailed(result);
      }
      // Previous-lifetime exits resolve a sentinel; the mount renders replay.
      return result.pane;
    };

    /** Mount-side rendering for the lease's attach outcome. */
    const renderAttach = async (gen: number, id: number): Promise<void> => {
      const result = attachResultFor<AttachResult>(sessionKey);
      if (!result) {
        term.write("\r\n[attach produced no outcome]\r\n");
        handleExit(false, null);
        return;
      }
      paneEpoch = result.epoch;
      paneIncarnation = result.incarnation;

      if (result.attached === "exited" && id <= 0) {
        // Exited in a previous daemon lifetime: replay saved output, never
        // silently rerun. PaneView offers an explicit restart.
        const replay = await fetchReplay(sessionKey, result).catch(() => "");
        if (disposed || gen !== generation) return;
        if (replay) term.write(replay);
        handleExit(result.exit?.success ?? false, result.exit?.code ?? null);
        return;
      }

      let snapshot: TerminalSnapshot | null = null;
      try {
        snapshot = await fetchSnapshot(id, result.epoch, result.incarnation);
      } catch (error) {
        if (disposed || gen !== generation) return;
        // Stale identity or transport loss between attach and snapshot: the
        // daemon was replaced under us. Re-resolve instead of faking an exit.
        snapshotFailures += 1;
        if (snapshotFailurePlan(snapshotFailures) === "retry") {
          reattach(false);
          return;
        }
        term.write(`\r\n[snapshot unavailable: ${String(error)}]\r\n`);
        showUnavailable(`snapshot unavailable: ${String(error)}`, () => reattach(true));
        return;
      }
      if (disposed || gen !== generation) return;
      snapshotFailures = 0;
      paneId = id;
      if (result.attached === "exited") {
        // Retained final screen of a naturally exited session.
        if (snapshot) term.resize(snapshot.cols, snapshot.rows);
        const restored = attachment.restore(id, snapshot);
        for (const chunk of restored.chunks) term.write(chunk);
        handleExit(result.exit?.success ?? false, result.exit?.code ?? null);
        return;
      }
      onSpawn?.({ id, epoch: result.epoch, incarnation: result.incarnation });
      if (snapshot) term.resize(snapshot.cols, snapshot.rows);
      const restored = attachment.restore(id, snapshot);
      for (const chunk of restored.chunks) term.write(chunk);
      if (restored.exit) handleExit(restored.exit.success, restored.exit.code);
      if (!exited && result.note) {
        notice = result.note;
        noticeRetryAction = () => {
          notice = null;
          invoke("pty_close", { key: sessionKey })
            .catch((error) => console.error(error))
            .finally(() => reattach(true));
        };
        term.write(`\r\n[${result.note}]\r\n`);
      }
      if (!exited) ensureFit();
    };

    const renderUnavailable = async (
      gen: number,
      message: string,
      result: AttachResult | null,
    ): Promise<void> => {
      if (result) {
        const replay = await fetchReplay(sessionKey, result).catch(() => "");
        if (disposed || gen !== generation) return;
        if (replay) term.write(replay);
      }
      if (disposed || gen !== generation) return;
      term.write(`\r\n[${message}]\r\n`);
      showUnavailable(message, () => reattach(true));
    };

    const attachFlow = async (gen: number): Promise<void> => {
      lease = acquireSession(sessionKey, doAttach, killPane);
      try {
        const id = await lease.ready;
        if (disposed || lease.cancelled || gen !== generation) return;
        await renderAttach(gen, id);
      } catch (error) {
        if (disposed || (lease && lease.cancelled) || gen !== generation) return;
        if (error instanceof AttachFailed) {
          await renderUnavailable(gen, error.result.error ?? "terminal unavailable", error.result);
        } else if (!exited) {
          await renderUnavailable(
            gen,
            `failed to reach the background daemon: ${error instanceof Error ? error.message : String(error)}`,
            attachResultFor<AttachResult>(sessionKey),
          );
        }
      }
    };

    (async () => {
      const outputUnlisten = await listen<TerminalOutput>(
        "pty-output",
        (event) => {
          if (disposed) return;
          const data = attachment.output(event.payload);
          if (data !== null) term.write(data);
        },
      );
      if (disposed) {
        outputUnlisten();
        return;
      }
      unlistens.push(outputUnlisten);

      const exitUnlisten = await listen<TerminalExit>("pty-exit", (event) => {
        if (disposed) return;
        const exit = attachment.exit(event.payload);
        if (exit) handleExit(exit.success, exit.code);
      });
      if (disposed) {
        exitUnlisten();
        return;
      }
      unlistens.push(exitUnlisten);

      const daemonUnlisten = await listen<DaemonStatus>("daemon-status", (event) => {
        if (disposed) return;
        const status = event.payload;
        connected = status.connected;
        disconnected = !status.connected;
        // Reconnected daemons renumbered everything; panes stuck showing
        // unavailable also retry here, covering first connects that landed
        // after their initial attach failed.
        if (status.connected && (status.reconnected || unavailable)) reattach(true);
      });
      if (disposed) {
        daemonUnlisten();
        return;
      }
      unlistens.push(daemonUnlisten);

      term.onData((data) => {
        if (paneId !== null && !disposed && connected) {
          invoke("pty_write", {
            id: paneId,
            data,
            epoch: paneEpoch,
            incarnation: paneIncarnation,
          }).catch(console.error);
        }
      });
      generation += 1;
      void attachFlow(generation);
    })().catch((e) => {
      console.error(e);
      if (disposed) return;
      term.write(
        `\r\n[failed to start: ${e instanceof Error ? e.message : String(e)}]\r\n`,
      );
      onExit?.();
    });

    return () => {
      disposed = true;
      terminal = null;
      refit = null;
      searchAddon = null;
      if (copyTimer) clearTimeout(copyTimer);
      selectionDispose.dispose();
      host.removeEventListener("mouseup", onMouseUp);
      resizeObserver.disconnect();
      unlistens.forEach((u) => u());
      const stillPlaced =
        store.layout !== null && findTabByPane(store.layout, sessionKey) !== null;
      if (!stillPlaced) {
        if (paneId !== null) onDispose?.(paneId);
        closeSession(sessionKey);
      }
      term.dispose();
    };
  });

  function onRetryClick(): void {
    retryAction?.();
  }

  function onNoticeRetry(): void {
    noticeRetryAction?.();
  }
</script>

<div class="terminal" bind:this={container}>
{#if finding}
  <div class="findbar" role="search">
    <input
      class="find-input"
      use:focusInput
      bind:value={findText}
      oninput={() => {
        if (findText) searchAddon?.findNext(findText, { incremental: true });
      }}
      onkeydown={(e) => {
        if (e.key === "Enter" && !e.shiftKey) findStep(1);
        else if (e.key === "Enter") findStep(-1);
        else if (e.key === "Escape") closeFind();
      }}
      placeholder="Find in terminal"
      aria-label="Find in terminal"
    />
    <button
      class="find-btn"
      title="Previous match (Shift+Enter)"
      aria-label="Previous match"
      onclick={() => findStep(-1)}
      ><Icon name="chevron-up" size={12} /></button
    >
    <button
      class="find-btn"
      title="Next match (Enter)"
      aria-label="Next match"
      onclick={() => findStep(1)}
      ><Icon name="chevron-down" size={12} /></button
    >
    <button
      class="find-btn"
      title="Close find (Esc)"
      aria-label="Close find"
      onclick={closeFind}><Icon name="x" size={12} /></button
    >
  </div>
{/if}
{#if notice}
  <div class="noticebar" role="status">
    <span class="notice-text">{notice}</span>
    <button class="notice-btn" onclick={onNoticeRetry}>Retry</button>
  </div>
{/if}
{#if disconnected}
  <div class="overlay" role="status">
    <div class="overlay-card">
      <div class="overlay-title">Reconnecting…</div>
      <div class="overlay-desc">The background daemon connection was lost.</div>
    </div>
  </div>
{/if}
{#if unavailable}
  <div class="overlay" role="alert">
    <div class="overlay-card">
      <div class="overlay-title">Terminal unavailable</div>
      <div class="overlay-desc">{unavailable}</div>
      <button class="overlay-btn" onclick={onRetryClick}>Retry</button>
    </div>
  </div>
{/if}
</div>

<style>
  .terminal {
    position: relative;
    width: 100%;
    height: 100%;
    background: var(--terminal-background);
  }
  .findbar {
    position: absolute;
    top: 6px;
    right: 8px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.35);
  }
  .find-input {
    width: 180px;
    font: calc(12px * var(--ui-text-scale, 1)) var(--font-ui);
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 4px;
    padding: 3px 6px;
  }
  .find-btn {
    display: inline-flex;
    align-items: center;
    color: var(--text-subtle);
    background: transparent;
    border: none;
    border-radius: 4px;
    padding: 3px 6px;
    cursor: pointer;
  }
  .find-btn:hover {
    color: var(--text);
    background: var(--surface-hover);
  }
  .noticebar {
    position: absolute;
    top: 6px;
    left: 8px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: calc(100% - 16px);
    padding: 4px 6px 4px 10px;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.35);
    font: 12px var(--font-ui);
    color: var(--text);
  }
  .notice-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .notice-btn {
    flex: none;
    font: 12px var(--font-ui);
    color: var(--text);
    background: var(--surface-hover);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 8px;
    cursor: pointer;
  }
  .overlay {
    position: absolute;
    inset: 0;
    z-index: 6;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgb(0 0 0 / 0.45);
  }
  .overlay-card {
    max-width: 320px;
    padding: 16px 18px;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    text-align: center;
    font: 12px var(--font-ui);
    color: var(--text);
  }
  .overlay-title {
    font-weight: 600;
    margin-bottom: 6px;
  }
  .overlay-desc {
    color: var(--text-subtle);
    margin-bottom: 12px;
    overflow-wrap: break-word;
  }
  .overlay-btn {
    font: 12px var(--font-ui);
    color: var(--text);
    background: var(--surface-hover);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 16px;
    cursor: pointer;
  }
  .terminal :global(.xterm) {
    height: 100%;
    padding: 4px 0 4px 8px;
    background: var(--terminal-background);
  }
  .terminal :global(.xterm-viewport) {
    background-color: var(--terminal-background) !important;
    scrollbar-color: var(--scrollbar-thumb) var(--terminal-background);
  }
  .terminal :global(.xterm-scrollable-element > .scrollbar > .slider) {
    background: var(--scrollbar-thumb) !important;
    border-radius: 4px;
  }
  .terminal :global(.xterm-scrollable-element > .scrollbar > .slider:hover) {
    background: var(--accent) !important;
  }
</style>

<script lang="ts">
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { SearchAddon } from "@xterm/addon-search";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import type { WebglAddon } from "@xterm/addon-webgl";
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
  import { acquireSession, closeSession, dropSession, type SessionLease } from "./ptySessions";
  import { frameCoalescer, trailingDebouncer } from "./schedule";
  import { terminalCommands } from "./terminalCommands";
  import { disableGpuRenderer, enableGpuRenderer, gpuUpgradeQueue } from "./terminalGpu";
  import { TerminalAttachment, type PtySessionInfo, type TerminalOutput, type TerminalExit, type TerminalSnapshot } from "./terminalLifecycle";
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
    /** False forces the canvas renderer (Settings toggle). */
    gpuEnabled: boolean;
    onExit?: () => void;
    /**
     * Fired once the live PTY id is known. `attached` means a surviving
     * backend session was adopted (never rerun the agent); `firstDelivery`
     * is true only for the first mount delivering this lease.
     */
    onSpawn?: (liveId: number, attached: boolean, firstDelivery: boolean) => void;
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
    gpuEnabled,
    onExit,
    onSpawn,
    onDispose,
  }: Props = $props();

  let container: HTMLDivElement | undefined = $state();
  let terminal: Terminal | null = null;
  let refit: (() => void) | null = null;
  let setGpuEnabled: ((on: boolean) => void) | null = null;
  let searchAddon: SearchAddon | null = null;
  let finding = $state(false);
  let findText = $state("");

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

  $effect(() => {
    const on = gpuEnabled;
    // Null until onMount installs the handler; the mount path reads the
    // initial prop directly, so a pre-mount run is safely skipped.
    setGpuEnabled?.(on);
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
    let disposed = false;
    // GPU renderer state: visible panes upgrade to WebGL while hidden ones
    // stay on (or drop back to) canvas, so background tabs never hold GPU
    // contexts. A failed attempt waits for the next hide/show cycle instead
    // of logging once per frame during resize bursts.
    let gpu: WebglAddon | null = null;
    let gpuFailed = false;
    let gpuWanted = gpuEnabled;
    const ensureGpu = (): void => {
      if (disposed || gpu !== null || gpuFailed || !gpuWanted) return;
      // Upgrades drain through the shared queue (latest wins per pane) so a
      // multi-pane reveal never pays N WebGL context creations in one frame.
      // The canvas fit already ran, so panes paint correctly while queued.
      gpuUpgradeQueue.push(sessionKey, () => {
        if (disposed || gpu !== null || gpuFailed || !gpuWanted) return;
        gpu = enableGpuRenderer(term);
        if (gpu !== null) console.debug(`ubra: GPU terminal renderer active (${sessionKey})`);
        else gpuFailed = true;
      });
    };
    const dropGpu = (): void => {
      gpuFailed = false;
      gpuUpgradeQueue.drop(sessionKey);
      gpu = disableGpuRenderer(gpu);
    };
    setGpuEnabled = (on: boolean) => {
      gpuWanted = on;
      if (disposed) return;
      if (!on) dropGpu();
      else if (
        container &&
        container.clientWidth >= 10 &&
        container.clientHeight >= 10
      )
        ensureGpu();
      // Hidden panes upgrade on show via doFit.
    };
    const unlistens: UnlistenFn[] = [];
    const attachment = new TerminalAttachment();
    let lease: SessionLease | null = null;
    let exited = false;
    const unregisterCommands = terminalCommands.register(sessionKey, {
      selection: () => term.getSelection(),
      paste: (text) => {
        if (!disposed && !exited && paneId !== null) { term.focus(); term.paste(text); }
      },
      selectAll: () => { if (!disposed) term.selectAll(); },
      running: () => !disposed && !exited && paneId !== null,
    });
    const menuSelectionDispose = term.onSelectionChange(() => terminalCommands.changed());
    // The document capture handler owns app shortcuts before xterm handles them.
    term.attachCustomKeyEventHandler((event) => !event.defaultPrevented);

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
    //
    // Resize bursts (window/sidebar/pane drags) collapse in two stages: the
    // canvas refits at most once per frame for smooth visuals, while the
    // backend PTY resize fires once the burst goes quiet. Shells and CLIs
    // then see one stable size instead of a resize storm.
    const PTY_RESIZE_DEBOUNCE_MS = 120;
    const fitFrame = frameCoalescer();
    const ptyResize = trailingDebouncer(PTY_RESIZE_DEBOUNCE_MS);
    let lastSent: { id: number; cols: number; rows: number } | null = null;
    const sendPtyResize = (): void => {
      if (disposed || paneId === null) return;
      const cols = term.cols;
      const rows = term.rows;
      if (
        lastSent?.id === paneId &&
        lastSent.cols === cols &&
        lastSent.rows === rows
      )
        return;
      const size = { id: paneId, cols, rows };
      lastSent = size;
      invoke("pty_resize", size).catch((error) => {
        if (lastSent === size) lastSent = null;
        console.error(error);
      });
    };
    const doFit = (): void => {
      if (
        disposed ||
        !container ||
        container.clientWidth < 10 ||
        container.clientHeight < 10
      ) {
        // Hidden: release the GPU context; re-acquired on show.
        dropGpu();
        return;
      }
      ensureGpu();
      fit.fit();
      ptyResize.schedule(sendPtyResize);
    };
    const ensureFit = (): void => {
      fitFrame.schedule(doFit);
    };
    ensureFit();
    refit = ensureFit;

    const resizeObserver = new ResizeObserver(ensureFit);
    resizeObserver.observe(container!);

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

      term.onData((data) => {
        if (paneId !== null && !disposed) {
          invoke("pty_write", { id: paneId, data }).catch(console.error);
        }
      });
      lease = acquireSession(sessionKey, async () => {
        const argv = store.commandForSpawn(sessionKey);
        // Adopt a surviving backend session with our stable key (e.g. after
        // a webview reload with the backend alive) instead of spawning a
        // blank replacement and orphaning the old process.
        try {
          const sessions = await invoke<PtySessionInfo[]>("pty_list");
          const owned = sessions
            .filter((s) => s.key === sessionKey)
            .map((s) => s.id)
            .sort((a, b) => a - b);
          for (const candidate of owned) {
            try {
              await invoke("pty_snapshot", { id: candidate });
              lease!.attached = true;
              return candidate;
            } catch {
              // Exited between list and adopt; try the next duplicate.
            }
          }
        } catch (error) {
          console.error("ubra: session adopt failed, spawning", error);
        }
        lease!.attached = false;
        return invoke<number>("pty_spawn", {
          shell: argv?.[0] ?? null,
          cwd: cwd ?? null,
          args: argv?.slice(1) ?? null,
          cols: Math.max(term.cols, 2),
          rows: Math.max(term.rows, 1),
          key: sessionKey,
        });
      }, (id) => invoke("pty_kill", { id }));
      const id = await lease.ready;
      if (disposed || lease.cancelled) return;
      let snapshot: TerminalSnapshot | null = null;
      let snapshotError: unknown = null;
      try {
        snapshot = await invoke<TerminalSnapshot>("pty_snapshot", { id });
      } catch (error) {
        snapshotError = error;
      }
      if (disposed || lease.cancelled) return;
      paneId = id;
      terminalCommands.changed();
      const firstDelivery = !lease.restoreTaken;
      lease.restoreTaken = true;
      onSpawn?.(id, lease.attached, firstDelivery);
      if (snapshot) term.resize(snapshot.cols, snapshot.rows);
      if (disposed || lease.cancelled) return;
      const restored = attachment.restore(id, snapshot);
      restored.chunks.forEach((chunk) => {
        term.write(chunk);
      });
      if (restored.exit) handleExit(restored.exit.success, restored.exit.code);
      else if (snapshotError) {
        // A session that exited while unmounted must show respawn, not silently
        // create a replacement process or retain dead registry ownership.
        term.write(`\r\n[snapshot unavailable: ${String(snapshotError)}]\r\n`);
        handleExit(false, null);
      }
      if (!exited) ensureFit();
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
      setGpuEnabled = null;
      searchAddon = null;
      dropGpu();
      fitFrame.cancel();
      ptyResize.cancel();
      if (copyTimer) clearTimeout(copyTimer);
      selectionDispose.dispose();
      menuSelectionDispose.dispose();
      unregisterCommands();
      host.removeEventListener("mouseup", onMouseUp);
      resizeObserver.disconnect();
      unlistens.forEach((u) => u());
      // A null layout (boot/recovery/HMR windows) proves nothing: only a
      // loaded layout missing the pane is a true close that may kill.
      const layout = store.layout;
      const stillPlaced =
        layout === null || findTabByPane(layout, sessionKey) !== null;
      if (!stillPlaced) {
        if (paneId !== null) onDispose?.(paneId);
        closeSession(sessionKey);
      }
      term.dispose();
    };
  });
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
    font: 12px var(--font-ui);
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

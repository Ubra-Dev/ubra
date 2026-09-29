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
  import { claimLiveId, dropLiveId, peekLiveId } from "./ptySessions";
  import { store } from "./store.svelte";
  import { withAlpha, type AppTheme } from "./themes";
  import { toasts } from "./toasts.svelte.ts";

  interface Props {
    /** Stable pane node id: reattaches to the surviving PTY across remounts. */
    sessionKey: string;
    cwd?: string;
    shell?: string;
    args?: string[];
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
    onSpawn?: (liveId: number) => void;
    onDispose?: (liveId: number) => void;
  }
  let {
    sessionKey,
    cwd,
    shell,
    args,
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
    let disposed = false;
    const unlistens: UnlistenFn[] = [];
    // Output/exit can arrive before pty_spawn resolves; buffer by pane id.
    const pending = new Map<number, string[]>();
    const pendingExits = new Map<
      number,
      { success: boolean; code: number | null }
    >();

    const handleExit = (success: boolean, code: number | null) => {
      // The live session is gone: a respawn remount must spawn fresh.
      if (paneId !== null) dropLiveId(sessionKey, paneId);
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
    const ensureFit = () => {
      if (
        disposed ||
        !container ||
        container.clientWidth < 10 ||
        container.clientHeight < 10
      )
        return;
      fit.fit();
      if (paneId !== null) {
        invoke("pty_resize", { id: paneId, cols: term.cols, rows: term.rows }).catch(
          console.error,
        );
      }
    };
    ensureFit();
    refit = ensureFit;

    const resizeObserver = new ResizeObserver(ensureFit);
    resizeObserver.observe(container!);

    (async () => {
      const outputUnlisten = await listen<{ id: number; data: string }>(
        "pty-output",
        (event) => {
          if (disposed) return;
          if (paneId === null) {
            const list = pending.get(event.payload.id) ?? [];
            list.push(event.payload.data);
            pending.set(event.payload.id, list);
          } else if (event.payload.id === paneId) {
            term.write(event.payload.data);
          }
        },
      );
      if (disposed) {
        outputUnlisten();
        return;
      }
      unlistens.push(outputUnlisten);

      const exitUnlisten = await listen<{
        id: number;
        success: boolean;
        code: number | null;
      }>("pty-exit", (event) => {
        if (disposed) return;
        if (paneId === null) {
          pendingExits.set(event.payload.id, {
            success: event.payload.success,
            code: event.payload.code,
          });
          return;
        }
        if (event.payload.id === paneId) {
          handleExit(event.payload.success, event.payload.code);
        }
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
      // Reattach: when this pane node already owns a live PTY (its component
      // remounted after a move across tabs/workspaces), reuse it instead of
      // spawning a fresh shell, and repaint from a backend screen snapshot.
      const existing = peekLiveId(sessionKey);
      if (existing !== null) {
        try {
          const snap = await invoke<string>("pty_snapshot", { id: existing });
          if (disposed) return;
          paneId = existing;
          onSpawn?.(paneId);
          // Snapshot first, then buffered output: anything already in
          // `pending` predates the snapshot and may duplicate a fragment,
          // but nothing is lost that arrived after it was taken.
          term.write(snap);
          for (const chunk of pending.get(paneId) ?? []) term.write(chunk);
          pending.clear();
          pendingExits.clear();
          ensureFit();
          return;
        } catch {
          dropLiveId(sessionKey, existing);
          if (disposed) return;
          // Session is gone (process exited while unmounted): fall through
          // to a fresh spawn below.
        }
      }
      paneId = await invoke<number>("pty_spawn", {
        shell: shell ?? null,
        cwd: cwd ?? null,
        args: args ?? null,
        cols: Math.max(term.cols, 2),
        rows: Math.max(term.rows, 2),
      });
      if (disposed) {
        invoke("pty_kill", { id: paneId }).catch(() => {});
        return;
      }
      claimLiveId(sessionKey, paneId);
      onSpawn?.(paneId);
      for (const chunk of pending.get(paneId) ?? []) term.write(chunk);
      pending.clear();
      const earlyExit = pendingExits.get(paneId);
      pendingExits.clear();
      if (earlyExit) handleExit(earlyExit.success, earlyExit.code);
      ensureFit();
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
      // A remount for a move keeps the node in the layout: leave the PTY and
      // its registry entry (and agent mapping) alive for the new terminal to
      // reattach to. Only a true close (node gone) kills. `paneId` can still
      // be null when unmount wins the race with spawn/attach; the registry
      // then holds the id to kill.
      const liveId = paneId ?? peekLiveId(sessionKey);
      if (liveId !== null) {
        const stillPlaced =
          store.layout !== null &&
          findTabByPane(store.layout, sessionKey) !== null;
        if (!stillPlaced) {
          dropLiveId(sessionKey, liveId);
          onDispose?.(liveId);
          invoke("pty_kill", { id: liveId }).catch(() => {});
        }
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
    font: 12px system-ui, sans-serif;
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

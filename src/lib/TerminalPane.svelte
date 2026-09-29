<script lang="ts">
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import "@xterm/xterm/css/xterm.css";
  import { onMount } from "svelte";
  import type { AppTheme } from "./themes";

  interface Props {
    cwd?: string;
    shell?: string;
    args?: string[];
    theme: AppTheme;
    fontSize: number;
    onExit?: () => void;
    onSpawn?: (liveId: number) => void;
    onDispose?: (liveId: number) => void;
  }
  let {
    cwd,
    shell,
    args,
    theme,
    fontSize,
    onExit,
    onSpawn,
    onDispose,
  }: Props = $props();

  let container: HTMLDivElement | undefined = $state();
  let terminal: Terminal | null = null;
  let refit: (() => void) | null = null;

  $effect(() => {
    const palette = theme.terminal;
    if (terminal) terminal.options.theme = { ...palette };
  });

  $effect(() => {
    const px = fontSize;
    if (terminal) {
      terminal.options.fontSize = px;
      // Refitting reflows and reports the new grid so the PTY resizes too.
      refit?.();
    }
  });

  onMount(() => {
    const term = new Terminal({
      cursorBlink: true,
      fontSize,
      fontFamily: "Menlo, Consolas, 'Courier New', monospace",
      theme: { ...theme.terminal },
    });
    terminal = term;
    const fit = new FitAddon();
    term.loadAddon(fit);
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
      term.write(
        `\r\n[process exited${success ? "" : ` (code ${code})`}]\r\n`,
      );
      onExit?.();
    };

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
      paneId = await invoke<number>("pty_spawn", {
        shell: shell ?? null,
        cwd: cwd ?? null,
        args: args ?? null,
        cols: Math.max(term.cols, 2),
        rows: Math.max(term.rows, 2),
      });
      onSpawn?.(paneId);
      for (const chunk of pending.get(paneId) ?? []) term.write(chunk);
      pending.clear();
      const earlyExit = pendingExits.get(paneId);
      pendingExits.clear();
      if (earlyExit) handleExit(earlyExit.success, earlyExit.code);
      ensureFit();
      if (disposed) {
        invoke("pty_kill", { id: paneId }).catch(() => {});
      }
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
      resizeObserver.disconnect();
      unlistens.forEach((u) => u());
      if (paneId !== null) {
        onDispose?.(paneId);
        invoke("pty_kill", { id: paneId }).catch(() => {});
      }
      term.dispose();
    };
  });
</script>

<div class="terminal" bind:this={container}></div>

<style>
  .terminal {
    width: 100%;
    height: 100%;
    background: var(--terminal-background);
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

<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { commandContext } from "./appCommandRuntime";
  import { NativeMenuController } from "./nativeMenus";
  import { isMacPlatform } from "./shortcuts";
  import { store } from "./store.svelte";
  import { terminalCommands } from "./terminalCommands";

  let controller = $state<NativeMenuController | null>(null);
  let revision = $state(0);
  let fullscreen = $state(false);

  $effect(() => {
    revision;
    controller?.sync({
      context: commandContext(),
      workspaces: store.layout?.workspaces.map((ws) => ({ id: ws.id, name: ws.name })) ?? [],
      activeWorkspaceId: store.layout?.activeWorkspaceId ?? "",
      fullscreen,
    });
  });

  onMount(() => {
    let disposed = false;
    const invalidate = () => { if (!disposed) revision += 1; };
    const menus = new NativeMenuController(isMacPlatform(navigator.platform), invalidate);
    controller = menus;
    void menus.start();
    const unsubscribe = terminalCommands.subscribe(invalidate);
    for (const type of ["focusin", "selectionchange", "input"]) document.addEventListener(type, invalidate);
    const observer = new MutationObserver((records) => {
      if (records.some((record) => [...record.addedNodes, ...record.removedNodes].some((node) =>
        node instanceof Element && (node.matches("[data-keyboard-overlay]") || node.querySelector("[data-keyboard-overlay]"))))) invalidate();
    });
    observer.observe(document.body, { childList: true, subtree: true });
    const win = getCurrentWindow();
    let query = 0;
    const updateFullscreen = async () => {
      const current = ++query;
      try {
        const value = await win.isFullscreen();
        if (!disposed && current === query) fullscreen = value;
      } catch (error) { console.error(error); }
    };
    void updateFullscreen();
    const unlisten = win.onResized(() => { void updateFullscreen(); });
    return () => {
      disposed = true;
      unsubscribe();
      observer.disconnect();
      for (const type of ["focusin", "selectionchange", "input"]) document.removeEventListener(type, invalidate);
      void unlisten.then((stop) => stop()).catch(console.error);
      void menus.dispose().catch(console.error);
    };
  });
</script>

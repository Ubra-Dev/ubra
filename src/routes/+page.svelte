<script lang="ts">
  import { onMount } from "svelte";
  import { agent } from "$lib/agent.svelte";
  import SettingsModal from "$lib/SettingsModal.svelte";
  import { isEditableTarget, matchShortcut } from "$lib/shortcuts";
  import { store } from "$lib/store.svelte";
  import Sidebar from "$lib/Sidebar.svelte";
  import TabBar from "$lib/TabBar.svelte";
  import TabCanvas from "$lib/TabCanvas.svelte";
  import Toasts from "$lib/Toasts.svelte";
  import { themeStyle } from "$lib/themes";

  onMount(() => {
    void store.boot();
    agent.start();
  });

  function onGlobalKeyDown(e: KeyboardEvent): void {
    const matched = matchShortcut({
      key: e.key,
      mod: e.metaKey || e.ctrlKey,
      shift: e.shiftKey,
      alt: e.altKey,
    });
    if (!matched) return;
    // Typing wins in rename inputs and selects — except settings, which opens
    // from anywhere. xterm's helper textarea is exempt: it looks editable but
    // terminal focus must not swallow app shortcuts.
    const ae = document.activeElement as HTMLElement | null;
    const inTerminal = ae?.closest?.(".xterm") != null;
    if (
      !inTerminal &&
      isEditableTarget(ae) &&
      matched.action !== "open-settings"
    )
      return;
    if (matched.action === "open-settings") {
      e.preventDefault();
      store.settingsOpen = !store.settingsOpen;
      return;
    }
    if (!store.loaded || !store.layout) return;
    switch (matched.action) {
      case "new-tab":
        store.addTab();
        break;
      case "close-pane-or-tab":
        store.closePaneOrTab();
        break;
      case "new-workspace":
        store.addWorkspace();
        break;
      case "split-right": {
        const cur = store.currentPane();
        if (cur) store.splitPane(cur.paneId, "row");
        break;
      }
      case "split-down": {
        const cur = store.currentPane();
        if (cur) store.splitPane(cur.paneId, "col");
        break;
      }
      case "toggle-zoom": {
        const cur = store.currentPane();
        if (cur) store.toggleZoomPane(cur.paneId);
        break;
      }
      case "prev-tab":
        store.cycleTab(-1);
        break;
      case "next-tab":
        store.cycleTab(1);
        break;
      case "jump-tab":
        if (matched.index !== undefined) store.jumpTab(matched.index);
        break;
      case "prev-workspace":
        store.cycleWorkspace(-1);
        break;
      case "next-workspace":
        store.cycleWorkspace(1);
        break;
      case "font-bigger":
        store.bumpTermFontSize(1);
        break;
      case "font-smaller":
        store.bumpTermFontSize(-1);
        break;
      case "font-reset":
        store.resetTermFontSize();
        break;
      case "rename-pane":
        store.requestPaneRename();
        break;
      default:
        return;
    }
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={onGlobalKeyDown} />

<div class="root" style={themeStyle(store.theme)}>
  {#if !store.loaded}
    <div class="loading">Loading Ubra&hellip;</div>
  {:else if store.layout}
    <div class="app">
      <Sidebar />
      <div class="main">
        <TabBar />
        <div class="tabs">
          {#each store.layout.workspaces as ws (ws.id)}
            <div class="ws" hidden={ws.id !== store.layout.activeWorkspaceId}>
              {#each ws.tabs as tab (tab.id)}
                <div class="tab" hidden={tab.id !== ws.activeTabId}>
                  <TabCanvas root={tab.root} zoomedId={tab.zoomedPaneId} />
                </div>
              {/each}
            </div>
          {/each}
        </div>
        {#if store.loadError}
          <div class="error">Layout load failed ({store.loadError}); started fresh.</div>
        {/if}
      </div>
    </div>
  {/if}
  {#if store.settingsOpen}
    <SettingsModal />
  {/if}
  <Toasts />
</div>

<style>
  :global(html, body) {
    margin: 0;
    height: 100%;
    background: transparent;
    overflow: hidden;
  }
  .root {
    width: 100vw;
    height: 100vh;
    background: var(--app-bg);
    color: var(--text);
    color-scheme: var(--color-scheme);
  }
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    color: var(--text-muted);
    font: 14px system-ui, sans-serif;
  }
  .app {
    display: flex;
    height: 100vh;
    width: 100vw;
  }
  .main {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .tabs {
    flex: 1 1 0;
    min-height: 0;
    padding: 4px;
  }
  .ws,
  .tab {
    height: 100%;
  }
  .ws[hidden],
  .tab[hidden] {
    display: none;
  }
  .error {
    padding: 4px 10px;
    background: var(--error-bg);
    color: var(--error-text);
    font: 12px system-ui, sans-serif;
  }
  :global(button:focus-visible, select:focus-visible, input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>

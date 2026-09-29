<script lang="ts">
  import { onMount } from "svelte";
  import { agent } from "$lib/agent.svelte";
  import ConfirmDialog from "$lib/ConfirmDialog.svelte";
  import FirstRun from "$lib/FirstRun.svelte";
  import SettingsModal from "$lib/SettingsModal.svelte";
  import { isEditableTarget, matchShortcutEvent, isMacPlatform } from "$lib/shortcuts";
  import { overlayFocus } from "$lib/overlayFocus";
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
    if (e.defaultPrevented) return;
    const overlayOpen = store.settingsOpen || !!store.pendingClose ||
      store.firstRun || store.onboardingOpen || store.recoveryRequired ||
      store.recoveryBusy || !!document.querySelector("[data-keyboard-overlay]");
    const matched = matchShortcutEvent(e, isMacPlatform(navigator.platform), overlayOpen);
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
        store.requestClosePaneOrTab();
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
      case "focus-neighbor":
        if (matched.dir) store.focusNeighbor(matched.dir);
        break;
      case "swap-neighbor":
        if (matched.dir) store.swapWithNeighbor(matched.dir);
        break;
      case "resize-pane":
        if (matched.dir) store.resizeFocused(matched.dir);
        break;
      case "move-pane-to-new-tab": {
        const cur = store.currentPane();
        if (cur) store.movePaneToNewTab(cur.paneId);
        break;
      }
      case "move-pane-to-new-workspace": {
        const cur = store.currentPane();
        if (cur) store.movePaneToNewWorkspace(cur.paneId);
        break;
      }
      case "find-in-pane":
        store.requestPaneFind();
        break;
      default:
        return;
    }
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={onGlobalKeyDown} />

<div class="root" style={themeStyle(store.theme, store.termOpacity / 100)}>
  {#if !store.loaded}
    <div class="loading">Loading Ubra&hellip;</div>
  {:else if store.recoveryRequired}
    <div class="recovery" role="alertdialog" aria-modal="true" aria-labelledby="recovery-title"
      tabindex="-1" data-keyboard-overlay use:overlayFocus>
      <div class="recovery-content">
        <h1 id="recovery-title">Recover your saved layout</h1>
        <p>Your saved layout could not be loaded. It has not been replaced.</p>
        <pre>{store.loadError}</pre>
        <p>Retry after repairing the file, export an exact copy, or reset.
          Reset backs up the original before replacing it with a fresh layout.</p>
        <div class="recovery-actions">
          <button disabled={store.recoveryBusy} onclick={() => void store.retryLayout()}>Retry</button>
          <button disabled={store.recoveryBusy} onclick={() => void store.exportRecoveryLayout()}>Export original</button>
          <button disabled={store.recoveryBusy} onclick={() => void store.resetRecoveryLayout()}>Back up and reset</button>
        </div>
        {#if store.recoveryError}<p role="alert">{store.recoveryError}</p>{/if}
        {#if store.recoveryBackupPath}<p class="backup">Original copied to: {store.recoveryBackupPath}</p>{/if}
      </div>
    </div>
  {:else if store.firstRun}
    <FirstRun />
  {:else if store.layout}
    <div class="app" inert={store.settingsOpen || !!store.pendingClose || store.onboardingOpen}>
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
        {#if store.saveError}
          <div class="error" role="alert">Layout save failed: {store.saveError}</div>
        {/if}
        {#if store.recoveryBackupPath}
          <div class="error backup">Original layout preserved at: {store.recoveryBackupPath}</div>
        {/if}
      </div>
    </div>
    {#if store.onboardingOpen}
      <FirstRun />
    {/if}
  {/if}
  {#if store.settingsOpen}
    <SettingsModal />
  {/if}
  <ConfirmDialog />
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
  .recovery {
    min-height: 100vh;
    display: grid;
    place-items: center;
    font: 14px/1.6 system-ui, sans-serif;
    overflow-y: auto;
  }
  .recovery-content {
    max-width: 620px;
    padding: 32px;
  }
  .recovery h1 { font-size: 22px; color: var(--text-strong); }
  .recovery pre, .backup { white-space: pre-wrap; overflow-wrap: anywhere; }
  .recovery pre { color: var(--text-muted); }
  .recovery-actions { display: flex; flex-wrap: wrap; gap: 8px; }
  .recovery button {
    background: var(--surface-bg);
    color: var(--text-strong);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 12px;
    cursor: pointer;
  }
  .recovery button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .recovery button:disabled { opacity: 0.5; cursor: wait; }
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

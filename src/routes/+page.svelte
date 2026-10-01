<script lang="ts">
  import { onMount } from "svelte";
  import { agent } from "$lib/agent.svelte";
  import ConfirmDialog from "$lib/ConfirmDialog.svelte";
  import FirstRun from "$lib/FirstRun.svelte";
  import SettingsModal from "$lib/SettingsModal.svelte";
  import { matchShortcutEvent, isMacPlatform } from "$lib/shortcuts";
  import NativeMenus from "$lib/NativeMenus.svelte";
  import { commandContext, dispatchCommand } from "$lib/appCommandRuntime";
  import { canDispatch, matchEditingShortcut, shouldDispatchDom, type CommandRequest } from "$lib/appCommands";
  import { nativeOwnsShortcut } from "$lib/nativeMenus";
  import { overlayFocus } from "$lib/overlayFocus";
  import { store } from "$lib/store.svelte";
  import RightSidebar from "$lib/RightSidebar.svelte";
  import Sidebar from "$lib/Sidebar.svelte";
  import StatusBar from "$lib/StatusBar.svelte";
  import TabBar from "$lib/TabBar.svelte";
  import TabCanvas from "$lib/TabCanvas.svelte";
  import Toasts from "$lib/Toasts.svelte";
  import { agentClis } from "$lib/agentClis.svelte";
  import { posthogLogs } from "$lib/posthogLogs";
  import { syncTelemetryToConsent } from "$lib/telemetrySync";
  import { workspaceGit } from "$lib/workspaceGit.svelte";
  import { themeStyle } from "$lib/themes";
  import { syncWindowTheme } from "$lib/windowTheme";
  import "$lib/uiFontFaces";
  import { uiFontStyle } from "$lib/uiFonts";
  import { uiZoomStyle } from "$lib/uiScale";

  onMount(() => {
    // Consent first: align the analytics client before any lifecycle events.
    void syncTelemetryToConsent().then(() => posthogLogs.applicationBooted());
    void store.boot();
    agent.start();
    workspaceGit.start();
    void agentClis.ensure();
    document.addEventListener("keydown", onGlobalKeyDown, true);
    return () => document.removeEventListener("keydown", onGlobalKeyDown, true);
  });

  // Keep the native titlebar on the app theme's scheme (boot + every pick).
  $effect(() => {
    void syncWindowTheme(store.theme.ui.colorScheme);
  });

  function onGlobalKeyDown(e: KeyboardEvent): void {
    if (e.defaultPrevented || e.isComposing) return;
    const isMac = isMacPlatform(navigator.platform);
    const context = commandContext();
    let matched: CommandRequest | null = matchShortcutEvent(e, isMac);
    if (!matched && !(isMac ? e.ctrlKey : e.metaKey)) {
      const action = matchEditingShortcut({ key: e.key, mod: isMac ? e.metaKey : e.ctrlKey,
        shift: e.shiftKey, alt: e.altKey }, isMac, context.terminalFocus);
      if (action) matched = { action };
    }
    if (!matched || matched.action === "switch-workspace" || !canDispatch(matched, context)) return;
    // Capture before xterm so app accelerators never become terminal input.
    e.preventDefault();
    const ordinaryTextEdit = !isMac && context.textFocus && !e.shiftKey &&
      ["copy", "paste", "select-all", "undo", "redo", "cut"].includes(matched.action);
    if (shouldDispatchDom(matched.action, !ordinaryTextEdit && nativeOwnsShortcut(matched.action), e.key === "+" && e.shiftKey)) {
      void dispatchCommand(matched);
    }
  }
</script>

<NativeMenus />

<div class="root" style={`${themeStyle(store.theme, store.termOpacity / 100)};${uiZoomStyle(store.uiScale)};${uiFontStyle(store.uiFontId)}`}>
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
    <div class="shell" inert={store.settingsOpen || !!store.pendingClose || store.onboardingOpen}>
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
        </div>
        <RightSidebar />
      </div>
      <StatusBar />
    </div>
    {#if store.layout.workspaces.length === 0}
      <FirstRun />
    {/if}
  {/if}
  {#if store.daemonConnected === false}
    <div class="daemon-banner" role="alert">
      <span>Agent runtime disconnected — reconnecting…</span>
      {#if store.daemonError}<span class="daemon-error">{store.daemonError}</span>{/if}
    </div>
  {/if}
  {#if store.settingsOpen}
    <SettingsModal />
  {/if}
  {#if store.onboardingOpen}
    <FirstRun revisit />
  {/if}
  <ConfirmDialog />
  <Toasts />
</div>

<style>
  :global(html, body) {
    margin: 0;
    height: 100%;
    /* Opaque: a transparent webview forces full-window blending every frame
       and makes resize sluggish on macOS. Matches the default theme until
       the app root paints. */
    background: #1a1a1a;
    overflow: hidden;
  }
  .root {
    --font-ui: system-ui, sans-serif;
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
    font: 14px/1.6 var(--font-ui);
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
    font: 14px var(--font-ui);
  }
  .daemon-banner {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 50;
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: center;
    padding: 6px 12px;
    background: var(--surface-bg);
    border-bottom: 1px solid var(--border);
    color: var(--text);
    font: 12px var(--font-ui);
  }
  .daemon-error {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 60vw;
  }
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
  }
  .app {
    display: flex;
    flex: 1 1 0;
    min-height: 0;
    min-width: 0;
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
  :global(button:focus-visible, select:focus-visible, input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>

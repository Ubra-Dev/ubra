<script lang="ts">
  import { onMount } from "svelte";
  import { agent } from "$lib/agent.svelte";
  import { store } from "$lib/store.svelte";
  import Sidebar from "$lib/Sidebar.svelte";
  import TabBar from "$lib/TabBar.svelte";
  import TabCanvas from "$lib/TabCanvas.svelte";
  import { themeStyle } from "$lib/themes";

  onMount(() => {
    void store.boot();
    agent.start();
  });
</script>

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

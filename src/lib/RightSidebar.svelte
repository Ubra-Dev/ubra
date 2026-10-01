<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import ExplorerView from "./ExplorerView.svelte";
  import Icon from "./Icon.svelte";
  import SourceControlView from "./SourceControlView.svelte";
  import { store } from "./store.svelte";

  const ws = $derived(store.workspace());
  const root = $derived(store.activeWorkspaceRoot());
  let picking = $state(false);
  let error = $state<string | null>(null);

  async function chooseRoot(): Promise<void> {
    const workspace = ws;
    if (!workspace || picking) return;
    picking = true;
    error = null;
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: "Choose a project folder",
      });
      if (typeof path === "string") store.setWorkspaceRoot(workspace.id, path);
    } catch (e) {
      console.error("ubra: project folder picker failed", e);
      error = "Couldn't open the folder picker. Try again.";
    } finally {
      picking = false;
    }
  }

  function showView(view: "explorer" | "source-control"): void {
    store.setRightPanelView(view);
    store.setRightPanelOpen(true);
  }
</script>

{#if store.layout && ws}
  {#if store.rightPanelOpen}
    <aside class="rightbar" aria-label="Explorer and source control">
      <div class="tabs" role="tablist" aria-label="Right sidebar views">
        <button
          role="tab"
          class:active={store.rightPanelView === "explorer"}
          aria-selected={store.rightPanelView === "explorer"}
          title="Explorer"
          onclick={() => store.setRightPanelView("explorer")}
        >
          <Icon name="folder" size={13} />
          <span>Explorer</span>
        </button>
        <button
          role="tab"
          class:active={store.rightPanelView === "source-control"}
          aria-selected={store.rightPanelView === "source-control"}
          title="Source Control"
          onclick={() => store.setRightPanelView("source-control")}
        >
          <Icon name="git-branch" size={13} />
          <span>Source Control</span>
        </button>
      </div>
      <div class="body">
        {#if root}
          {#key ws.id + root + store.rightPanelView}
            {#if store.rightPanelView === "explorer"}
              <ExplorerView root={root} />
            {:else}
              <SourceControlView root={root} />
            {/if}
          {/key}
        {:else}
          <div class="empty">
            <p>No folder open for this workspace.</p>
            <button
              class="choose"
              disabled={picking}
              onclick={chooseRoot}
            >
              {picking ? "Opening folders…" : "Choose a project folder"}
            </button>
            {#if error}
              <p class="error" role="alert">{error}</p>
            {/if}
          </div>
        {/if}
      </div>
    </aside>
  {:else}
    <div class="rail" aria-label="Right sidebar">
      <button
        title="Explorer"
        aria-label="Show Explorer"
        onclick={() => showView("explorer")}
      >
        <Icon name="folder" size={15} />
      </button>
      <button
        title="Source Control"
        aria-label="Show Source Control"
        onclick={() => showView("source-control")}
      >
        <Icon name="git-branch" size={15} />
      </button>
    </div>
  {/if}
{/if}

<style>
  .rightbar {
    display: flex;
    flex-direction: column;
    width: 250px;
    flex: 0 0 250px;
    min-height: 0;
    overflow: hidden;
    background: var(--sidebar-bg);
    border-left: 1px solid var(--border);
    font: 12px system-ui, sans-serif;
    color: var(--text);
    user-select: none;
  }
  .tabs {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 2px;
    padding: 8px 6px 6px;
    border-bottom: 1px solid var(--border);
  }
  .tabs button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 1 1 0;
    min-width: 0;
    gap: 6px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-muted);
    font: inherit;
    padding: 5px 6px;
    cursor: pointer;
    white-space: nowrap;
  }
  .tabs button span {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tabs button:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .tabs button.active {
    color: var(--text-strong);
    background: var(--surface-active);
  }
  .body {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
  }
  .empty {
    padding: 18px 14px;
    color: var(--text-muted);
    line-height: 1.5;
  }
  .empty p {
    margin: 0 0 12px;
  }
  .choose {
    background: var(--surface-bg);
    color: var(--text-strong);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 12px;
    font: inherit;
    cursor: pointer;
  }
  .choose:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .choose:disabled {
    opacity: 0.6;
    cursor: wait;
  }
  .empty .error {
    color: var(--danger, #f87171);
    font-size: 11px;
  }
  .rail {
    display: flex;
    flex: 0 0 40px;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding-top: 10px;
    background: var(--sidebar-bg);
    border-left: 1px solid var(--border);
  }
  .rail button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .rail button:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
</style>

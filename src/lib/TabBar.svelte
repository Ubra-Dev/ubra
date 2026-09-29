<script lang="ts">
  import { agent } from "./agent.svelte";
  import ContextMenu, { announceMenuOpen } from "./ContextMenu.svelte";
  import { store } from "./store.svelte";

  let editing = $state<string | null>(null);
  let draft = $state("");
  let menu = $state<{ id: string; x: number; y: number } | null>(null);

  function focus(el: HTMLInputElement): void {
    el.focus();
    el.select();
  }

  function commitRename(id: string): void {
    if (editing === id) {
      store.renameTab(id, draft);
      editing = null;
    }
  }

  function openMenu(e: MouseEvent, id: string): void {
    e.preventDefault();
    e.stopPropagation();
    if (editing) commitRename(editing);
    announceMenuOpen();
    menu = { id, x: e.clientX, y: e.clientY };
  }

  function onPick(action: string): void {
    const m = menu;
    menu = null;
    if (!m) return;
    if (action === "rename") {
      const tab = store.workspace()?.tabs.find((t) => t.id === m.id);
      if (tab) {
        editing = tab.id;
        draft = tab.name;
      }
    } else if (action === "close") {
      store.closeTab(m.id);
    }
  }
</script>

{#if store.layout}
  {@const ws = store.workspace()}
  {#if ws}
    <div class="tabbar">
      {#each ws.tabs as tab (tab.id)}
        {@const rollup = agent.tabRollup(tab)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="tab"
          class:active={tab.id === ws.activeTabId}
          oncontextmenu={(e) => openMenu(e, tab.id)}
        >
          {#if editing === tab.id}
            <input
              use:focus
              bind:value={draft}
              onblur={() => commitRename(tab.id)}
              onkeydown={(e) => {
                if (e.key === "Enter") commitRename(tab.id);
                if (e.key === "Escape") editing = null;
              }}
            />
          {:else}
            <button
              class="name"
              onclick={() => store.switchTab(tab.id)}
              ondblclick={() => {
                editing = tab.id;
                draft = tab.name;
              }}
            >
              {tab.name}
            </button>
          {/if}
          {#if rollup !== "idle"}
            <span class={"dot " + rollup}></span>
          {/if}
          <button
            class="close"
            title="Close tab"
            onclick={() => store.closeTab(tab.id)}
          >
            &times;
          </button>
        </div>
      {/each}
      <button class="add" title="New tab" onclick={() => store.addTab()}>+</button>
    </div>
    {#if menu}
      <ContextMenu
        x={menu.x}
        y={menu.y}
        items={[
          { id: "rename", label: "Rename" },
          { id: "close", label: "Close", danger: true },
        ]}
        onPick={onPick}
        onDismiss={() => (menu = null)}
      />
    {/if}
  {/if}
{/if}

<style>
  .tabbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px;
    background: var(--tabbar-bg);
    font: 12px system-ui, sans-serif;
    user-select: none;
  }
  .tab {
    display: flex;
    align-items: center;
    background: var(--surface-bg);
    border-radius: 6px;
    color: var(--text);
    max-width: 180px;
  }
  .tab.active {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    padding: 5px 4px 5px 10px;
    cursor: pointer;
    text-align: left;
  }
  .tab input {
    width: 120px;
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 4px;
    color: var(--text-strong);
    font: inherit;
    padding: 3px 6px;
    margin: 2px 0 2px 4px;
  }
  .close {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 13px;
    padding: 4px 8px 4px 4px;
    cursor: pointer;
    border-radius: 4px;
  }
  .close:hover {
    color: var(--text-strong);
  }
  .add {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 15px;
    padding: 2px 8px;
    cursor: pointer;
    border-radius: 4px;
  }
  .add:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .dot {
    flex: 0 0 auto;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin-right: 2px;
  }
  .dot.working {
    background: var(--success);
  }
  .dot.attention {
    background: var(--attention);
  }
</style>

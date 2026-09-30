<script lang="ts">
  import { tick } from "svelte";
  import { agent } from "./agent.svelte";
  import ContextMenu, { announceMenuOpen } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";
  import { isMacPlatform, modLabel } from "./shortcuts";
  import { store } from "./store.svelte";

  let editing = $state<string | null>(null);
  let draft = $state("");
  let menu = $state<{ id: string; x: number; y: number; opener: HTMLElement | null } | null>(null);
  const mod = modLabel(isMacPlatform(navigator.platform));
  let tabList = $state<HTMLDivElement | null>(null);

  function revealActive(): void {
    tabList?.querySelector<HTMLElement>(".tab.active")
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }

  $effect(() => {
    const active = store.workspace()?.activeTabId;
    if (active && tabList) void tick().then(revealActive);
  });

  function trackViewport(node: HTMLDivElement) {
    const observer = new ResizeObserver(revealActive);
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }

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
    menu = { id, x: e.clientX, y: e.clientY,
      opener: (e.currentTarget as HTMLElement).querySelector<HTMLElement>(".name") };
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
      store.requestCloseTab(m.id);
    }
  }
</script>

{#if store.layout}
  {@const ws = store.workspace()}
  {#if ws}
    <div class="tabbar">
      <div class="tab-list" bind:this={tabList} use:trackViewport>
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
          {#if rollup === "done"}
            <span class="done-check" title="done">
              <Icon name="check" size={10} />
            </span>
          {:else if rollup !== "idle"}
            <span class={"dot " + rollup}></span>
          {/if}
          <button
            class="close"
            title={`Close tab (${mod}W closes the pane)`}
            onclick={() => store.requestCloseTab(tab.id)}
          >
            <Icon name="x" size={12} />
          </button>
        </div>
      {/each}
        <button
          class="add"
          title={`New tab (${mod}T)`}
          onclick={() => store.addTab()}
        >
          <Icon name="plus" size={13} />
        </button>
      </div>
      <button
        class="toggle"
        class:on={store.leftPanelOpen}
        title={store.leftPanelOpen ? "Hide left sidebar" : "Show left sidebar"}
        aria-label={store.leftPanelOpen ? "Hide left sidebar" : "Show left sidebar"}
        aria-pressed={store.leftPanelOpen}
        onclick={() => store.setLeftPanelOpen(!store.leftPanelOpen)}
      >
        <Icon name="sidebar-left" size={13} />
      </button>
      <button
        class="toggle"
        class:on={store.rightPanelOpen}
        title={store.rightPanelOpen ? "Hide right sidebar" : "Show right sidebar"}
        aria-label={store.rightPanelOpen ? "Hide right sidebar" : "Show right sidebar"}
        aria-pressed={store.rightPanelOpen}
        onclick={() => store.setRightPanelOpen(!store.rightPanelOpen)}
      >
        <Icon name="sidebar-right" size={13} />
      </button>
    </div>
    {#if menu}
      <ContextMenu
        x={menu.x}
        y={menu.y}
        opener={menu.opener}
        items={[
          { id: "rename", label: "Rename", icon: "edit" },
          { id: "close", label: "Close", danger: true, icon: "x" },
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
    min-width: 0;
    padding: 6px 8px;
    background: var(--tabbar-bg);
    font: 12px var(--font-ui);
    user-select: none;
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    flex: 0 0 auto;
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 4px 6px;
    cursor: pointer;
    border-radius: 4px;
  }
  .toggle:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .toggle.on {
    color: var(--accent);
  }
  .tab-list {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1 1 auto;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
    overscroll-behavior-x: contain;
  }
  .tab {
    display: flex;
    align-items: center;
    background: var(--surface-bg);
    border-radius: 6px;
    color: var(--text);
    max-width: 180px;
    flex: 0 0 auto;
    min-width: 96px;
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
    display: inline-flex;
    align-items: center;
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 4px 8px 4px 4px;
    cursor: pointer;
    border-radius: 4px;
  }
  .close:hover {
    color: var(--text-strong);
  }
  .add {
    display: inline-flex;
    align-items: center;
    flex: 0 0 auto;
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 4px 8px;
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
  .dot.blocked {
    background: var(--error-text);
  }
  .dot.unknown {
    background: var(--text-subtle);
  }
  .dot.attention {
    background: var(--attention);
  }
  .done-check {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--success);
    margin-right: 2px;
  }
</style>

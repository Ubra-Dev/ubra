<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { agent } from "./agent.svelte";
  import ContextMenu, { announceMenuOpen } from "./ContextMenu.svelte";
  import { store } from "./store.svelte";
  import { THEMES, THEME_IDS, isThemeId } from "./themes";

  let editing = $state<string | null>(null);
  let draft = $state("");
  let autostart = $state(false);
  let autostartLoaded = $state(false);
  const agents = $derived(agent.activeAgents());

  function focus(el: HTMLInputElement): void {
    el.focus();
    el.select();
  }

  function commitRename(id: string): void {
    if (editing === id) {
      store.renameWorkspace(id, draft);
      editing = null;
    }
  }

  let menu = $state<{ id: string; x: number; y: number } | null>(null);

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
    if (!m || !store.layout) return;
    if (action === "rename") {
      const ws = store.layout.workspaces.find((w) => w.id === m.id);
      if (ws) {
        editing = ws.id;
        draft = ws.name;
      }
    } else if (action === "close") {
      store.closeWorkspace(m.id);
    }
  }

  onMount(() => {
    invoke<boolean>("autostart_enabled")
      .then((v) => {
        autostart = v;
        autostartLoaded = true;
      })
      .catch((e) => console.error("ubra: autostart check failed", e));
  });

  function onAutostartChange(e: Event): void {
    const checked = (e.target as HTMLInputElement).checked;
    autostart = checked;
    invoke("autostart_set", { enabled: checked }).catch((err) => {
      console.error("ubra: autostart update failed", err);
      autostart = !checked;
    });
  }

  function onThemeChange(e: Event): void {
    const value = (e.target as HTMLSelectElement).value;
    if (isThemeId(value)) store.setTheme(value);
  }
</script>

{#if store.layout}
  <aside class="sidebar">
    <div class="section">Workspaces</div>
    {#if agent.attention.length > 0}
      <button class="attention" onclick={() => agent.jumpToAttention()}>
        {agent.attention.length}
        {agent.attention.length === 1 ? "needs" : "need"} attention
      </button>
    {/if}
    {#each store.layout.workspaces as ws (ws.id)}
      {@const rollup = agent.workspaceRollup(ws)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="ws"
        class:active={ws.id === store.layout.activeWorkspaceId}
        oncontextmenu={(e) => openMenu(e, ws.id)}
      >
        {#if editing === ws.id}
          <input
            use:focus
            bind:value={draft}
            onblur={() => commitRename(ws.id)}
            onkeydown={(e) => {
              if (e.key === "Enter") commitRename(ws.id);
              if (e.key === "Escape") editing = null;
            }}
          />
        {:else}
          <button
            class="name"
            onclick={() => store.switchWorkspace(ws.id)}
            ondblclick={() => {
              editing = ws.id;
              draft = ws.name;
            }}
          >
            {ws.name}
            {#if rollup !== "idle"}
              <span class={"dot " + rollup}></span>
            {/if}
          </button>
        {/if}
        <button
          class="close"
          title="Close workspace"
          onclick={() => store.closeWorkspace(ws.id)}
        >
          &times;
        </button>
      </div>
    {/each}
    <button class="add" onclick={() => store.addWorkspace()}>+ Workspace</button>
    <div class="section agents">Agents</div>
    {#if agents.length === 0}
      <div class="none">No active agents</div>
    {:else}
      {#each agents as g (g.wsId)}
        <div class="agent-ws">{g.wsName}</div>
        {#each g.agents as a (a.nodeId)}
          <button class="agent-row" onclick={() => agent.jumpToPane(a.nodeId)}>
            <span class={"dot " + a.status}></span>
            <span class="agent-name">{a.agent}</span>
            <span class="agent-status">
              {a.status === "working" ? "working" : "needs review"}
            </span>
            <span class="agent-loc">{a.paneTitle ?? a.tabName}</span>
          </button>
        {/each}
      {/each}
    {/if}
    <div class="footer">
      <label class="theme-setting">
        <span>Theme</span>
        <select value={store.themeId} onchange={onThemeChange} aria-label="App theme">
          {#each THEME_IDS as id}
            <option value={id}>{THEMES[id].name}</option>
          {/each}
        </select>
      </label>
      <label class="autostart-setting">
        <input
          type="checkbox"
          checked={autostart}
          disabled={!autostartLoaded}
          onchange={onAutostartChange}
        />
        Launch at login
      </label>
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
  </aside>
{/if}

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 190px;
    flex: 0 0 190px;
    background: var(--sidebar-bg);
    padding: 10px 8px;
    font: 12px system-ui, sans-serif;
    color: var(--text);
    user-select: none;
  }
  .section {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    padding: 0 6px 6px;
  }
  .section.agents {
    margin-top: 10px;
  }
  .ws {
    display: flex;
    align-items: center;
    border-radius: 6px;
  }
  .ws.active {
    background: var(--surface-active);
  }
  .name {
    flex: 1 1 auto;
    min-width: 0;
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: 6px;
    border-radius: 6px;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close {
    flex: 0 0 auto;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 13px;
    padding: 4px 8px;
    cursor: pointer;
    border-radius: 4px;
  }
  .close:hover {
    color: var(--text-strong);
  }
  .ws.active .name {
    color: var(--text-strong);
  }
  .ws input {
    width: 100%;
    box-sizing: border-box;
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 4px;
    color: var(--text-strong);
    font: inherit;
    padding: 4px 6px;
  }
  .add {
    margin-top: 6px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font: inherit;
    text-align: left;
    padding: 6px;
    border-radius: 6px;
    cursor: pointer;
  }
  .add:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .none {
    color: var(--text-subtle);
    padding: 2px 6px;
  }
  .agent-ws {
    color: var(--text-muted);
    font-size: 11px;
    padding: 4px 6px 1px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .agent-row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    box-sizing: border-box;
    background: transparent;
    border: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    padding: 4px 6px;
    border-radius: 6px;
    cursor: pointer;
    white-space: nowrap;
  }
  .agent-row:hover {
    background: var(--surface-bg);
    color: var(--text-strong);
  }
  .agent-row .dot {
    margin-left: 0;
    flex: 0 0 auto;
  }
  .agent-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .agent-status,
  .agent-loc {
    flex: 0 0 auto;
    color: var(--text-subtle);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .agent-loc {
    margin-left: auto;
    max-width: 62px;
  }
  .footer {
    margin-top: auto;
    padding: 8px 6px 0;
    border-top: 1px solid var(--border);
    color: var(--text-muted);
  }
  .theme-setting {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 5px;
    margin-bottom: 10px;
    color: var(--text-muted);
  }
  .theme-setting select {
    width: 100%;
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 5px 7px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
  }
  .autostart-setting {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .attention {
    background: var(--attention-bg);
    border: none;
    color: var(--attention);
    font: inherit;
    text-align: left;
    padding: 6px;
    border-radius: 6px;
    cursor: pointer;
    margin-bottom: 4px;
  }
  .attention:hover {
    background: var(--attention-hover-bg);
  }
  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin-left: 6px;
    vertical-align: baseline;
  }
  .dot.working {
    background: var(--success);
  }
  .dot.attention {
    background: var(--attention);
  }
</style>

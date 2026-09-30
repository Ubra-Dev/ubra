<script lang="ts">
  import { agent } from "./agent.svelte";
  import ContextMenu, { announceMenuOpen } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";
  import { isMacPlatform, modLabel } from "./shortcuts";
  import { store } from "./store.svelte";

  let editing = $state<string | null>(null);
  let draft = $state("");
  const agents = $derived(agent.activeAgents());
  const mod = modLabel(isMacPlatform(navigator.platform));

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

  let menu = $state<{ id: string; x: number; y: number; opener: HTMLElement | null } | null>(null);

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
    if (!m || !store.layout) return;
    if (action === "rename") {
      const ws = store.layout.workspaces.find((w) => w.id === m.id);
      if (ws) {
        editing = ws.id;
        draft = ws.name;
      }
    } else if (action === "save-template") {
      store.openSavedSetups(m.id);
    } else if (action === "close") {
      store.requestCloseWorkspace(m.id);
    }
  }

  // Workspace drag-reorder (HTML5 DnD). The drop position is the hovered
  // row's top/bottom half, shown as an insertion line.
  let dragId = $state<string | null>(null);
  let dropId = $state<string | null>(null);
  let dropPos = $state<"before" | "after" | null>(null);

  function dropPosition(e: DragEvent): "before" | "after" {
    const el = e.currentTarget as HTMLElement;
    const rect = el.getBoundingClientRect();
    return e.clientY > rect.top + rect.height / 2 ? "after" : "before";
  }

  function onDragStart(e: DragEvent, id: string): void {
    dragId = id;
    dropId = null;
    dropPos = null;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", id);
    }
  }

  function onDragOver(e: DragEvent, id: string): void {
    if (!dragId || dragId === id) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    dropId = id;
    dropPos = dropPosition(e);
  }

  function onDrop(e: DragEvent, id: string): void {
    e.preventDefault();
    const source = dragId;
    const pos = dropId === id && dropPos ? dropPos : dropPosition(e);
    dragId = null;
    dropId = null;
    dropPos = null;
    if (source && source !== id) store.moveWorkspace(source, id, pos);
  }

  function onDragEnd(): void {
    dragId = null;
    dropId = null;
    dropPos = null;
  }

  function onDragLeave(id: string): void {
    if (dropId === id) {
      dropId = null;
      dropPos = null;
    }
  }

</script>

{#if store.layout}
  <aside class="sidebar">
    <div class="navigation-scroll">
    <div class="section"><Icon name="layers" size={12} /> Workspaces</div>
    {#if agent.attention.length + agent.done.length > 0}
      {@const reviewCount = agent.attention.length + agent.done.length}
      <button class="attention" onclick={() => agent.jumpToReview()}>
        <Icon name="alert" size={12} />
        <span>
          {reviewCount}
          {reviewCount === 1 ? "needs" : "need"} review
        </span>
      </button>
    {/if}
    {#each store.layout.workspaces as ws (ws.id)}
      {@const rollup = agent.workspaceRollup(ws)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="ws"
        class:active={ws.id === store.layout.activeWorkspaceId}
        class:dragging={dragId === ws.id}
        class:drop-before={dropId === ws.id && dropPos === "before"}
        class:drop-after={dropId === ws.id && dropPos === "after"}
        draggable={editing !== ws.id}
        oncontextmenu={(e) => openMenu(e, ws.id)}
        ondragstart={(e) => onDragStart(e, ws.id)}
        ondragover={(e) => onDragOver(e, ws.id)}
        ondrop={(e) => onDrop(e, ws.id)}
        ondragend={onDragEnd}
        ondragleave={() => onDragLeave(ws.id)}
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
            {#if rollup === "done"}
              <span class="done-check" title="done">
                <Icon name="check" size={10} />
              </span>
            {:else if rollup !== "idle"}
              <span class={"dot " + rollup}></span>
            {/if}
          </button>
        {/if}
        <button
          class="close"
          title="Close workspace"
          onclick={() => store.requestCloseWorkspace(ws.id)}
        >
          <Icon name="x" size={12} />
        </button>
      </div>
    {/each}
    <div class="section agents"><Icon name="cpu" size={12} /> Agents</div>
    {#if agents.length === 0}
      <div class="none">
        <Icon name="cpu" size={12} />
        <span>No active agents</span>
      </div>
    {:else}
      {#each agents as g (g.wsId)}
        <div class="agent-ws">{g.wsName}</div>
        {#each g.agents as a (a.nodeId)}
          <div class="agent-item">
            <button
              class="agent-row"
              title={a.dirPath
                ? `${a.statusTitle} — ${a.dirPath} — ${a.paneTitle ?? a.tabName}`
                : `${a.statusTitle} — ${a.paneTitle ?? a.tabName}`}
              onclick={() => agent.jumpToPane(a.nodeId)}
            >
              {#if a.status === "done"}
                <span class="done-check" title={a.statusTitle}>
                  <Icon name="check" size={10} />
                </span>
              {:else}
                <span
                  class={"dot " + a.status}
                  title={a.statusTitle}
                ></span>
              {/if}
              <span class="agent-name">{a.dir}</span>
              {#if a.cli}
                <span class="agent-cli">{a.cli}</span>
              {/if}
            </button>
            <button
              class="close"
              title={`Close ${a.dir} pane`}
              aria-label={`Close ${a.dir} pane`}
              onclick={(e) => {
                e.stopPropagation();
                store.requestClosePane(a.nodeId);
              }}
            >
              <Icon name="x" size={12} />
            </button>
          </div>
        {/each}
      {/each}
    {/if}
    </div>
    <div class="footer">
    <div class="add-workspace">
      <button
        class="add"
        title={`New workspace (${mod}N)`}
        onclick={() => store.addWorkspace()}
      >
        <Icon name="plus" size={12} />
        <span>Workspace</span>
      </button>
      <button
        class="add-grid"
        title="New workspace with 4 terminals (2×2)"
        aria-label="New workspace with 4 terminals in a 2 by 2 grid"
        onclick={() => store.addWorkspace(true)}
      >
        <Icon name="grid" size={14} />
      </button>
    </div>
      <button
        class="settings-btn"
        title={`Settings (${mod},)`}
        onclick={() => (store.settingsOpen = true)}
      >
        <Icon name="settings" size={13} />
        <span>Settings</span>
      </button>
      <button
        class="settings-btn"
        title="Saved setups"
        onclick={() => store.openSavedSetups()}
      >
        <Icon name="layers" size={13} />
        <span>Saved setups</span>
      </button>
    {#if menu}
      <ContextMenu
        x={menu.x}
        y={menu.y}
        opener={menu.opener}
        items={[
          { id: "rename", label: "Rename", icon: "edit" },
          { id: "save-template", label: "Save as template", icon: "layers" },
          { id: "close", label: "Close", danger: true, icon: "x" },
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
    min-height: 0;
    overflow: hidden;
    background: var(--sidebar-bg);
    padding: 10px 8px;
    font: 12px system-ui, sans-serif;
    color: var(--text);
    user-select: none;
  }
  .navigation-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: thin;
    overscroll-behavior: contain;
  }
  .section {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    padding: 0 6px 6px;
  }
  .section.agents {
    margin-top: 8px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .ws {
    position: relative;
    display: flex;
    align-items: center;
    border-radius: 6px;
  }
  .ws.active {
    background: var(--surface-active);
  }
  .ws.dragging {
    opacity: 0.5;
  }
  .ws.drop-before::before,
  .ws.drop-after::after {
    content: "";
    position: absolute;
    left: 4px;
    right: 4px;
    height: 2px;
    border-radius: 1px;
    background: var(--accent);
    pointer-events: none;
  }
  .ws.drop-before::before {
    top: -2px;
  }
  .ws.drop-after::after {
    bottom: -2px;
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
    display: inline-flex;
    align-items: center;
    flex: 0 0 auto;
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 4px 8px;
    cursor: pointer;
    border-radius: 4px;
    opacity: 0;
    pointer-events: none;
  }
  .ws:hover .close,
  .ws:focus-within .close,
  .agent-item:hover .close,
  .agent-item:focus-within .close {
    opacity: 1;
    pointer-events: auto;
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
  .add-workspace {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-top: 6px;
  }
  .add-grid {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 28px;
    height: 28px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0;
    pointer-events: none;
  }
  .add-workspace:hover .add-grid,
  .add-workspace:focus-within .add-grid {
    opacity: 1;
    pointer-events: auto;
  }
  .add-grid:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .add-grid:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  @media (hover: none) {
    .add-grid {
      opacity: 1;
      pointer-events: auto;
    }
  }
  .add {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 6px;
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
    display: flex;
    align-items: center;
    gap: 6px;
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
  .agent-item {
    display: flex;
    align-items: center;
    min-width: 0;
    border-radius: 6px;
  }
  .agent-row {
    display: flex;
    align-items: center;
    flex: 1 1 auto;
    min-width: 0;
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
  .agent-row:hover,
  .agent-item:focus-within .agent-row {
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
  .agent-cli {
    flex: 0 0 auto;
    margin-left: auto;
    max-width: 96px;
    color: var(--text-subtle);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .footer {
    margin-top: auto;
    flex: 0 0 auto;
    padding: 8px 6px 0;
    border-top: 1px solid var(--border);
    color: var(--text-muted);
  }
  .settings-btn {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    box-sizing: border-box;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font: inherit;
    text-align: left;
    padding: 6px;
    border-radius: 6px;
    cursor: pointer;
  }
  .settings-btn:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .attention {
    display: flex;
    align-items: center;
    gap: 6px;
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
  .dot.blocked {
    background: var(--error-text);
  }
  .dot.unknown {
    background: var(--text-subtle);
  }
  .dot.attention {
    background: var(--attention);
  }
  .dot.idle {
    background: transparent;
    border: 1px solid var(--text-subtle);
    box-sizing: border-box;
  }
  .done-check {
    display: inline-flex;
    color: var(--success);
    margin-left: 6px;
    vertical-align: -1px;
  }
  .agent-row .done-check {
    margin-left: 0;
    flex: 0 0 auto;
  }
</style>

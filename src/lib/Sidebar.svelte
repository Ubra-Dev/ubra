<script lang="ts">
  import { agent } from "./agent.svelte";
  import ContextMenu, { announceMenuOpen } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";
  import { isMacPlatform, modLabel } from "./shortcuts";
  import {
    MAX_SIDEBAR_WIDTH,
    MIN_SIDEBAR_WIDTH,
    effectiveSplitRatio,
    ratioFromPointer,
    stepSidebarWidth,
    stepSplitRatio,
  } from "./sidebarResize";
  import { store } from "./store.svelte";
  import { workspaceDir } from "./workspaceGit";
  import { workspaceGit } from "./workspaceGit.svelte";

  let editing = $state<string | null>(null);
  let draft = $state("");
  const agents = $derived(agent.activeAgents());
  const mod = modLabel(isMacPlatform(navigator.platform));

  // Refresh branch subtitles when workspace directories change.
  $effect(() => {
    if (!store.layout) return;
    for (const ws of store.layout.workspaces) workspaceDir(ws);
    void workspaceGit.refresh();
  });

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

  // Sidebar resize: width handle on the outer edge, split divider between
  // the workspaces and agents panes. Both drag with the pointer and step
  // with the keyboard; sizes persist in the store.
  let splitEl = $state<HTMLDivElement | null>(null);
  let splitHeight = $state(0);
  const splitRatio = $derived(effectiveSplitRatio(store.sidebarSplit, splitHeight));
  let splitDrag: { top: number; height: number } | null = null;
  let widthDrag: { startX: number; startWidth: number } | null = null;

  function primaryButton(e: PointerEvent): boolean {
    return e.pointerType !== "mouse" || e.button === 0;
  }

  function startSplitDrag(e: PointerEvent): void {
    if (!splitEl || !primaryButton(e)) return;
    const rect = splitEl.getBoundingClientRect();
    splitDrag = { top: rect.top, height: rect.height };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function moveSplitDrag(e: PointerEvent): void {
    if (!splitDrag) return;
    store.setSidebarSplit(
      effectiveSplitRatio(
        ratioFromPointer(e.clientY, splitDrag.top, splitDrag.height),
        splitDrag.height,
      ),
    );
  }

  function endSplitDrag(): void {
    splitDrag = null;
  }

  function onSplitKey(e: KeyboardEvent): void {
    const height = splitEl?.clientHeight ?? 0;
    if (e.key === "ArrowUp" || e.key === "ArrowLeft") {
      e.preventDefault();
      store.setSidebarSplit(
        effectiveSplitRatio(stepSplitRatio(store.sidebarSplit, -1), height),
      );
    } else if (e.key === "ArrowDown" || e.key === "ArrowRight") {
      e.preventDefault();
      store.setSidebarSplit(
        effectiveSplitRatio(stepSplitRatio(store.sidebarSplit, 1), height),
      );
    } else if (e.key === "Home") {
      e.preventDefault();
      store.setSidebarSplit(effectiveSplitRatio(0, height));
    } else if (e.key === "End") {
      e.preventDefault();
      store.setSidebarSplit(effectiveSplitRatio(1, height));
    }
  }

  function startWidthDrag(e: PointerEvent): void {
    if (!primaryButton(e)) return;
    widthDrag = { startX: e.clientX, startWidth: store.sidebarWidth };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function moveWidthDrag(e: PointerEvent): void {
    if (!widthDrag) return;
    store.setSidebarWidth(widthDrag.startWidth + (e.clientX - widthDrag.startX));
  }

  function endWidthDrag(): void {
    widthDrag = null;
  }

  function onWidthKey(e: KeyboardEvent): void {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      store.setSidebarWidth(stepSidebarWidth(store.sidebarWidth, -1));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      store.setSidebarWidth(stepSidebarWidth(store.sidebarWidth, 1));
    } else if (e.key === "Home") {
      e.preventDefault();
      store.setSidebarWidth(MIN_SIDEBAR_WIDTH);
    } else if (e.key === "End") {
      e.preventDefault();
      store.setSidebarWidth(MAX_SIDEBAR_WIDTH);
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
  <aside class="sidebar" style="width: {store.sidebarWidth}px">
    <div class="split" bind:this={splitEl} bind:clientHeight={splitHeight}>
      <section class="pane" aria-label="Workspaces" style:flex-grow={splitRatio}>
        <div class="section"><Icon name="layers" size={12} /> Workspaces</div>
        <div class="pane-scroll">
    {#each store.layout.workspaces as ws (ws.id)}
      {@const rollup = agent.workspaceRollup(ws)}
      {@const branch = workspaceGit.branchFor(ws.id)}
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
            title={branch ? `${ws.name} — ${branch}` : ws.name}
            onclick={() => store.switchWorkspace(ws.id)}
            ondblclick={() => {
              editing = ws.id;
              draft = ws.name;
            }}
          >
            <span class="ws-name">
              <span class="ws-title">{ws.name}</span>
              {#if rollup === "done"}
                <span class="done-check" title="done">
                  <Icon name="check" size={10} />
                </span>
              {:else if rollup !== "idle"}
                <span class={"dot " + rollup}></span>
              {/if}
            </span>
            {#if branch}
              <span class="ws-branch">
                <Icon name="git-branch" size={10} />
                <span class="ws-branch-name">{branch}</span>
              </span>
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
    <div class="add-workspace">
      <button
        class="add"
        title={`New workspace (${mod}N)`}
        onclick={() => store.addWorkspace()}
      >
        <Icon name="plus" size={12} />
        <span>New</span>
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
        </div>
      </section>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div
        class="divider"
        role="separator"
        aria-orientation="horizontal"
        aria-label="Resize workspaces and agents panes"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(splitRatio * 100)}
        tabindex="0"
        onpointerdown={startSplitDrag}
        onpointermove={moveSplitDrag}
        onpointerup={endSplitDrag}
        onpointercancel={endSplitDrag}
        onkeydown={onSplitKey}
      ></div>
      <section class="pane agents" aria-label="Agents" style:flex-grow={1 - splitRatio}>
        <div class="section agents"><Icon name="cpu" size={12} /> Agents</div>
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
        <div class="pane-scroll">
    {#if agents.length === 0}
      <div class="none">No active agents</div>
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
      </section>
    </div>
    <div class="footer">
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
        title="Saved Setups"
        onclick={() => store.openSavedSetups()}
      >
        <Icon name="layers" size={13} />
        <span>Saved Setups</span>
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
    </div>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="resize-x"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sidebar width"
      aria-valuemin={MIN_SIDEBAR_WIDTH}
      aria-valuemax={MAX_SIDEBAR_WIDTH}
      aria-valuenow={store.sidebarWidth}
      tabindex="0"
      onpointerdown={startWidthDrag}
      onpointermove={moveWidthDrag}
      onpointerup={endWidthDrag}
      onpointercancel={endWidthDrag}
      onkeydown={onWidthKey}
    ></div>
  </aside>
{/if}

<style>
  .sidebar {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 0 0 auto;
    min-height: 0;
    overflow: hidden;
    background: var(--sidebar-bg);
    padding: 10px 8px;
    font: 12px var(--font-ui);
    color: var(--text);
    user-select: none;
  }
  .split {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .pane {
    flex: 1 1 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .divider {
    flex: 0 0 7px;
    margin: -3px 0;
    cursor: ns-resize;
    touch-action: none;
    position: relative;
    z-index: 1;
  }
  .divider::after {
    content: "";
    display: block;
    height: 1px;
    margin-top: 3px;
    background: var(--border);
  }
  .divider:hover::after,
  .divider:focus-visible::after,
  .divider:active::after {
    background: var(--accent);
  }
  .resize-x {
    position: absolute;
    top: 0;
    bottom: 0;
    right: -3px;
    width: 8px;
    cursor: ew-resize;
    touch-action: none;
    z-index: 1;
  }
  .resize-x::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 3px;
    width: 1px;
  }
  .resize-x:hover::after,
  .resize-x:focus-visible::after,
  .resize-x:active::after {
    background: var(--accent);
  }
  .pane-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: thin;
    overscroll-behavior: contain;
  }
  .section {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    padding: 0 6px 6px;
  }
  .section.agents {
    padding-top: 10px;
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
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 1px;
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: 6px;
    border-radius: 6px;
    cursor: pointer;
    overflow: hidden;
  }
  .ws-name {
    display: flex;
    align-items: center;
    min-width: 0;
  }
  .ws-title {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ws-name .dot,
  .ws-name .done-check {
    flex: 0 0 auto;
  }
  .ws-branch {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    font-size: 11px;
    color: var(--text-subtle);
  }
  .ws-branch-name {
    flex: 1 1 auto;
    min-width: 0;
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
    flex: 0 0 auto;
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
    color: var(--text-subtle);
    opacity: 0.6;
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
    flex: 0 0 auto;
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

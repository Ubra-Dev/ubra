<script lang="ts">
  import ContextMenu, { announceMenuOpen } from "./ContextMenu.svelte";
  import TerminalPane from "./TerminalPane.svelte";
  import { agent } from "./agent.svelte";
  import { store } from "./store.svelte";
  import type { PaneNode } from "./layout";

  let { node, zoomed = false }: { node: PaneNode; zoomed?: boolean } =
    $props();

  let runId = $state(0);
  let exited = $state(false);
  let editing = $state(false);
  let draft = $state("");
  let menu = $state<{ x: number; y: number } | null>(null);
  const agentState = $derived(agent.paneState(node.id));

  const title = $derived(
    node.title ?? node.cwd?.split("/").filter(Boolean).pop() ?? "Terminal",
  );

  function focus(el: HTMLInputElement): void {
    el.focus();
    el.select();
  }

  function commitRename(): void {
    if (editing) {
      store.renamePane(node.id, draft);
      editing = false;
    }
  }

  function openMenu(e: MouseEvent): void {
    e.preventDefault();
    e.stopPropagation();
    if (editing) commitRename();
    announceMenuOpen();
    menu = { x: e.clientX, y: e.clientY };
  }

  function onPick(action: string): void {
    menu = null;
    if (action === "rename") {
      editing = true;
      draft = node.title ?? "";
    } else if (action === "zoom") {
      store.toggleZoomPane(node.id);
    } else if (action === "close") {
      store.closePane(node.id);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="pane-view" oncontextmenu={openMenu}>
  <div class="pane-header">
    {#if editing}
      <input
        class="rename"
        use:focus
        bind:value={draft}
        onblur={commitRename}
        onkeydown={(e) => {
          if (e.key === "Enter") commitRename();
          if (e.key === "Escape") editing = false;
        }}
      />
    {:else}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <span
        class="title"
        ondblclick={() => {
          editing = true;
          draft = node.title ?? "";
        }}
      >
        {title}
      </span>
    {/if}
    {#if agentState?.state === "working"}
      <span class="agent">{agentState.agent}</span>
    {/if}
    {#if zoomed}
      <button
        class="zoomed"
        title="Exit zoom"
        onclick={() => store.toggleZoomPane(node.id)}
      >
        Zoomed
      </button>
    {/if}
    <span class="actions">
      <button title="Split right" onclick={() => store.splitPane(node.id, "row")}>
        Split &rarr;
      </button>
      <button title="Split down" onclick={() => store.splitPane(node.id, "col")}>
        Split &darr;
      </button>
      <button title="Close pane" onclick={() => store.closePane(node.id)}>&times;</button>
    </span>
  </div>
  <div class="term-wrap">
    {#key runId}
      <TerminalPane
        cwd={node.cwd}
        shell={node.cmd?.[0]}
        args={node.cmd?.slice(1)}
        theme={store.theme}
        onExit={() => (exited = true)}
        onSpawn={(id) => agent.register(id, node.id)}
        onDispose={(id) => agent.unregister(id)}
      />
    {/key}
    {#if exited}
      <button
        class="respawn"
        onclick={() => {
          exited = false;
          runId += 1;
        }}
      >
        Respawn shell
      </button>
    {/if}
  </div>
  {#if menu}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      items={[
        { id: "rename", label: "Rename" },
        { id: "zoom", label: zoomed ? "Unzoom" : "Zoom" },
        { id: "close", label: "Close", danger: true },
      ]}
      onPick={onPick}
      onDismiss={() => (menu = null)}
    />
  {/if}
</div>

<style>
  .pane-view {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--pane-bg);
    border-radius: 6px;
    overflow: hidden;
  }
  .pane-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    height: 26px;
    flex: 0 0 26px;
    padding: 0 4px 0 10px;
    background: var(--pane-header-bg);
    font: 12px system-ui, sans-serif;
    color: var(--text);
    user-select: none;
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-right: auto;
  }
  .rename {
    width: 140px;
    margin-right: auto;
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 4px;
    color: var(--text-strong);
    font: inherit;
    padding: 1px 6px;
  }
  .agent {
    font-size: 11px;
    color: var(--agent-text);
    background: var(--agent-bg);
    border: none;
    padding: 1px 8px;
    border-radius: 8px;
    white-space: nowrap;
    cursor: pointer;
  }
  .zoomed {
    border: 0;
    border-radius: 4px;
    padding: 2px 7px;
    background: var(--agent-bg);
    color: var(--agent-text);
    font: inherit;
    cursor: pointer;
  }
  .zoomed:hover {
    background: var(--surface-hover);
  }
  .actions {
    display: flex;
    gap: 2px;
    opacity: 0;
  }
  .pane-view:hover .actions {
    opacity: 1;
  }
  .actions button {
    background: transparent;
    border: none;
    color: var(--text);
    font-size: 11px;
    padding: 2px 6px;
    border-radius: 4px;
    cursor: pointer;
  }
  .actions button:hover {
    background: var(--surface-hover);
    color: var(--text-strong);
  }
  .term-wrap {
    position: relative;
    flex: 1 1 0;
    min-height: 0;
  }
  .respawn {
    position: absolute;
    top: 8px;
    right: 12px;
    background: var(--accent);
    color: var(--text-strong);
    border: none;
    font-size: 12px;
    padding: 4px 12px;
    border-radius: 4px;
    cursor: pointer;
  }
  .respawn:hover {
    background: var(--accent-hover);
  }
</style>

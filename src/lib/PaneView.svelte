<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ContextMenu, { announceMenuOpen } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";
  import TerminalPane from "./TerminalPane.svelte";
  import { agent } from "./agent.svelte";
  import { agentClis } from "./agentClis.svelte";
  import { agentStatusLabel } from "./agentStatus";
  import { isMacPlatform, modLabel } from "./shortcuts";
  import { store } from "./store.svelte";
  import { forgetSession } from "./ptySessions";
  import { paneDisplayTitle, type PaneNode } from "./layout";
  import { toasts } from "./toasts.svelte.ts";

  let {
    node,
    zoomed = false,
    onHeaderPointerDown,
  }: {
    node: PaneNode;
    zoomed?: boolean;
    onHeaderPointerDown?: (paneId: string, e: PointerEvent) => void;
  } = $props();

  const isMac = isMacPlatform(navigator.platform);
  const mod = modLabel(isMac);

  let runId = $state(0);
  let exited = $state(false);
  let editing = $state(false);
  let draft = $state("");
  let menu = $state<{ x: number; y: number; opener: HTMLElement | null } | null>(null);

  // Split-button agent picker: hovering a split button opens a menu of
  // detected agent CLIs to run in the new pane. Click still splits with
  // the default shell; ArrowDown opens the menu from the keyboard.
  const SPLIT_MENU_OPEN_MS = 400;
  const SPLIT_MENU_CLOSE_MS = 200;
  let splitMenu = $state<{
    dir: "row" | "col";
    x: number;
    y: number;
    opener: HTMLElement | null;
  } | null>(null);
  let hoverOpenTimer: ReturnType<typeof setTimeout> | null = null;
  let hoverCloseTimer: ReturnType<typeof setTimeout> | null = null;
  const hasAgentClis = $derived((agentClis.clis?.length ?? 0) > 0);

  function cancelHoverOpen(): void {
    if (hoverOpenTimer !== null) {
      clearTimeout(hoverOpenTimer);
      hoverOpenTimer = null;
    }
  }

  function cancelHoverClose(): void {
    if (hoverCloseTimer !== null) {
      clearTimeout(hoverCloseTimer);
      hoverCloseTimer = null;
    }
  }

  function dismissSplitMenu(): void {
    cancelHoverOpen();
    cancelHoverClose();
    splitMenu = null;
  }

  function openSplitMenu(target: HTMLElement, dir: "row" | "col"): void {
    if (!hasAgentClis) {
      void agentClis.ensure();
      return;
    }
    const rect = target.getBoundingClientRect();
    announceMenuOpen();
    splitMenu = { dir, x: rect.left, y: rect.bottom + 4, opener: target };
  }

  function scheduleHoverClose(): void {
    cancelHoverClose();
    hoverCloseTimer = setTimeout(() => {
      hoverCloseTimer = null;
      splitMenu = null;
    }, SPLIT_MENU_CLOSE_MS);
  }

  function splitButtonEnter(e: PointerEvent, dir: "row" | "col"): void {
    if (e.pointerType !== "mouse") return;
    cancelHoverClose();
    if (splitMenu) {
      if (splitMenu.dir !== dir) {
        openSplitMenu(e.currentTarget as HTMLElement, dir);
      }
      return;
    }
    cancelHoverOpen();
    const target = e.currentTarget as HTMLElement;
    hoverOpenTimer = setTimeout(() => {
      hoverOpenTimer = null;
      openSplitMenu(target, dir);
    }, SPLIT_MENU_OPEN_MS);
  }

  function splitButtonLeave(e: PointerEvent): void {
    if (e.pointerType !== "mouse") return;
    cancelHoverOpen();
    if (splitMenu) scheduleHoverClose();
  }

  function onSplitPick(id: string): void {
    const picked = splitMenu;
    dismissSplitMenu();
    if (picked) store.splitPaneWithCommand(node.id, picked.dir, id);
  }

  function onSplitKey(e: KeyboardEvent, dir: "row" | "col"): void {
    if (e.key !== "ArrowDown") return;
    e.preventDefault();
    dismissSplitMenu();
    openSplitMenu(e.currentTarget as HTMLElement, dir);
  }
  const agentLabel = $derived(agent.paneAgentLabel(node.id));
  const agentStatus = $derived(agent.paneStatus(node.id));

  // F2 rename: the matching pane takes the request and clears it.
  $effect(() => {
    if (store.paneRenameTarget === node.id) {
      editing = true;
      draft = node.title ?? "";
      store.paneRenameTarget = null;
    }
  });

  // Focus requests: bump the token so the terminal takes keyboard focus.
  let focusToken = $state(0);
  $effect(() => {
    if (store.paneFocusTarget === node.id) {
      store.paneFocusTarget = null;
      focusToken += 1;
    }
  });

  // Find requests: bump the token so the terminal opens its find bar.
  let findToken = $state(0);
  $effect(() => {
    if (store.paneFindTarget === node.id) {
      store.paneFindTarget = null;
      findToken += 1;
    }
  });

  const title = $derived(paneDisplayTitle(node));

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

  function headerPointerDown(e: PointerEvent): void {
    // Drags start from bare header only: buttons/inputs keep their behavior,
    // and right-clicks still open the context menu.
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button, input")) return;
    onHeaderPointerDown?.(node.id, e);
  }

  function onTerminalSpawn(live: { id: number; epoch: number; incarnation: number }): void {
    agent.register(live.id, node.id);
    const command = store.takePendingTerminalCommand(node.id);
    if (!command) return;
    invoke("pty_write", {
      id: live.id,
      data: `${command}\r`,
      epoch: live.epoch,
      incarnation: live.incarnation,
    }).catch((error) => {
      console.error("ubra: failed to start onboarding command", error);
      toasts.push(
        "Couldn't send the agent command",
        "Enter it in the terminal to try again.",
        node.id,
      );
    });
  }

  function openMenu(e: MouseEvent): void {
    e.preventDefault();
    e.stopPropagation();
    if (editing) commitRename();
    announceMenuOpen();
    menu = { x: e.clientX, y: e.clientY,
      opener: (e.currentTarget as HTMLElement).querySelector<HTMLElement>(".xterm-helper-textarea") };
  }

  function onPick(action: string): void {
    menu = null;
    if (action === "rename") {
      editing = true;
      draft = node.title ?? "";
    } else if (action === "zoom") {
      store.toggleZoomPane(node.id);
    } else if (action === "find") {
      findToken += 1;
    } else if (action === "move-tab") {
      store.movePaneToNewTab(node.id);
    } else if (action === "move-workspace") {
      store.movePaneToNewWorkspace(node.id);
    } else if (action === "close") {
      store.requestClosePane(node.id);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="pane-view"
  data-pane-id={node.id}
  oncontextmenu={openMenu}
  onfocusin={() => {
    store.focusPane(node.id);
    agent.acknowledge(node.id);
  }}
  onpointerdown={() => store.focusPane(node.id)}
>
  <div class="pane-header" onpointerdown={headerPointerDown}>
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
    {#if agentLabel}
      <span class={"agent " + agentStatus} title={agent.paneStatusTitle(node.id)}>
        {agentLabel} · {agentStatusLabel(agentStatus)}
      </span>
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
      <button
        title={hasAgentClis ? `Split right (${mod}D) — hover to pick an agent` : `Split right (${mod}D)`}
        onclick={() => {
          dismissSplitMenu();
          store.splitPane(node.id, "row");
        }}
        onpointerenter={(e) => splitButtonEnter(e, "row")}
        onpointerleave={splitButtonLeave}
        onkeydown={(e) => onSplitKey(e, "row")}
      >
        <Icon name="columns" size={12} />
      </button>
      <button
        title={hasAgentClis ? `Split down (${mod}${isMac ? "⇧" : "Shift+"}D) — hover to pick an agent` : `Split down (${mod}${isMac ? "⇧" : "Shift+"}D)`}
        onclick={() => {
          dismissSplitMenu();
          store.splitPane(node.id, "col");
        }}
        onpointerenter={(e) => splitButtonEnter(e, "col")}
        onpointerleave={splitButtonLeave}
        onkeydown={(e) => onSplitKey(e, "col")}
      >
        <Icon name="rows" size={12} />
      </button>
      <button
        title={`Close pane (${mod}W)`}
        onclick={() => store.requestClosePane(node.id)}
      >
        <Icon name="x" size={12} />
      </button>
    </span>
  </div>
  <div class="term-wrap">
    {#key runId}
      <TerminalPane
        sessionKey={node.id}
        cwd={node.cwd}
        theme={store.theme}
        fontSize={store.termFontSize}
        opacity={store.termOpacity / 100}
        focusToken={focusToken}
        findToken={findToken}
        scrollback={store.termScrollback}
        onExit={() => (exited = true)}
        onSpawn={onTerminalSpawn}
        onDispose={(id) => agent.unregister(id)}
      />
    {/key}
    {#if exited}
      <button
        class="respawn"
        onclick={() => {
          // Close retained state first so the remount spawns fresh instead
          // of re-rendering the retained exit.
          invoke("pty_close", { key: node.id })
            .catch((error) => console.error(error))
            .finally(() => {
              forgetSession(node.id);
              store.authorizePaneCommand(node.id);
              exited = false;
              runId += 1;
            });
        }}
      >
        <Icon name="refresh" size={12} />
        <span>{node.cmd?.length && node.cmdOnRestore === false ? "Run saved command" : "Restart terminal"}</span>
      </button>
    {/if}
  </div>
  {#if menu}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      opener={menu.opener}
      items={[
        { id: "rename", label: "Rename", icon: "edit" },
        {
          id: "zoom",
          label: zoomed ? "Unzoom" : "Zoom",
          icon: zoomed ? "minimize" : "maximize",
        },
        { id: "find", label: "Find in pane", icon: "search" },
        { id: "move-tab", label: "Move to new tab", icon: "external" },
        { id: "move-workspace", label: "Move to new workspace", icon: "layers" },
        { id: "close", label: "Close", danger: true, icon: "x" },
      ]}
      onPick={onPick}
      onDismiss={() => (menu = null)}
    />
  {/if}
  {#if splitMenu}
    <ContextMenu
      x={splitMenu.x}
      y={splitMenu.y}
      opener={splitMenu.opener}
      items={(agentClis.clis ?? []).map((entry) => ({
        id: entry.cli,
        label: `${entry.label} · ${entry.cli}`,
        cli: entry.cli,
      }))}
      onPick={onSplitPick}
      onDismiss={dismissSplitMenu}
      onHoverChange={(inside) => {
        if (inside) cancelHoverClose();
        else scheduleHoverClose();
      }}
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
    min-height: 26px;
    flex: 0 0 auto;
    padding: 0 4px 0 10px;
    background: var(--pane-header-bg);
    font: calc(12px * var(--ui-text-scale, 1)) var(--font-ui);
    color: var(--text);
    user-select: none;
    cursor: grab;
    touch-action: none;
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
    font-size: calc(11px * var(--ui-text-scale, 1));
    color: var(--agent-text);
    background: var(--agent-bg);
    border: none;
    padding: 1px 8px;
    border-radius: 8px;
    white-space: nowrap;
    cursor: pointer;
  }
  .agent.blocked {
    color: var(--error-text);
    background: var(--error-bg);
  }
  .agent.attention {
    color: var(--attention);
    background: var(--attention-bg);
  }
  .agent.done {
    color: var(--success);
  }
  .agent.idle,
  .agent.unknown {
    color: var(--text-muted);
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
  .pane-view:hover .actions,
  .pane-view:focus-within .actions {
    opacity: 1;
  }
  .actions button {
    display: inline-flex;
    align-items: center;
    background: transparent;
    border: none;
    color: var(--text);
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
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--accent);
    color: var(--on-accent);
    border: none;
    font-size: calc(12px * var(--ui-text-scale, 1));
    padding: 4px 12px;
    border-radius: 4px;
    cursor: pointer;
  }
  .respawn:hover {
    background: var(--accent-hover);
  }
</style>

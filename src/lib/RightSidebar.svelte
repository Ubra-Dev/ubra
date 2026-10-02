<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { fly } from "svelte/transition";
  import ExplorerView from "./ExplorerView.svelte";
  import Icon from "./Icon.svelte";
  import { MOTION_MED_MS, motionMs } from "./motion";
  import NotesView from "./NotesView.svelte";
  import {
    MAX_SIDEBAR_WIDTH,
    MIN_SIDEBAR_WIDTH,
    stepSidebarWidth,
  } from "./sidebarResize";
  import SourceControlView from "./SourceControlView.svelte";
  import { frameCoalescer } from "./schedule";
  import { store, type RightPanelView } from "./store.svelte";

  const ws = $derived(store.workspace());
  const root = $derived(store.activeWorkspaceRoot());
  const panelLabel = $derived(
    store.rightPanelView === "explorer"
      ? "Explorer"
      : store.rightPanelView === "source-control"
        ? "Source Control"
        : "Notes",
  );
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

  // Width resize mirrors the left sidebar: the handle sits on the panel's
  // inner (left) edge, so dragging left widens and ArrowLeft steps wider.
  let widthDrag: { startX: number; startWidth: number } | null = null;

  function primaryButton(e: PointerEvent): boolean {
    return e.pointerType !== "mouse" || e.button === 0;
  }

  function startWidthDrag(e: PointerEvent): void {
    if (!primaryButton(e)) return;
    widthDrag = { startX: e.clientX, startWidth: store.rightPanelWidth };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  // The drag coalesces to one layout update per frame and persists once
  // on release, so pointermove bursts never queue layout + storage work.
  const widthFrame = frameCoalescer();

  function moveWidthDrag(e: PointerEvent): void {
    if (!widthDrag) return;
    const width = widthDrag.startWidth - (e.clientX - widthDrag.startX);
    widthFrame.schedule(() => store.setRightPanelWidthLive(width));
  }

  function endWidthDrag(): void {
    widthDrag = null;
    widthFrame.flush();
    store.saveRightPanelWidth();
  }

  function onWidthKey(e: KeyboardEvent): void {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      store.setRightPanelWidth(stepSidebarWidth(store.rightPanelWidth, 1));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      store.setRightPanelWidth(stepSidebarWidth(store.rightPanelWidth, -1));
    } else if (e.key === "Home") {
      e.preventDefault();
      store.setRightPanelWidth(MIN_SIDEBAR_WIDTH);
    } else if (e.key === "End") {
      e.preventDefault();
      store.setRightPanelWidth(MAX_SIDEBAR_WIDTH);
    }
  }

  const views: { id: RightPanelView; title: string; icon: "folder" | "git-branch" | "book" }[] = [
    { id: "explorer", title: "Explorer", icon: "folder" },
    { id: "source-control", title: "Source Control", icon: "git-branch" },
    { id: "notes", title: "Notes", icon: "book" },
  ];
</script>

{#if store.layout && ws}
  <div class="rightwrap">
    {#if store.rightPanelOpen}
      <!-- Transform-only so show/hide animates without touching the drag-resized width. -->
      <aside
        class="rightbar"
        aria-label={panelLabel}
        style="width: {store.rightPanelWidth}px"
        transition:fly={{ x: 16, duration: motionMs(MOTION_MED_MS) }}
      >
        <div class="body">
          {#if store.rightPanelView === "notes"}
            {#key ws.id}
              <NotesView workspaceId={ws.id} />
            {/key}
          {:else if root}
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
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <div
          class="resize-x"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize right sidebar width"
          aria-valuemin={MIN_SIDEBAR_WIDTH}
          aria-valuemax={MAX_SIDEBAR_WIDTH}
          aria-valuenow={store.rightPanelWidth}
          tabindex="0"
          onpointerdown={startWidthDrag}
          onpointermove={moveWidthDrag}
          onpointerup={endWidthDrag}
          onpointercancel={endWidthDrag}
          onkeydown={onWidthKey}
        ></div>
      </aside>
    {/if}
    <div class="rail" role="tablist" aria-label="Right sidebar views">
      {#each views as view (view.id)}
        <button
          role="tab"
          title={view.title}
          aria-label={view.title}
          aria-selected={store.rightPanelOpen && store.rightPanelView === view.id}
          class:active={store.rightPanelOpen && store.rightPanelView === view.id}
          onclick={() => store.toggleRightPanelView(view.id)}
        >
          <Icon name={view.icon} size={15} />
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .rightwrap {
    display: flex;
    flex: 0 0 auto;
    min-height: 0;
  }
  .rightbar {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 0 0 auto;
    min-height: 0;
    overflow: hidden;
    background: var(--sidebar-bg);
    border-left: 1px solid var(--border);
    font: 12px system-ui, sans-serif;
    color: var(--text);
    user-select: none;
  }
  .resize-x {
    position: absolute;
    top: 0;
    bottom: 0;
    left: -3px;
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
    right: 3px;
    width: 1px;
  }
  .resize-x:hover::after,
  .resize-x:focus-visible::after,
  .resize-x:active::after {
    background: var(--accent);
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
  .rail button.active {
    color: var(--text-strong);
    background: var(--surface-active);
  }
</style>

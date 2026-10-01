<script lang="ts">
  import PaneView from "./PaneView.svelte";
  import { frameCoalescer } from "./schedule";
  import { store } from "./store.svelte";
  import {
    computeLayout,
    findPane,
    findPaneAtPoint,
    findSplit,
    type LayoutNode,
  } from "./layout";

  let { root, zoomedId }: { root: LayoutNode; zoomedId?: string } = $props();

  let el: HTMLDivElement | undefined = $state();
  // Keyed by stable pane-node ids: reshapes move DOM nodes but never remount
  // (and kill) live terminals. See computeLayout. Zoom keeps every pane
  // mounted — hidden siblings keep their PTYs alive — and shows only the
  // zoomed one full-tab.
  const placed = $derived.by(() => {
    const layout = computeLayout(root);
    const z = zoomedId ? findPane(root, zoomedId) : null;
    if (!z) return { ...layout, zoomed: null as string | null };
    return {
      panes: layout.panes.map((p) =>
        p.node.id === z.id
          ? {
              node: p.node,
              rect: [0, 0, 1, 1] as [number, number, number, number],
            }
          : p,
      ),
      dividers: [],
      zoomed: z.id as string | null,
    };
  });

  function setRatio(splitId: string, ratio: number): void {
    const split = findSplit(root, splitId);
    if (!split) return;
    const clamped = Math.min(0.9, Math.max(0.1, ratio));
    split.sizes = [clamped, 1 - clamped];
  }

  // Divider drags coalesce to one layout update per frame; the canvas box
  // is measured once at grab time so moves never force sync layout.
  const dividerFrame = frameCoalescer();

  function onDividerPointerDown(
    e: PointerEvent,
    splitId: string,
    dir: "row" | "col",
    rect: [number, number, number, number],
  ): void {
    e.preventDefault();
    if (!el) return;
    const box = el.getBoundingClientRect();
    const [x, y, w, h] = rect;
    const left = box.left + box.width * x;
    const top = box.top + box.height * y;
    const span = dir === "row" ? box.width * w : box.height * h;
    const origin = dir === "row" ? left : top;
    const move = (ev: PointerEvent) => {
      const point = dir === "row" ? ev.clientX : ev.clientY;
      const ratio = span > 0 ? (point - origin) / span : 0.5;
      dividerFrame.schedule(() => setRatio(splitId, ratio));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      dividerFrame.flush();
      store.paneSizesChanged();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  // Titlebar drag-drop: the dragged pane swaps slots with the drop target.
  // Plain clicks never activate (5px threshold), so rename/focus are safe.
  let dragSource = $state<string | null>(null);
  let dragTarget = $state<string | null>(null);

  function onHeaderPointerDown(paneId: string, e: PointerEvent): void {
    if (placed.zoomed !== null) return;
    const startX = e.clientX;
    const startY = e.clientY;
    let active = false;
    const move = (ev: PointerEvent) => {
      if (!active) {
        if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < 5) return;
        active = true;
        dragSource = paneId;
        document.body.classList.add("pane-dragging");
      }
      if (!el) return;
      const box = el.getBoundingClientRect();
      const fx =
        box.width > 0 ? (ev.clientX - box.left) / box.width : Number.NaN;
      const fy =
        box.height > 0 ? (ev.clientY - box.top) / box.height : Number.NaN;
      const hit = findPaneAtPoint(root, fx, fy);
      dragTarget = hit && hit.id !== paneId ? hit.id : null;
    };
    const cleanup = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("keydown", cancel);
      document.body.classList.remove("pane-dragging");
    };
    const up = () => {
      const target = active ? dragTarget : null;
      cleanup();
      dragSource = null;
      dragTarget = null;
      if (target) store.swapPanes(paneId, target);
    };
    const cancel = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape") return;
      cleanup();
      dragSource = null;
      dragTarget = null;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("keydown", cancel);
  }

  function onDividerKeyDown(e: KeyboardEvent, splitId: string): void {
    if (!e.key.startsWith("Arrow")) return;
    e.preventDefault();
    const split = findSplit(root, splitId);
    if (!split) return;
    const step = e.key === "ArrowLeft" || e.key === "ArrowUp" ? -0.05 : 0.05;
    setRatio(splitId, split.sizes[0] + step);
    store.paneSizesChanged();
  }

  function pct(n: number): string {
    return `${n * 100}%`;
  }
</script>

<div class="canvas" bind:this={el}>
  {#each placed.panes as pane, i (pane.node.id)}
    <div
      class="slot"
      class:drag-source={dragSource === pane.node.id}
      class:drop-target={dragTarget === pane.node.id}
      class:active={placed.zoomed === null &&
        placed.panes.length > 1 &&
        store.focusedPaneId === pane.node.id}
      hidden={placed.zoomed !== null && pane.node.id !== placed.zoomed}
      style:left={pct(pane.rect[0])}
      style:top={pct(pane.rect[1])}
      style:width={pct(pane.rect[2])}
      style:height={pct(pane.rect[3])}
      style:animation-delay={`${Math.min(i, 5) * 28}ms`}
    >
      <PaneView
        node={pane.node}
        zoomed={pane.node.id === placed.zoomed}
        onHeaderPointerDown={onHeaderPointerDown}
      />
    </div>
  {/each}
  {#each placed.dividers as div (div.splitId)}
    {@const vertical = div.dir === "row"}
    {@const pos = vertical
      ? div.rect[0] + div.rect[2] * div.at
      : div.rect[1] + div.rect[3] * div.at}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="divider"
      class:vertical
      role="separator"
      tabindex="0"
      aria-orientation={vertical ? "vertical" : "horizontal"}
      aria-valuenow={Math.round(div.at * 100)}
      style:left={vertical ? pct(pos) : pct(div.rect[0])}
      style:top={vertical ? pct(div.rect[1]) : pct(pos)}
      style:width={vertical ? "6px" : pct(div.rect[2])}
      style:height={vertical ? pct(div.rect[3]) : "6px"}
      onpointerdown={(e) =>
        onDividerPointerDown(e, div.splitId, div.dir, div.rect)}
      onkeydown={(e) => onDividerKeyDown(e, div.splitId)}
    ></div>
  {/each}
</div>

<style>
  .canvas {
    position: relative;
    width: 100%;
    height: 100%;
  }
  .slot {
    position: absolute;
    padding: 3px;
    box-sizing: border-box;
    outline: 1px solid var(--border);
    outline-offset: -1px;
    border-radius: 6px;
  }
  .slot[hidden] {
    display: none;
  }
  /* Workspace reveal: panes fade in with a per-slot stagger (see the inline
     animation-delay). The `both` fill holds the first frame through the
     delay so slots appear in sequence instead of flashing at full opacity. */
  :global(.ws-enter) .slot {
    animation: ubra-ws-slot-in 160ms ease-out both;
  }
  @keyframes ubra-ws-slot-in {
    from {
      opacity: 0;
      transform: translateY(5px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.ws-enter) .slot {
      animation: none;
    }
  }
  .slot.drag-source {
    opacity: 0.55;
  }
  .slot.drop-target {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
    border-radius: 6px;
  }
  .slot.active {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
    border-radius: 6px;
  }
  :global(body.pane-dragging),
  :global(body.pane-dragging *) {
    cursor: grabbing !important;
  }
  .divider {
    position: absolute;
    border-radius: 2px;
    z-index: 2;
    background: transparent;
    touch-action: none;
  }
  .divider::after {
    content: "";
    position: absolute;
    background: var(--separator);
    border-radius: 1px;
    pointer-events: none;
  }
  .divider.vertical {
    transform: translateX(-50%);
    cursor: col-resize;
  }
  .divider.vertical::after {
    top: 0;
    bottom: 0;
    left: 50%;
    width: 1px;
    transform: translateX(-50%);
  }
  .divider:not(.vertical) {
    transform: translateY(-50%);
    cursor: row-resize;
  }
  .divider:not(.vertical)::after {
    top: 50%;
    left: 0;
    right: 0;
    height: 1px;
    transform: translateY(-50%);
  }
  .divider:hover,
  .divider:focus-visible {
    background: var(--accent);
    outline: none;
  }
</style>

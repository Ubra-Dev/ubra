<script lang="ts">
  import PaneView from "./PaneView.svelte";
  import { store } from "./store.svelte";
  import {
    computeLayout,
    findPane,
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

  function onDividerPointerDown(
    e: PointerEvent,
    splitId: string,
    dir: "row" | "col",
    rect: [number, number, number, number],
  ): void {
    e.preventDefault();
    const move = (ev: PointerEvent) => {
      if (!el) return;
      const box = el.getBoundingClientRect();
      const [x, y, w, h] = rect;
      const left = box.left + box.width * x;
      const top = box.top + box.height * y;
      setRatio(
        splitId,
        dir === "row"
          ? (ev.clientX - left) / (box.width * w)
          : (ev.clientY - top) / (box.height * h),
      );
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      store.paneSizesChanged();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
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
  {#each placed.panes as pane (pane.node.id)}
    <div
      class="slot"
      hidden={placed.zoomed !== null && pane.node.id !== placed.zoomed}
      style:left={pct(pane.rect[0])}
      style:top={pct(pane.rect[1])}
      style:width={pct(pane.rect[2])}
      style:height={pct(pane.rect[3])}
    >
      <PaneView node={pane.node} zoomed={pane.node.id === placed.zoomed} />
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
  }
  .slot[hidden] {
    display: none;
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

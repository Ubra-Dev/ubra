<script module lang="ts">
  import {
    computeLayout,
    presetTab,
    WORKSPACE_LAYOUT_LABELS,
    WORKSPACE_LAYOUT_PANE_COUNTS,
    WORKSPACE_LAYOUT_PRESETS,
    type WorkspaceLayoutPreset,
  } from "./layout";

  export interface LayoutOption {
    id: WorkspaceLayoutPreset;
    label: string;
    detail: string;
    tiles: Array<[number, number, number, number]>;
  }

  function pluralize(count: number): string {
    return `${count} terminal${count === 1 ? "" : "s"}`;
  }

  /**
   * Menu options with deterministic preview geometry. Only tile rects are
   * kept; the sample tab ids are discarded.
   */
  export function layoutOptions(): LayoutOption[] {
    return WORKSPACE_LAYOUT_PRESETS.map((id) => {
      const count = WORKSPACE_LAYOUT_PANE_COUNTS[id];
      return {
        id,
        label: WORKSPACE_LAYOUT_LABELS[id],
        detail: pluralize(count),
        tiles: computeLayout(presetTab(id).root).panes.map((p) => p.rect),
      };
    });
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { overlayFocus } from "./overlayFocus";

  interface Props {
    x: number;
    y: number;
    onPick: (id: WorkspaceLayoutPreset) => void;
    onDismiss: () => void;
    opener?: HTMLElement | null;
    /** Hover tracking for the hover-opened menu. */
    onHoverChange?: (inside: boolean) => void;
  }
  let { x, y, onPick, onDismiss, opener = null, onHoverChange }: Props = $props();
  let menuEl: HTMLDivElement;

  const options = layoutOptions();

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      onDismiss();
      return;
    }
    const buttons = [...menuEl.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];
    if (!buttons.length) return;
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    let next: number;
    if (e.key === "ArrowDown") next = (index + 1) % buttons.length;
    else if (e.key === "ArrowUp") next = index < 0 ? buttons.length - 1 : (index - 1 + buttons.length) % buttons.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = buttons.length - 1;
    else return;
    e.preventDefault();
    e.stopPropagation();
    buttons[next].focus();
  }

  onMount(() => {
    const dismiss = () => onDismiss();
    window.addEventListener("ubra-menu-open", dismiss);
    return () => window.removeEventListener("ubra-menu-open", dismiss);
  });

  // Clamp to the viewport; the menu is created fresh per open.
  const cx = $derived(Math.max(0, Math.min(x, window.innerWidth - 226)));
  const cy = $derived(
    Math.max(0, Math.min(y, window.innerHeight - 24 - options.length * 60)),
  );
</script>

<svelte:window
  onclick={onDismiss}
  oncontextmenu={onDismiss}
/>

<div class="menu" role="menu" tabindex="-1" data-keyboard-overlay
  bind:this={menuEl} use:overlayFocus={{ initial: '[role="menuitem"]', opener }}
  onkeydown={onKeydown} style:left={cx + "px"} style:top={cy + "px"}
  onpointerenter={() => onHoverChange?.(true)}
  onpointerleave={() => onHoverChange?.(false)}>
  {#each options as option (option.id)}
    <button
      role="menuitem"
      tabindex="-1"
      onclick={() => onPick(option.id)}
    >
      <span class="preview" aria-hidden="true">
        {#each option.tiles as tile (tile.join(","))}
          {@const [tx, ty, tw, th] = tile}
          <span
            class="tile"
            style="left: {(tx * 100).toFixed(2)}%; top: {(ty * 100).toFixed(2)}%; width: {(tw * 100).toFixed(2)}%; height: {(th * 100).toFixed(2)}%;"
          ></span>
        {/each}
      </span>
      <span class="labels">
        <span class="label">{option.label}</span>
        <span class="detail">{option.detail}</span>
      </span>
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 1000;
    display: flex;
    flex-direction: column;
    min-width: 208px;
    max-height: calc(100vh - 48px);
    overflow-y: auto;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px;
    box-shadow: 0 4px 16px var(--shadow-color);
    font: 12px var(--font-ui);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    background: transparent;
    border: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    padding: 6px 10px;
    border-radius: 4px;
    cursor: pointer;
  }
  .menu button:hover,
  .menu button:focus-visible {
    color: var(--text-strong);
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .preview {
    position: relative;
    flex: 0 0 auto;
    width: 52px;
    height: 36px;
  }
  .tile {
    position: absolute;
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--pane-bg);
  }
  .menu button:hover .tile,
  .menu button:focus-visible .tile {
    border-color: var(--accent);
  }
  .labels {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .label {
    color: var(--text-strong);
    font-weight: 600;
    white-space: nowrap;
  }
  .detail {
    color: var(--text-muted);
    font-size: 11px;
    white-space: nowrap;
  }
</style>

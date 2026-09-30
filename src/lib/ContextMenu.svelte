<script module lang="ts">
  import type { IconName } from "./Icon.svelte";

  export interface MenuItem {
    id: string;
    label: string;
    danger?: boolean;
    icon?: IconName;
    /** Agent CLI id; renders a brand tile instead of `icon` when set. */
    cli?: string;
  }

  /**
   * Openers call this before showing a menu so any other open menu dismisses
   * itself (the opener's stopPropagation blocks the shared window handler).
   */
  export function announceMenuOpen(): void {
    window.dispatchEvent(new Event("ubra-menu-open"));
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import AgentCliIcon from "./AgentCliIcon.svelte";
  import Icon from "./Icon.svelte";
  import { overlayFocus } from "./overlayFocus";

  interface Props {
    x: number;
    y: number;
    items: MenuItem[];
    onPick: (id: string) => void;
    onDismiss: () => void;
    opener?: HTMLElement | null;
    /** Hover tracking for hover-opened menus; omitted by click-opened menus. */
    onHoverChange?: (inside: boolean) => void;
  }
  let { x, y, items, onPick, onDismiss, opener = null, onHoverChange }: Props = $props();
  let menuEl: HTMLDivElement;
  let viewportWidth = $state(window.innerWidth);
  let viewportHeight = $state(window.innerHeight);
  let menuWidth = $state(158);
  let menuHeight = $state(0);

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
    const measure = () => {
      const bounds = menuEl.getBoundingClientRect();
      menuWidth = bounds.width;
      menuHeight = bounds.height;
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(menuEl);
    const dismiss = () => onDismiss();
    window.addEventListener("ubra-menu-open", dismiss);
    return () => {
      observer.disconnect();
      window.removeEventListener("ubra-menu-open", dismiss);
    };
  });

  // Use rendered dimensions so enlarged text and long labels stay on screen.
  const cx = $derived(Math.max(0, Math.min(x, viewportWidth - menuWidth)));
  const cy = $derived(
    Math.max(0, Math.min(y, viewportHeight - menuHeight)),
  );
</script>

<svelte:window
  bind:innerWidth={viewportWidth}
  bind:innerHeight={viewportHeight}
  onclick={onDismiss}
  oncontextmenu={onDismiss}
/>

<div class="menu" role="menu" tabindex="-1" data-keyboard-overlay
  bind:this={menuEl} use:overlayFocus={{ initial: '[role="menuitem"]', opener }}
  onkeydown={onKeydown} style:left={cx + "px"} style:top={cy + "px"}
  onpointerenter={() => onHoverChange?.(true)}
  onpointerleave={() => onHoverChange?.(false)}>
  {#each items as item (item.id)}
    <button
      role="menuitem"
      tabindex="-1"
      class:danger={item.danger}
      onclick={() => onPick(item.id)}
    >
      {#if item.cli}
        <AgentCliIcon cli={item.cli} size={14} />
      {:else if item.icon}
        <Icon name={item.icon} size={12} />
      {/if}
      {item.label}
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 1000;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    width: max-content;
    min-width: min(140px, 100vw);
    max-width: 100vw;
    max-height: 100vh;
    overflow-y: auto;
    overflow-wrap: anywhere;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px;
    box-shadow: 0 4px 16px var(--shadow-color);
    font: calc(12px * var(--ui-text-scale, 1)) var(--font-ui);
  }
  .menu button {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
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
  .menu button.danger:hover,
  .menu button.danger:focus-visible {
    outline-color: var(--error-text);
  }
</style>

<script module lang="ts">
  import type { IconName } from "./Icon.svelte";

  export interface MenuItem {
    id: string;
    label: string;
    danger?: boolean;
    icon?: IconName;
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
  import Icon from "./Icon.svelte";
  import { overlayFocus } from "./overlayFocus";

  interface Props {
    x: number;
    y: number;
    items: MenuItem[];
    onPick: (id: string) => void;
    onDismiss: () => void;
    opener?: HTMLElement | null;
  }
  let { x, y, items, onPick, onDismiss, opener = null }: Props = $props();
  let menuEl: HTMLDivElement;

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

  // Clamp to the viewport; the menu is created fresh per open, at the cursor.
  const cx = $derived(Math.max(0, Math.min(x, window.innerWidth - 158)));
  const cy = $derived(
    Math.max(0, Math.min(y, window.innerHeight - 24 - items.length * 30)),
  );
</script>

<svelte:window
  onclick={onDismiss}
  oncontextmenu={onDismiss}
/>

<div class="menu" role="menu" tabindex="-1" data-keyboard-overlay
  bind:this={menuEl} use:overlayFocus={{ initial: '[role="menuitem"]', opener }}
  onkeydown={onKeydown} style:left={cx + "px"} style:top={cy + "px"}>
  {#each items as item (item.id)}
    <button
      role="menuitem"
      tabindex="-1"
      class:danger={item.danger}
      onclick={() => onPick(item.id)}
    >
      {#if item.icon}
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
    min-width: 140px;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px;
    box-shadow: 0 4px 16px var(--shadow-color);
    font: 12px system-ui, sans-serif;
  }
  .menu button {
    display: flex;
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
    background: var(--accent);
    color: var(--text-strong);
  }
  .menu button.danger:hover,
  .menu button.danger:focus-visible {
    background: var(--error-hover-bg);
  }
</style>

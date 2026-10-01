<script lang="ts">
  import AgentCliIcon from "./AgentCliIcon.svelte";
  import Icon from "./Icon.svelte";
  import { CUSTOM_COMMAND, type DetectedCli } from "./agentClis";
  import { announceMenuOpen } from "./ContextMenu.svelte";
  import { overlayFocus } from "./overlayFocus";

  interface Row {
    value: string;
    cli: string | null;
    label: string;
    sub: string | null;
    title: string | null;
  }

  interface Props {
    entries: DetectedCli[];
    value?: string;
    /** Shown when value is "" and no none row exists. */
    placeholder?: string;
    /** When set, adds a selectable "" row (e.g. "None (plain shells)"). */
    noneLabel?: string | null;
    customLabel?: string;
    ariaLabel?: string;
    ariaDescribedBy?: string;
    /** "field" is borderless UI font; "input" mimics a bordered select. */
    variant?: "field" | "input";
    onChange?: (value: string) => void;
  }
  let {
    entries,
    value = $bindable(""),
    placeholder = "Choose an agent CLI",
    noneLabel = null,
    customLabel = "Custom command…",
    ariaLabel,
    ariaDescribedBy,
    variant = "input",
    onChange,
  }: Props = $props();

  const rows = $derived<Row[]>([
    ...(noneLabel != null
      ? [{ value: "", cli: null, label: noneLabel, sub: null, title: null }]
      : []),
    ...entries.map((entry) => ({
      value: entry.cli,
      cli: entry.cli,
      label: entry.label,
      sub: entry.cli,
      title: entry.path,
    })),
    { value: CUSTOM_COMMAND, cli: null, label: customLabel, sub: null, title: null },
  ]);
  const selected = $derived(rows.find((row) => row.value === value) ?? null);

  let open = $state(false);
  let active = $state(0);
  let buttonEl = $state<HTMLButtonElement | null>(null);
  let popEl: HTMLDivElement | null = $state(null);
  let viewportWidth = $state(typeof window === "undefined" ? 0 : window.innerWidth);
  let viewportHeight = $state(typeof window === "undefined" ? 0 : window.innerHeight);
  let anchor = $state({ x: 0, y: 0, w: 0, h: 0 });
  let popWidth = $state(0);
  let popHeight = $state(0);

  const GAP = 4;
  const left = $derived(
    Math.max(0, Math.min(anchor.x, viewportWidth - Math.max(popWidth, anchor.w, 1))),
  );
  const top = $derived(
    anchor.y + anchor.h + GAP + popHeight > viewportHeight &&
      anchor.y - GAP - popHeight >= 0
      ? anchor.y - GAP - popHeight
      : anchor.y + anchor.h + GAP,
  );

  /** Exposes button focus so parents can move focus here (replaces selectEl). */
  export function focus(): void {
    buttonEl?.focus();
  }

  function openMenu(): void {
    if (open || !buttonEl) return;
    const bounds = buttonEl.getBoundingClientRect();
    anchor = { x: bounds.left, y: bounds.top, w: bounds.width, h: bounds.height };
    const index = rows.findIndex((row) => row.value === value);
    active = index < 0 ? 0 : index;
    announceMenuOpen();
    open = true;
  }

  function closeMenu(): void {
    open = false;
  }

  function pick(picked: string): void {
    value = picked;
    closeMenu();
    onChange?.(picked);
  }

  function optionButtons(): HTMLButtonElement[] {
    if (!popEl) return [];
    return [...popEl.querySelectorAll<HTMLButtonElement>('[role="option"]')];
  }

  function move(next: number): void {
    const buttons = optionButtons();
    if (!buttons.length) return;
    active = (next + buttons.length) % buttons.length;
    buttons[active].focus();
  }

  function onButtonKeydown(e: KeyboardEvent): void {
    if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openMenu();
    }
  }

  function onPopupKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      closeMenu();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      move(active + (e.key === "ArrowDown" ? 1 : -1));
      return;
    }
    if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      e.stopPropagation();
      move(e.key === "Home" ? 0 : rows.length - 1);
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      const row = rows[active];
      if (row) pick(row.value);
    }
  }

  $effect(() => {
    if (!open || !popEl) return;
    const el = popEl;
    const measure = () => {
      const bounds = el.getBoundingClientRect();
      popWidth = bounds.width;
      popHeight = bounds.height;
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    const dismiss = () => closeMenu();
    window.addEventListener("ubra-menu-open", dismiss);
    return () => {
      observer.disconnect();
      window.removeEventListener("ubra-menu-open", dismiss);
    };
  });
</script>

<svelte:window
  bind:innerWidth={viewportWidth}
  bind:innerHeight={viewportHeight}
  onclick={() => {
    if (open) closeMenu();
  }}
/>

<div class="select" class:field={variant === "field"}>
  <button
    type="button"
    class="button"
    bind:this={buttonEl}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label={ariaLabel}
    aria-describedby={ariaDescribedBy}
    title={selected?.title ?? undefined}
    onclick={(e) => {
      e.stopPropagation();
      if (open) closeMenu();
      else openMenu();
    }}
    onkeydown={onButtonKeydown}
  >
    {#if selected?.cli}
      <AgentCliIcon cli={selected.cli} label={selected.label} size={16} />
      <span class="button-text">{selected.label} · {selected.cli}</span>
    {:else if selected}
      <span class="button-text">{selected.label}</span>
    {:else}
      <span class="button-text dim">{placeholder}</span>
    {/if}
    <Icon name="chevron-down" size={12} />
  </button>
  {#if open}
    <div
      class="popup"
      role="listbox"
      tabindex="-1"
      data-keyboard-overlay
      aria-label={ariaLabel ?? "Agent CLI"}
      bind:this={popEl}
      use:overlayFocus={{ initial: '[role="option"][aria-selected="true"]', opener: buttonEl }}
      onkeydown={onPopupKeydown}
      style:left={left + "px"}
      style:top={top + "px"}
      style:min-width={anchor.w + "px"}
    >
      {#each rows as row, index (row.value)}
        <button
          type="button"
          role="option"
          tabindex="-1"
          aria-selected={row.value === value}
          class:active={index === active}
          title={row.title ?? undefined}
          onclick={(e) => {
            e.stopPropagation();
            pick(row.value);
          }}
          onpointerenter={() => {
            active = index;
          }}
        >
          {#if row.cli}
            <AgentCliIcon cli={row.cli} label={row.label} size={16} />
            <span class="option-text">{row.label} · {row.sub}</span>
          {:else}
            <span class="option-text lone">{row.label}</span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .select {
    position: relative;
    display: flex;
    width: 100%;
  }
  .button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: 1px solid var(--input-border);
    border-radius: 6px;
    padding: 5px 8px 5px 9px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    cursor: pointer;
    text-align: left;
  }
  .field .button {
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--text-strong);
    font: 12px var(--font-ui);
  }
  .button-text {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .button-text.dim {
    color: var(--text-subtle);
  }
  .popup {
    position: fixed;
    z-index: 1000;
    box-sizing: border-box;
    width: max-content;
    max-width: min(320px, 100vw);
    max-height: min(240px, 100vh);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px;
    box-shadow: 0 4px 16px var(--shadow-color);
    font: calc(12px * var(--ui-text-scale, 1)) var(--font-ui);
  }
  .popup button {
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
  .popup button:hover,
  .popup button.active,
  .popup button:focus-visible {
    color: var(--text-strong);
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .option-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .option-text.lone {
    padding-left: 24px;
  }
</style>

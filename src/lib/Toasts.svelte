<script lang="ts">
  import { agent } from "./agent.svelte";
  import Icon from "./Icon.svelte";
  import { store } from "./store.svelte";
  import { toasts } from "./toasts.svelte.ts";

  function onClick(nodeId: string, id: number): void {
    toasts.dismiss(id);
    // Copy toasts (and other node-less notices) dismiss only; agent toasts
    // jump to the pane for review.
    if (nodeId !== "") agent.jumpToPane(nodeId);
  }
</script>

{#if toasts.items.length > 0}
  <div class="stack {store.toastPosition}" role="status" aria-live="polite">
    {#each toasts.items as t (t.id)}
      <button
        class="toast"
        class:copy={t.kind === "copy"}
        onclick={() => onClick(t.nodeId, t.id)}
      >
        <span class="toast-icon">
          <Icon name={t.kind === "copy" ? "check" : "bell"} size={14} />
        </span>
        <span class="texts">
          <span class="title">{t.title}</span>
          <span class="body">{t.body}</span>
        </span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .stack {
    position: fixed;
    z-index: 1500;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 320px;
    pointer-events: none;
  }
  .stack.top-left {
    top: 12px;
    left: 12px;
  }
  .stack.top-right {
    top: 12px;
    right: 12px;
  }
  .stack.bottom-left {
    bottom: 12px;
    left: 12px;
  }
  .stack.bottom-right {
    bottom: 12px;
    right: 12px;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    gap: 8px;
    text-align: left;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-left: 3px solid var(--attention);
    border-radius: 6px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.45);
    padding: 9px 12px;
    cursor: pointer;
    font: 12px system-ui, sans-serif;
    color: var(--text);
  }
  .toast:hover {
    border-color: var(--accent);
    border-left-color: var(--attention);
  }
  .toast-icon {
    flex: 0 0 auto;
    color: var(--attention);
    margin-top: 1px;
  }
  .toast.copy {
    border-left-color: var(--accent);
  }
  .toast.copy:hover {
    border-left-color: var(--accent);
  }
  .toast.copy .toast-icon {
    color: var(--accent);
  }
  .texts {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }
  .title {
    color: var(--text-strong);
    font-weight: 600;
  }
  .body {
    color: var(--text-muted);
  }
</style>

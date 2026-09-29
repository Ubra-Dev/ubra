<script lang="ts">
  import { agent } from "./agent.svelte";
  import { store } from "./store.svelte";
  import { toasts } from "./toasts.svelte.ts";

  function onClick(nodeId: string, id: number): void {
    toasts.dismiss(id);
    agent.jumpToPane(nodeId);
  }
</script>

{#if toasts.items.length > 0}
  <div class="stack {store.toastPosition}" role="status" aria-live="polite">
    {#each toasts.items as t (t.id)}
      <button class="toast" onclick={() => onClick(t.nodeId, t.id)}>
        <span class="title">{t.title}</span>
        <span class="body">{t.body}</span>
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
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
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
  .title {
    color: var(--text-strong);
    font-weight: 600;
  }
  .body {
    color: var(--text-muted);
  }
</style>

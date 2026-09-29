<script lang="ts">
  import { onMount } from "svelte";
  import { store } from "./store.svelte";

  let dialogEl = $state<HTMLDivElement | null>(null);
  let confirmBtn = $state<HTMLButtonElement | null>(null);

  function detail(): string {
    const p = store.pendingClose;
    if (!p) return "";
    if (p.kind === "workspace") {
      const tabs = `${p.tabs} ${p.tabs === 1 ? "tab" : "tabs"}`;
      const panes = `${p.panes} ${p.panes === 1 ? "pane" : "panes"}`;
      return `${tabs}, ${panes}. Running processes in this workspace will be killed.`;
    }
    if (p.kind === "tab") {
      const panes = `${p.panes} ${p.panes === 1 ? "pane" : "panes"}`;
      return `${panes}. Running processes in this tab will be killed.`;
    }
    return "The running process in this pane will be killed.";
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      store.cancelPendingClose();
      return;
    }
    // Single-key shortcuts: no modifiers, dialog has no text inputs.
    if (!e.metaKey && !e.ctrlKey && !e.altKey) {
      if (e.key === "y" || e.key === "Y") {
        store.confirmPendingClose();
        return;
      }
      if (e.key === "n" || e.key === "N") {
        store.cancelPendingClose();
        return;
      }
    }
    if (e.key !== "Tab" || !dialogEl) return;
    // Focus trap: Tab wraps between Cancel and Close.
    const items = [...dialogEl.querySelectorAll<HTMLElement>("button")];
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }

  // Confirm is the default: Enter confirms and dismisses, Esc closes.
  onMount(() => confirmBtn?.focus());
</script>

{#if store.pendingClose}
  {@const p = store.pendingClose}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div
    class="backdrop"
    onclick={(e) => {
      if (e.target === e.currentTarget) store.cancelPendingClose();
    }}
  >
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-close-title"
      aria-describedby="confirm-close-desc"
      tabindex="-1"
      bind:this={dialogEl}
      onkeydown={onKeydown}
    >
      <div class="title" id="confirm-close-title">
        Close {p.kind} "{p.name}"?
      </div>
      <div class="desc" id="confirm-close-desc">{detail()}</div>
      <div class="actions">
        <button
          title="Cancel (N or Esc)"
          onclick={() => store.cancelPendingClose()}
        >
          Cancel
        </button>
        <button
          bind:this={confirmBtn}
          class="danger"
          title="Confirm (Enter or Y)"
          onclick={() => store.confirmPendingClose()}
        >
          Close {p.kind}
        </button>
      </div>
      <div class="keys-hint">
        <kbd>Enter</kbd> confirm · <kbd>N</kbd> cancel
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 3000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
  }
  .dialog {
    width: 360px;
    max-width: calc(100vw - 48px);
    background: var(--app-bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
    font: 12px system-ui, sans-serif;
    color: var(--text);
    padding: 16px;
    outline: none;
  }
  .dialog :focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .title {
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 600;
  }
  .desc {
    margin-top: 8px;
    line-height: 1.5;
    color: var(--text);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
  .actions button {
    border: 1px solid var(--input-border);
    border-radius: 6px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    padding: 5px 14px;
    cursor: pointer;
  }
  .actions button:hover {
    background: var(--surface-bg);
    color: var(--text-strong);
  }
  .actions .danger {
    border-color: var(--error-text);
    color: var(--error-text);
  }
  .actions .danger:hover {
    background: var(--error-bg);
    color: var(--error-text);
  }
  .keys-hint {
    margin-top: 10px;
    text-align: right;
    color: var(--text-subtle);
    font-size: 11px;
  }
  kbd {
    display: inline-block;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 10px;
    color: var(--text-muted);
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 5px;
  }
</style>

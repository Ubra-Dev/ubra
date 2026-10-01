<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { agent } from "./agent.svelte";
  import { overlayFocus } from "./overlayFocus";
  import { quitDialogCopy } from "./quitConfirm";
  import { store } from "./store.svelte";

  let remember = $state(false);

  function activeAgentCount(): number {
    return agent
      .activeAgents()
      .reduce((sum, group) => sum + group.agents.length, 0);
  }

  function confirm(): void {
    store.confirmQuit(remember);
  }

  onMount(() => {
    let unlisten: (() => void) | null = null;
    // The tray menu lives in the backend; it shows the window and routes
    // Quit here so every path shares one confirmation decision.
    void listen("quit-requested", () => store.requestQuit(activeAgentCount()))
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e) => console.error("ubra: quit-request subscription failed", e));
    return () => unlisten?.();
  });

  // Fresh checkbox for every open; closing reuses this component instance.
  $effect(() => {
    if (store.pendingQuit) remember = false;
  });

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      store.cancelQuit();
      return;
    }
    if (e.key === "Enter" && !e.metaKey && !e.ctrlKey && !e.altKey) {
      // A focused button activates natively; confirm only when focus is
      // elsewhere in the dialog.
      if (e.target instanceof HTMLButtonElement) return;
      e.preventDefault();
      confirm();
      return;
    }
    // Single-key shortcuts: no modifiers, dialog has no text inputs.
    if (!e.metaKey && !e.ctrlKey && !e.altKey) {
      if (e.key === "y" || e.key === "Y") {
        confirm();
        return;
      }
      if (e.key === "n" || e.key === "N") {
        store.cancelQuit();
        return;
      }
    }
  }
</script>

{#if store.pendingQuit}
  {@const p = store.pendingQuit}
  {@const copy = quitDialogCopy(p.panes, p.agents)}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div
    class="backdrop"
    onclick={(e) => {
      if (e.target === e.currentTarget) store.cancelQuit();
    }}
  >
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="quit-title"
      aria-describedby="quit-desc"
      tabindex="-1"
      data-keyboard-overlay
      use:overlayFocus={{ initial: ".quit-primary" }}
      onkeydown={onKeydown}
    >
      <div class="title" id="quit-title">{copy.title}</div>
      <div class="desc" id="quit-desc">{copy.detail}</div>
      <label class="remember">
        <input type="checkbox" bind:checked={remember} />
        <span>Remember my choice</span>
      </label>
      <div class="actions">
        <button title="Cancel (N or Esc)" onclick={() => store.cancelQuit()}>
          Cancel
        </button>
        <button
          class="danger quit-primary"
          title="Quit; terminals stop (Enter or Y)"
          onclick={confirm}
        >
          Quit
        </button>
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
    width: 400px;
    max-width: calc(100vw - 48px);
    background: var(--app-bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
    font: 12px var(--font-ui);
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
  .remember {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .remember input {
    accent-color: var(--accent);
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
  .actions .danger:focus-visible {
    outline: none;
  }
  .actions .danger:hover,
  .actions .danger:focus-visible {
    background: var(--error-bg);
    color: var(--error-text);
  }
</style>

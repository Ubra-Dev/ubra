<script lang="ts">
  import { overlayFocus } from "./overlayFocus";
  import { store } from "./store.svelte";
</script>

<div class="empty" role="dialog" aria-modal="true" aria-labelledby="empty-title"
  tabindex="-1" data-keyboard-overlay use:overlayFocus
  inert={store.settingsOpen || !!store.savedSetupsRequest}>
  <div class="card">
    <h1 id="empty-title">No workspaces</h1>
    <p>Open a project folder to start working, or create an empty workspace.</p>
    <div class="actions">
      <button class="primary" onclick={() => store.openOnboarding()}>
        Open a project folder
      </button>
      <button onclick={() => store.addBlankWorkspace()}>New empty workspace</button>
    </div>
  </div>
</div>

<style>
  .empty {
    position: fixed;
    inset: 0;
    /* Above app content and context menus, below settings/onboarding/dialogs. */
    z-index: 1500;
    box-sizing: border-box;
    display: grid;
    place-items: center;
    padding: 32px;
    background: var(--app-bg);
    overflow: auto;
  }
  .card {
    width: min(420px, 100%);
    padding: 30px 30px 26px;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    text-align: center;
  }
  h1 {
    margin: 0;
    color: var(--text-strong);
    font-size: 20px;
    font-weight: 620;
  }
  p {
    margin: 10px 0 0;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.55;
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 22px;
  }
  button {
    min-height: 38px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--sidebar-bg);
    color: var(--text-strong);
    font: inherit;
    cursor: pointer;
  }
  button:hover {
    border-color: var(--accent);
  }
  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 650;
  }
  .primary:hover {
    filter: brightness(1.08);
  }
</style>

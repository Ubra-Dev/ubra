<script lang="ts">
  import { onMount, tick } from "svelte";
  import { overlayFocus } from "./overlayFocus";
  import { open } from "@tauri-apps/plugin-dialog";
  import { CUSTOM_COMMAND, resolveAgentCommand } from "./agentClis";
  import { agentClis } from "./agentClis.svelte";
  import { store } from "./store.svelte";

  let projectDirectory = $state<string | null>(null);
  const detected = $derived(agentClis.clis);
  let selection = $state("");
  let selectionSettled = false;
  let customCommand = $state("");
  let pickingDirectory = $state(false);
  let errorMessage = $state<string | null>(null);
  let selectEl = $state<HTMLSelectElement | null>(null);
  let customEl = $state<HTMLInputElement | null>(null);

  const command = $derived(resolveAgentCommand(selection, customCommand));
  const selectedCli = $derived(detected?.find((entry) => entry.cli === selection) ?? null);

  onMount(() => {
    void agentClis.ensure();
  });

  // Settle the initial selection once when detection resolves; never
  // clobber a choice the user (or a later refresh) already made. Prefill
  // with the current/last used agent CLI when one is known.
  $effect(() => {
    const clis = agentClis.clis;
    if (clis === null || selectionSettled) return;
    selectionSettled = true;
    const prefill = store.preferredAgentCli();
    if (prefill) {
      if (clis.some((entry) => entry.cli === prefill)) {
        selection = prefill;
      } else {
        selection = CUSTOM_COMMAND;
        customCommand = prefill;
      }
      return;
    }
    if (clis.length === 1) selection = clis[0].cli;
    else if (clis.length === 0) selection = CUSTOM_COMMAND;
  });

  function onSelectChange(e: Event): void {
    if ((e.target as HTMLSelectElement).value === CUSTOM_COMMAND) {
      void tick().then(() => customEl?.focus());
    }
  }


  async function chooseProject(): Promise<void> {
    if (pickingDirectory) return;
    pickingDirectory = true;
    errorMessage = null;
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: "Choose a project folder",
      });
      if (typeof path === "string") {
        projectDirectory = path;
        (selectEl ?? customEl)?.focus();
      }
    } catch (error) {
      console.error("ubra: project folder picker failed", error);
      errorMessage = "Couldn't open the folder picker. Try again or skip setup.";
    } finally {
      pickingDirectory = false;
    }
  }

  function startAgent(): void {
    if (!projectDirectory || !command.trim()) return;
    const paneId = store.completeOnboarding(projectDirectory, command);
    if (!paneId) {
      errorMessage = "Couldn't prepare the project terminal. Try skipping setup.";
    }
  }

  function skipSetup(): void {
    store.skipOnboarding();
  }
</script>

<div class="onboarding" role="dialog" aria-modal="true" aria-labelledby="welcome-title"
  tabindex="-1" data-keyboard-overlay use:overlayFocus>
  <div class="card">
    <aside class="intro">
      <div class="brand"><img class="brand-logo" src="/logo.png" alt="Ubra" width="2172" height="724" /></div>
      <div class="intro-copy">
        <h1 id="welcome-title">Put your agent in its project.</h1>
        <p>
          One terminal for your work, with agent activity visible while it runs.
        </p>
      </div>
      <div class="terminal-preview" aria-label="Example agent session">
        <div class="preview-bar">
          <span></span><span></span><span></span>
          <span class="preview-path">your-project</span>
        </div>
        <div class="preview-body">
          <div><span class="prompt">$</span> {command.trim() || "your-agent"}</div>
          <div class="preview-muted">Reading project files…</div>
          <div class="preview-rule"></div>
          <div class="preview-state"><i></i> Agent activity appears here</div>
        </div>
      </div>
      <p class="intro-foot">Your agent runs in a regular terminal.</p>
    </aside>

    <form
      class="setup"
      aria-label="First-run setup"
      onsubmit={(event) => {
        event.preventDefault();
        startAgent();
      }}
    >
      <div class="step-label">Get started</div>
      <h2>Choose where to work</h2>
      <p class="description">
        Pick a project folder, then tell Ubra which agent command to run there.
      </p>

      <div class="field-group">
        <span class="field-label">Project folder</span>
        {#if projectDirectory}
          <div class="selected-folder" title={projectDirectory}>
            <span class="folder-mark" aria-hidden="true">/</span>
            <span class="folder-path">{projectDirectory}</span>
            <button class="change-folder" type="button" onclick={chooseProject}>
              Change
            </button>
          </div>
        {:else}
          <button
            class="folder-button"
            type="button"
            onclick={chooseProject}
            disabled={pickingDirectory}
          >
            <span class="folder-icon" aria-hidden="true">
              <svg viewBox="0 0 20 20" fill="none">
                <path d="M2.5 5.5a1.5 1.5 0 0 1 1.5-1.5h4l1.8 2h6.2a1.5 1.5 0 0 1 1.5 1.5v7a1.5 1.5 0 0 1-1.5 1.5H4a1.5 1.5 0 0 1-1.5-1.5v-9Z" />
                <path d="M2.75 8h14.5" />
              </svg>
            </span>
            <span>{pickingDirectory ? "Opening folders…" : "Choose a project folder"}</span>
            <span class="button-chevron" aria-hidden="true">›</span>
          </button>
        {/if}
      </div>

      <div class="field-group command-group">
        <span class="field-label" id="command-label">Agent command</span>
        {#if detected === null}
          <span class="command-field">
            <span class="command-prompt" aria-hidden="true">$</span>
            <span class="detecting">Detecting installed agents…</span>
          </span>
        {:else}
          {#if detected.length > 0}
            <span class="command-field">
              <span class="command-prompt" aria-hidden="true">$</span>
              <select
                bind:this={selectEl}
                bind:value={selection}
                aria-labelledby="command-label"
                aria-describedby="command-hint"
                onchange={onSelectChange}
              >
                <option value="" disabled>Choose an agent CLI</option>
                {#each detected as entry (entry.cli)}
                  <option value={entry.cli} title={entry.path}>
                    {entry.label} · {entry.cli}
                  </option>
                {/each}
                <option value={CUSTOM_COMMAND}>Custom command…</option>
              </select>
            </span>
          {/if}
          {#if selection === CUSTOM_COMMAND}
            <span class="command-field">
              <span class="command-prompt" aria-hidden="true">$</span>
              <input
                type="text"
                bind:this={customEl}
                bind:value={customCommand}
                placeholder="my-agent --yes"
                autocomplete="off"
                autocapitalize="off"
                spellcheck="false"
                aria-label="Custom agent command"
                aria-describedby="command-hint"
              />
            </span>
          {/if}
        {/if}
        <span class="hint" id="command-hint">
          {#if selectedCli}
            Found at {selectedCli.path}
          {:else if detected === null}
            Looking for installed agent CLIs…
          {:else if detected.length === 0}
            No agent CLIs detected — enter any installed command.
          {:else if selection === CUSTOM_COMMAND}
            Enter any installed command, with arguments if needed.
          {:else}
            {detected.length} installed agent{detected.length === 1 ? "" : "s"} detected.
          {/if}
        </span>
      </div>

      <div class="status-tip">
        <span class="status-dot" aria-hidden="true"></span>
        <span>
          When it runs, its status shows in the pane header and the Agents list.
        </span>
      </div>

      {#if errorMessage}
        <div class="error" role="alert">{errorMessage}</div>
      {/if}

      <div class="actions">
        <button
          class="start-button"
          type="submit"
          onclick={startAgent}
          disabled={!projectDirectory || !command.trim()}
        >
          Open project and start agent
        </button>
        <button class="skip-button" type="button" onclick={skipSetup}>
          {store.firstRun ? "Skip setup" : "Cancel"}
        </button>
      </div>
    </form>
  </div>
</div>

<style>
  .onboarding {
    position: fixed;
    inset: 0;
    z-index: 2100;
    box-sizing: border-box;
    height: 100vh;
    display: grid;
    place-items: center;
    padding: 32px;
    background: var(--app-bg);
    overflow: auto;
  }
  .card {
    width: min(900px, 100%);
    min-height: 530px;
    display: grid;
    grid-template-columns: 0.88fr 1.12fr;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 18px 60px rgba(0, 0, 0, 0.38);
    overflow: hidden;
  }
  .intro {
    display: flex;
    flex-direction: column;
    padding: 32px 30px 24px;
    background: var(--sidebar-bg);
    border-right: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: center;
  }
  .brand-logo {
    display: block;
    height: 36px;
    width: auto;
  }
  .intro-copy {
    margin-top: 70px;
  }
  .intro h1 {
    max-width: 300px;
    margin: 0;
    color: var(--text-strong);
    font-size: 28px;
    line-height: 1.17;
    letter-spacing: -0.6px;
    font-weight: 620;
  }
  .intro-copy p {
    max-width: 290px;
    margin: 13px 0 0;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.55;
  }
  .terminal-preview {
    margin-top: auto;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--input-bg);
    color: var(--text);
    font: 11px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .preview-bar {
    height: 27px;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 9px;
    border-bottom: 1px solid var(--border);
    color: var(--text-subtle);
  }
  .preview-bar > span:not(.preview-path) {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-subtle);
    opacity: 0.55;
  }
  .preview-path {
    margin-left: auto;
    color: var(--text-muted);
  }
  .preview-body {
    display: flex;
    flex-direction: column;
    gap: 11px;
    padding: 14px 12px 13px;
  }
  .prompt {
    color: var(--accent);
  }
  .preview-muted {
    color: var(--text-subtle);
  }
  .preview-rule {
    height: 1px;
    background: var(--separator);
  }
  .preview-state {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--text-muted);
    font: 11px var(--font-ui);
  }
  .preview-state i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--success);
  }
  .intro-foot {
    margin: 13px 0 0;
    color: var(--text-subtle);
    font-size: 11px;
  }
  .setup {
    align-self: center;
    padding: 48px 46px 40px;
  }
  .step-label {
    margin-bottom: 11px;
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
  }
  .setup h2 {
    margin: 0;
    color: var(--text-strong);
    font-size: 21px;
    letter-spacing: -0.25px;
    font-weight: 620;
  }
  .description {
    margin: 8px 0 26px;
    color: var(--text-muted);
    line-height: 1.5;
  }
  .field-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .field-label {
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
  }
  .folder-button,
  .selected-folder,
  .command-field {
    min-height: 42px;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 10px;
    border: 1px solid var(--input-border);
    border-radius: 7px;
    background: var(--input-bg);
    color: var(--text);
  }
  .folder-button {
    width: 100%;
    padding: 0 12px;
    text-align: left;
    font: inherit;
    cursor: pointer;
  }
  .folder-button:hover:not(:disabled),
  .change-folder:hover {
    border-color: var(--accent);
    color: var(--text-strong);
  }
  .folder-button:disabled {
    opacity: 0.65;
    cursor: wait;
  }
  .folder-icon {
    display: inline-flex;
    color: var(--accent);
  }
  .folder-icon svg {
    width: 17px;
    height: 17px;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .button-chevron {
    margin-left: auto;
    color: var(--text-subtle);
    font-size: 19px;
  }
  .selected-folder {
    min-width: 0;
    padding: 0 8px 0 12px;
  }
  .folder-mark {
    flex: 0 0 auto;
    color: var(--accent);
    font-family: ui-monospace, Menlo, Consolas, monospace;
  }
  .folder-path {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font: 11px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .change-folder {
    flex: 0 0 auto;
    border: 1px solid transparent;
    border-radius: 5px;
    padding: 4px 7px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    cursor: pointer;
  }
  .command-group {
    margin-top: 21px;
  }
  .command-field {
    padding: 0 11px;
  }
  .command-prompt {
    color: var(--accent);
    font-family: ui-monospace, Menlo, Consolas, monospace;
  }
  .command-field input {
    width: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text-strong);
    font: 12px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .command-field select {
    width: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text-strong);
    font: 12px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    cursor: pointer;
  }
  .detecting {
    color: var(--text-subtle);
    font-size: 12px;
  }
  .command-field:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .hint {
    color: var(--text-subtle);
    font-size: 11px;
    line-height: 1.45;
  }
  .status-tip {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    margin-top: 20px;
    padding: 11px 12px;
    border-left: 2px solid var(--accent);
    background: var(--sidebar-bg);
    color: var(--text-muted);
    font-size: 11px;
    line-height: 1.45;
  }
  .status-dot {
    flex: 0 0 auto;
    width: 7px;
    height: 7px;
    margin-top: 3px;
    border-radius: 50%;
    background: var(--success);
  }
  .error {
    margin-top: 12px;
    color: var(--danger, #f87171);
    font-size: 11px;
    line-height: 1.4;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 25px;
  }
  .start-button {
    min-height: 38px;
    padding: 0 14px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent);
    color: var(--app-bg);
    font: inherit;
    font-weight: 650;
    cursor: pointer;
  }
  .start-button:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .start-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .skip-button {
    min-height: 34px;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    cursor: pointer;
  }
  .skip-button:hover {
    color: var(--text-strong);
  }
  :global(.onboarding :focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  @media (max-width: 760px) {
    .onboarding {
      place-items: start center;
      padding: 18px;
    }
    .card {
      max-width: 520px;
      grid-template-columns: 1fr;
    }
    .intro {
      padding: 22px 24px;
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
    .intro-copy {
      margin-top: 27px;
    }
    .intro h1 {
      max-width: 440px;
      font-size: 23px;
    }
    .intro-copy p {
      max-width: 440px;
    }
    .terminal-preview,
    .intro-foot {
      display: none;
    }
    .setup {
      padding: 28px 24px 25px;
    }
  }
  @media (max-height: 660px) {
    .onboarding {
      place-items: start center;
      padding-top: 24px;
      padding-bottom: 24px;
    }
  }
</style>

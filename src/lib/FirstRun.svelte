<script lang="ts">
  import { PUBLIC_POSTHOG_HOST, PUBLIC_POSTHOG_PROJECT_TOKEN } from "$env/static/public";
  import posthog from "posthog-js";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import { overlayFocus } from "./overlayFocus";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { CUSTOM_COMMAND, resolveAgentCommand } from "./agentClis";
  import AgentCliIcon from "./AgentCliIcon.svelte";
  import AgentCliSelect from "./AgentCliSelect.svelte";
  import {
    DEFAULT_FLEET_STARTER_ID,
    FLEET_MAX_PANES,
    FLEET_STARTERS,
    fleetStarterById,
    planFleet,
  } from "./fleet";
  import { agentClis } from "./agentClis.svelte";
  import { posthogLogs } from "./posthogLogs";
  import { PRIVACY_URL } from "./site.ts";
  import { store } from "./store.svelte";
  import { telemetryStatus } from "./telemetry";
  import { applyTelemetryConsent } from "./telemetrySync";

  interface Props {
    /** Revisit from Settings: creates a new workspace, never rewrites consent. */
    revisit?: boolean;
  }
  let { revisit = false }: Props = $props();

  let projectDirectory = $state<string | null>(null);
  const detected = $derived(agentClis.clis);
  let selection = $state("");
  let selectionSettled = false;
  let customCommand = $state("");
  let pickingDirectory = $state(false);
  let errorMessage = $state<string | null>(null);
  let selectEl = $state<AgentCliSelect | null>(null);
  let customEl = $state<HTMLInputElement | null>(null);
  let telemetryOptIn = $state(false);
  let telemetrySupported = $state(false);
  let onboardingActionBusy = $state(false);
  /** First-run wizard step; revisit mode always shows the single screen. */
  let step = $state<"folder" | "fleet">("folder");
  let fleetPicks = $state<string[]>([]);
  let fleetSettled = false;
  let starterId = $state(DEFAULT_FLEET_STARTER_ID);
  let supportedClis = $state<{ cli: string; label: string }[] | null>(null);
  let fleetShownCaptured = false;

  const command = $derived(resolveAgentCommand(selection, customCommand));
  const selectedCli = $derived(detected?.find((entry) => entry.cli === selection) ?? null);
  const fleetCommands = $derived(
    planFleet(fleetPicks, detected?.length === 0 ? customCommand : null),
  );
  const fleetStarter = $derived(fleetStarterById(starterId));
  const supportedNames = $derived.by(() => {
    const list = supportedClis ?? [];
    if (list.length === 0) return "";
    const head = list.slice(0, 6).map((entry) => entry.label);
    const rest = list.length - head.length;
    return rest > 0 ? `${head.join(", ")} and ${rest} more` : head.join(", ");
  });

  onMount(() => {
    void agentClis.ensure();
    telemetryStatus()
      .then((status) => {
        telemetrySupported = status.supported;
        telemetryOptIn = status.consented;
      })
      .catch((e) => console.error("ubra: telemetry status failed", e));
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

  // Settle fleet picks once: the preferred CLI first (it takes the hero
  // pane), then detection order, capped at the fleet size.
  $effect(() => {
    const clis = agentClis.clis;
    if (clis === null || fleetSettled) return;
    fleetSettled = true;
    const prefill = store.preferredAgentCli();
    const ordered = [...clis].sort((a, b) =>
      a.cli === prefill ? -1 : b.cli === prefill ? 1 : 0,
    );
    fleetPicks = ordered.slice(0, FLEET_MAX_PANES).map((entry) => entry.cli);
  });

  $effect(() => {
    if (revisit || step !== "fleet") return;
    const clis = detected;
    if (clis === null) return;
    if (!fleetShownCaptured) {
      fleetShownCaptured = true;
      if (PUBLIC_POSTHOG_PROJECT_TOKEN && PUBLIC_POSTHOG_HOST) {
        posthog.capture("fleet_shown", { detectedClis: clis.length });
        posthogLogs.fleetShown(clis.length);
      }
    }
    if (clis.length === 0 && supportedClis === null) {
      invoke<{ cli: string; label: string }[]>("supported_agent_clis")
        .then((list) => {
          supportedClis = list;
        })
        .catch((error: unknown) =>
          console.error("ubra: supported CLIs lookup failed", error),
        );
    }
  });

  function togglePick(cli: string): void {
    if (fleetPicks.includes(cli)) {
      fleetPicks = fleetPicks.filter((pick) => pick !== cli);
    } else if (fleetPicks.length < FLEET_MAX_PANES) {
      fleetPicks = [...fleetPicks, cli];
    }
  }

  function goFleet(): void {
    errorMessage = null;
    step = "fleet";
  }

  function goFolder(): void {
    errorMessage = null;
    step = "folder";
  }

  function onSelectChange(picked: string): void {
    if (picked === CUSTOM_COMMAND) {
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

  async function startAgent(): Promise<void> {
    if (onboardingActionBusy || !projectDirectory || !command.trim()) return;
    onboardingActionBusy = true;
    if (revisit) {
      const paneId = store.completeOnboardingRevisit(projectDirectory, command);
      if (!paneId) {
        errorMessage = "Couldn't prepare the project terminal. Try closing and retrying.";
        onboardingActionBusy = false;
      }
      return;
    }
    await applyTelemetryConsent(telemetryOptIn).catch((error: unknown) => {
      console.error("ubra: onboarding consent failed", error);
    });
    const paneId = store.completeOnboarding(projectDirectory, command);
    if (!paneId) {
      errorMessage = "Couldn't prepare the project terminal. Try skipping setup.";
      onboardingActionBusy = false;
      return;
    }
    if (PUBLIC_POSTHOG_PROJECT_TOKEN && PUBLIC_POSTHOG_HOST) {
      posthog.capture("onboarding_completed");
      posthogLogs.onboardingCompleted();
    }
  }

  async function launchFleet(): Promise<void> {
    if (onboardingActionBusy || !projectDirectory || fleetCommands.length === 0) {
      return;
    }
    onboardingActionBusy = true;
    await applyTelemetryConsent(telemetryOptIn).catch((error: unknown) => {
      console.error("ubra: onboarding consent failed", error);
    });
    const ids = store.completeOnboardingFleet(
      projectDirectory,
      fleetCommands,
      fleetStarter.prompt,
    );
    if (!ids) {
      errorMessage = "Couldn't prepare the project terminals. Try skipping setup.";
      onboardingActionBusy = false;
      return;
    }
    if (PUBLIC_POSTHOG_PROJECT_TOKEN && PUBLIC_POSTHOG_HOST) {
      posthog.capture("onboarding_completed");
      posthog.capture("fleet_launched", { cliCount: fleetCommands.length });
      posthogLogs.onboardingCompleted();
      posthogLogs.fleetLaunched(fleetCommands.length);
    }
  }

  async function startSingle(): Promise<void> {
    const primary = fleetCommands[0];
    if (onboardingActionBusy || !projectDirectory || !primary) return;
    onboardingActionBusy = true;
    await applyTelemetryConsent(telemetryOptIn).catch((error: unknown) => {
      console.error("ubra: onboarding consent failed", error);
    });
    const paneId = store.completeOnboarding(projectDirectory, primary);
    if (!paneId) {
      errorMessage = "Couldn't prepare the project terminal. Try skipping setup.";
      onboardingActionBusy = false;
      return;
    }
    if (PUBLIC_POSTHOG_PROJECT_TOKEN && PUBLIC_POSTHOG_HOST) {
      posthog.capture("onboarding_completed");
      posthogLogs.onboardingCompleted();
    }
  }

  async function skipSetup(): Promise<void> {
    if (onboardingActionBusy) return;
    if (revisit) {
      store.onboardingOpen = false;
      return;
    }
    onboardingActionBusy = true;
    await applyTelemetryConsent(telemetryOptIn).catch((error: unknown) => {
      console.error("ubra: onboarding consent failed", error);
    });
    if (PUBLIC_POSTHOG_PROJECT_TOKEN && PUBLIC_POSTHOG_HOST) {
      posthog.capture("onboarding_skipped");
      posthogLogs.onboardingSkipped();
    }
    await store.skipOnboarding();
  }

  function onRevisitKeydown(e: KeyboardEvent): void {
    if (revisit && e.key === "Escape") {
      e.preventDefault();
      store.onboardingOpen = false;
    }
  }
</script>

<div class="onboarding" role="dialog" aria-modal="true" aria-labelledby="welcome-title"
  tabindex="-1" data-keyboard-overlay use:overlayFocus onkeydown={onRevisitKeydown}>
  <div class="card">
    <aside class="intro">
      <div class="brand"><img class="brand-logo" src="/logo.png" alt="Ubra" width="2172" height="724" /></div>
      {#if !revisit && step === "fleet"}
        <div class="intro-copy">
          <h1 id="welcome-title">Launch your fleet.</h1>
          <p>
            Up to three agents, side by side, starting on your first task together.
          </p>
        </div>
        <div class="fleet-preview" aria-label="Fleet layout preview">
          {#if fleetCommands.length === 0}
            <div class="fleet-empty">Pick at least one CLI to preview your fleet.</div>
          {:else}
            <div class="fleet-grid" data-panes={fleetCommands.length}>
              {#each fleetCommands as cmd, i (cmd)}
                <div class="fleet-cell" class:hero={i === 0}>
                  <span class="fleet-cell-cli">{cmd}</span>
                  <span class="fleet-cell-task">{fleetStarter.title}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
        <p class="intro-foot">Every agent runs in a regular terminal.</p>
      {:else}
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
      {/if}
    </aside>

    <form
      class="setup"
      aria-label="Project setup"
      onsubmit={(event) => {
        event.preventDefault();
        if (revisit) startAgent();
        else if (step === "folder") goFleet();
        else launchFleet();
      }}
    >
      <div class="step-label">
        {revisit ? "Get started" : step === "folder" ? "Step 1 of 2 — Project" : "Step 2 of 2 — Fleet"}
      </div>
      {#if !revisit && step === "fleet"}
        <h2>Choose your fleet</h2>
        <p class="description">
          Pick the agents to launch side by side, and their first task.
        </p>
      {:else}
        <h2>Choose where to work</h2>
        <p class="description">
          {#if revisit}
            Pick a project folder, then tell Ubra which agent command to run there.
          {:else}
            Pick a project folder — your fleet will start working there.
          {/if}
        </p>
      {/if}

      {#if revisit || step === "folder"}
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

      {/if}
      {#if revisit}
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
              <AgentCliSelect
                bind:this={selectEl}
                entries={detected}
                bind:value={selection}
                placeholder="Choose an agent CLI"
                variant="field"
                ariaLabel="Agent command"
                ariaDescribedBy="command-hint"
                onChange={onSelectChange}
              />
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

      {/if}
      {#if !revisit && step === "fleet"}
      <div class="field-group">
        <span class="field-label" id="fleet-label">Your fleet</span>
        {#if detected === null}
          <span class="command-field">
            <span class="command-prompt" aria-hidden="true">$</span>
            <span class="detecting">Detecting installed agents…</span>
          </span>
        {:else if detected.length === 0}
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
            />
          </span>
          <span class="hint">
            No agent CLIs detected — enter any installed command.
            {#if supportedNames}
              Ubra works with {supportedNames}.
            {/if}
          </span>
        {:else}
          <div class="cli-cards" role="group" aria-labelledby="fleet-label">
            {#each detected as entry (entry.cli)}
              {@const picked = fleetPicks.includes(entry.cli)}
              {@const capped = !picked && fleetPicks.length >= FLEET_MAX_PANES}
              <label class="cli-card" class:picked class:capped>
                <input
                  type="checkbox"
                  checked={picked}
                  disabled={capped}
                  onchange={() => togglePick(entry.cli)}
                  aria-label={entry.label}
                />
                <AgentCliIcon cli={entry.cli} label={entry.label} size={20} />
                <span class="cli-card-text">
                  <span class="cli-card-label">{entry.label}</span>
                  <span class="cli-card-path" title={entry.path}>{entry.path}</span>
                </span>
                {#if fleetPicks[0] === entry.cli}
                  <span class="cli-card-hero">Main</span>
                {/if}
              </label>
            {/each}
          </div>
          {#if detected.length > FLEET_MAX_PANES}
            <span class="hint">
              +{detected.length - FLEET_MAX_PANES} more installed — add them after setup.
            </span>
          {/if}
        {/if}
      </div>

      <div class="field-group starter-group">
        <span class="field-label" id="starter-label">First task</span>
        <div class="starter-list" role="radiogroup" aria-labelledby="starter-label">
          {#each FLEET_STARTERS as starter (starter.id)}
            <label class="starter-card" class:picked={starterId === starter.id}>
              <input
                type="radio"
                name="fleet-starter"
                checked={starterId === starter.id}
                onchange={() => (starterId = starter.id)}
              />
              <span class="starter-text">
                <span class="starter-title">{starter.title}</span>
                <span class="starter-blurb">{starter.blurb}</span>
              </span>
            </label>
          {/each}
        </div>
      </div>

      <div class="status-tip">
        <span class="status-dot" aria-hidden="true"></span>
        <span>
          The fleet runs on your own CLI accounts. Starter tasks only read your
          code — nothing is changed.
        </span>
      </div>
      {:else}
      <div class="status-tip">
        <span class="status-dot" aria-hidden="true"></span>
        <span>
          When it runs, its status shows in the pane header and the Agents list.
        </span>
      </div>
      {/if}

      {#if errorMessage}
        <div class="error" role="alert">{errorMessage}</div>
      {/if}

      {#if telemetrySupported && !revisit && step === "folder"}
        <label class="consent-row">
          <input type="checkbox" bind:checked={telemetryOptIn} />
          <span>
            Help improve Ubra by sharing anonymous usage and crash reports.
            <span class="hint-inline">
              Never code, file paths, or commands. Change anytime in Settings.
              <button
                type="button"
                class="inline-link"
                onclick={(e) => {
                  e.stopPropagation();
                  e.preventDefault();
                  openUrl(PRIVACY_URL).catch(console.error);
                }}
              >
                Privacy Policy
              </button>
            </span>
          </span>
        </label>
      {/if}

      <div class="actions">
        {#if revisit}
          <button
            class="start-button"
            type="submit"
            disabled={!projectDirectory || !command.trim() || onboardingActionBusy}
          >
            Get Started
          </button>
          <button
            class="skip-button"
            type="button"
            onclick={skipSetup}
            disabled={onboardingActionBusy}
          >
            Close
          </button>
        {:else if step === "folder"}
          <button
            class="start-button"
            type="submit"
            disabled={!projectDirectory || onboardingActionBusy}
          >
            Continue →
          </button>
          <button
            class="skip-button"
            type="button"
            onclick={skipSetup}
            disabled={onboardingActionBusy}
          >
            {store.firstRun ? "Skip setup" : "Empty Workspace"}
          </button>
        {:else}
          <button
            class="start-button"
            type="submit"
            disabled={!projectDirectory ||
              fleetCommands.length === 0 ||
              onboardingActionBusy}
          >
            Launch my fleet →
          </button>
          <button
            class="skip-button"
            type="button"
            onclick={startSingle}
            disabled={!projectDirectory ||
              fleetCommands.length === 0 ||
              onboardingActionBusy}
          >
            Just one terminal
          </button>
          <button
            class="skip-button"
            type="button"
            onclick={goFolder}
            disabled={onboardingActionBusy}
          >
            ← Back
          </button>
          <button
            class="skip-button"
            type="button"
            onclick={skipSetup}
            disabled={onboardingActionBusy}
          >
            {store.firstRun ? "Skip setup" : "Empty Workspace"}
          </button>
        {/if}
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
    font-family: var(--font-ui);
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
    font: 11px var(--font-ui);
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
    font-family: var(--font-ui);
  }
  .folder-path {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font: 11px var(--font-ui);
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
    font-family: var(--font-ui);
  }
  .command-field input {
    width: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text-strong);
    font: 12px var(--font-ui);
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
  .consent-row {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    margin-top: 18px;
    color: var(--text-muted);
    font-size: 12px;
    line-height: 1.5;
    cursor: pointer;
  }
  .consent-row input {
    margin-top: 3px;
    flex: 0 0 auto;
    accent-color: var(--accent);
  }
  .consent-row .hint-inline {
    color: var(--text-subtle);
  }
  .inline-link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    cursor: pointer;
  }
  .inline-link:hover {
    color: var(--accent-hover);
    text-decoration: underline;
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
    flex-wrap: wrap;
    gap: 8px 14px;
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
  .cli-cards {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .cli-card {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 46px;
    padding: 8px 12px;
    border: 1px solid var(--input-border);
    border-radius: 7px;
    background: var(--input-bg);
    cursor: pointer;
  }
  .cli-card input {
    flex: 0 0 auto;
    accent-color: var(--accent);
  }
  .cli-card.picked {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .cli-card.capped {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .cli-card-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    line-height: 1.35;
  }
  .cli-card-label {
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 600;
  }
  .cli-card-path {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--text-subtle);
    font: 11px var(--font-ui);
  }
  .cli-card-hero {
    flex: 0 0 auto;
    padding: 2px 7px;
    border-radius: 20px;
    background: var(--accent);
    color: var(--app-bg);
    font-size: 10px;
    font-weight: 700;
  }
  .starter-group {
    margin-top: 21px;
  }
  .starter-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .starter-card {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--input-border);
    border-radius: 7px;
    background: var(--input-bg);
    cursor: pointer;
  }
  .starter-card input {
    margin-top: 2px;
    flex: 0 0 auto;
    accent-color: var(--accent);
  }
  .starter-card.picked {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .starter-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    line-height: 1.4;
  }
  .starter-title {
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 600;
  }
  .starter-blurb {
    color: var(--text-subtle);
    font-size: 11px;
  }
  .fleet-preview {
    margin-top: auto;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--input-bg);
    padding: 10px;
  }
  .fleet-empty {
    padding: 12px 6px;
    color: var(--text-subtle);
    font-size: 11px;
    line-height: 1.5;
    text-align: center;
  }
  .fleet-grid {
    display: grid;
    gap: 6px;
    min-height: 120px;
  }
  .fleet-grid[data-panes="1"] {
    grid-template-columns: 1fr;
  }
  .fleet-grid[data-panes="2"] {
    grid-template-columns: 1fr 1fr;
  }
  .fleet-grid[data-panes="3"] {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
  }
  .fleet-grid[data-panes="3"] .fleet-cell.hero {
    grid-row: span 2;
  }
  .fleet-cell {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 4px;
    min-width: 0;
    padding: 8px 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-bg);
  }
  .fleet-cell.hero {
    border-color: var(--accent);
  }
  .fleet-cell-cli {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--accent);
    font: 11px var(--font-ui);
    font-weight: 700;
  }
  .fleet-cell-task {
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    color: var(--text-subtle);
    font-size: 10px;
    line-height: 1.35;
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
    .fleet-preview,
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

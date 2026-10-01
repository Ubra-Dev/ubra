<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { agent } from "./agent.svelte";
  import { CUSTOM_COMMAND } from "./agentClis";
  import { telemetryStatus } from "./telemetry";
  import { applyTelemetryConsent } from "./telemetrySync";
  import AgentCliSelect from "./AgentCliSelect.svelte";
  import { agentClis } from "./agentClis.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import Spinner from "./Spinner.svelte";
  import { overlayFocus } from "./overlayFocus";
  import {
    describeNotifyOutcome,
    parseNotifyOutcome,
    parseNotifyPermission,
    playbackPayload,
    routeNotification,
    testNotificationPayload,
    type NotifyDelivery,
    type NotifyPermission,
    type ToastPosition,
  } from "./notify";
  import { cheatSheet, isMacPlatform } from "./shortcuts";
  import {
    DEFAULT_TERM_FONT_SIZE,
    MAX_TERM_FONT_SIZE,
    MAX_TERM_OPACITY,
    MIN_TERM_FONT_SIZE,
    MIN_TERM_OPACITY,
    SCROLLBACK_OPTIONS,
    store,
  } from "./store.svelte";
  import {
    DEFAULT_UI_SCALE,
    MAX_UI_SCALE,
    MIN_UI_SCALE,
  } from "./uiScale";
  import {
    DEFAULT_UI_FONT_ID,
    UI_FONTS,
    matchUiFonts,
  } from "./uiFonts";
  import { toasts } from "./toasts.svelte.ts";
  import { THEMES, THEME_IDS, isThemeId } from "./themes";
  import {
    formatResetCountdown,
    formatUpdatedAgo,
    joinLabels,
    selectUsageClis,
  } from "./usage";
  import { usage } from "./usage.svelte";
  import { updater } from "./updater.svelte";

  type SectionId =
    | "appearance"
    | "alerts"
    | "shortcuts"
    | "app"
    | "workspace"
    | "usage";

  const SECTIONS: { id: SectionId; label: string; icon: IconName }[] = [
    { id: "appearance", label: "Appearance", icon: "palette" },
    { id: "alerts", label: "Alerts", icon: "bell" },
    { id: "shortcuts", label: "Shortcuts", icon: "command" },
    { id: "app", label: "App", icon: "info" },
    { id: "workspace", label: "Workspace", icon: "layers" },
    { id: "usage", label: "Usage", icon: "activity" },
  ];

  let section = $state<SectionId>("app");

  let autostart = $state(false);
  let autostartLoaded = $state(false);
  let autostartError = $state<string | null>(null);
  let telemetrySupported = $state(false);
  let telemetryConsented = $state(false);
  let telemetryLoaded = $state(false);
  let telemetryError = $state<string | null>(null);
  let appName = $state("Ubra");
  let appVersion = $state("");
  let fontQuery = $state("");
  /** Workspace-section draft: "" = off, a CLI id, or CUSTOM_COMMAND. */
  let cliSelection = $state("");
  let cliCustom = $state("");
  let cliDraftReady = $state(false);
  let folderPicking = $state(false);
  let folderError = $state<string | null>(null);
  let appearanceTab = $state<"theme" | "text">("theme");
  /** System-notification permission: silent check on open, request on Test. */
  let notifyPermission = $state<NotifyPermission>("unknown");
  let testNotifyError = $state<string | null>(null);
  let testNotifyNote = $state<string | null>(null);
  /** True while a Test is in flight; the OS prompt can keep it waiting. */
  let testNotifyBusy = $state(false);
  const notifyPermissionLabel = $derived(
    notifyPermission === "granted"
      ? "granted"
      : notifyPermission === "denied"
        ? "denied — enable it in your OS settings"
        : notifyPermission === "prompt"
          ? "not decided yet — pressing Test will ask"
          : "unknown — this build can't query the system state",
  );

  /** Usage providers to show; null while detection or registry is loading. */
  const usageShowable = $derived(
    agentClis.clis === null || usage.supported === null
      ? null
      : selectUsageClis(agentClis.clis, usage.supported),
  );
  const usageAnyLoading = $derived(
    usageShowable?.some((entry) => usage.loading[entry.cli] === true) ?? false,
  );

  function refreshAllUsage(): void {
    if (!usageShowable) return;
    usage.refreshAllForced(usageShowable.map((entry) => entry.cli));
  }

  const shortcuts = cheatSheet(isMacPlatform(navigator.platform));

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    const mb = bytes / (1024 * 1024);
    return mb < 10 ? `${mb.toFixed(1)} MB` : `${Math.round(mb)} MB`;
  }

  function close(): void {
    store.settingsOpen = false;
  }

  function onDialogKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      return;
    }
  }

  onMount(() => {
    // Honor a section requested by menus/commands; fall back to App.
    const requested = store.settingsOpenSection;
    if (requested && SECTIONS.some((s) => s.id === requested)) {
      section = requested as SectionId;
    }
    store.settingsOpenSection = null;
    invoke<boolean>("autostart_enabled")
      .then((v) => {
        autostart = v;
        autostartLoaded = true;
      })
      .catch((e) => console.error("ubra: autostart check failed", e));
    telemetryStatus()
      .then((status) => {
        telemetrySupported = status.supported;
        telemetryConsented = status.consented;
        telemetryLoaded = true;
      })
      .catch((e) => console.error("ubra: telemetry status failed", e));
    invoke<{ name: string; version: string }>("app_info")
      .then((info) => {
        appName = info.name;
        appVersion = info.version;
      })
      .catch((e) => console.error("ubra: app info failed", e));
    void agentClis.ensure();
  });

  // Seed the CLI draft from the active workspace once detection resolves.
  $effect(() => {
    if (cliDraftReady) return;
    const detected = agentClis.clis;
    if (detected === null) return;
    const current = store.workspace()?.defaultCli?.trim() ?? "";
    if (!current) {
      cliSelection = "";
      cliCustom = "";
    } else if (detected.some((entry) => entry.cli === current)) {
      cliSelection = current;
      cliCustom = "";
    } else {
      cliSelection = CUSTOM_COMMAND;
      cliCustom = current;
    }
    cliDraftReady = true;
  });

  // Load usage providers when the section opens; refresh only supported CLIs.
  $effect(() => {
    if (section !== "usage") return;
    const detected = agentClis.clis;
    if (detected === null) return;
    void (async () => {
      const supported = await usage.ensureSupported();
      usage.refreshAll(selectUsageClis(detected, supported).map((entry) => entry.cli));
    })();
  });

  // Silent permission check when Alerts opens; the OS prompt only fires from
  // the Test button below, never from merely viewing this section.
  $effect(() => {
    if (section !== "alerts") return;
    invoke<unknown>("notification_permission")
      .then((state) => {
        notifyPermission = parseNotifyPermission(state);
      })
      .catch(() => {
        notifyPermission = "unknown";
      });
  });


  function onAutostartChange(e: Event): void {
    const checked = (e.target as HTMLInputElement).checked;
    autostart = checked;
    autostartError = null;
    invoke("autostart_set", { enabled: checked }).catch((err) => {
      console.error("ubra: autostart update failed", err);
      autostart = !checked;
      autostartError = "Couldn't update launch at login; reverted.";
    });
  }

  async function onTelemetryChange(e: Event): Promise<void> {
    const checked = (e.target as HTMLInputElement).checked;
    telemetryError = null;
    try {
      await applyTelemetryConsent(checked);
      telemetryConsented = checked;
    } catch (err) {
      console.error("ubra: telemetry consent failed", err);
      telemetryConsented = !checked;
      telemetryError = "Couldn't update the telemetry preference; reverted.";
    }
  }

  function onDefaultCliSelect(picked: string): void {
    const ws = store.workspace();
    if (!ws) return;
    const value = picked;
    cliSelection = value;
    if (value === CUSTOM_COMMAND) {
      store.setWorkspaceDefaultCli(ws.id, cliCustom || null);
      return;
    }
    cliCustom = "";
    store.setWorkspaceDefaultCli(ws.id, value || null);
  }

  function onDefaultCliCustom(e: Event): void {
    const ws = store.workspace();
    if (!ws) return;
    cliCustom = (e.target as HTMLInputElement).value;
    store.setWorkspaceDefaultCli(ws.id, cliCustom || null);
  }

  async function pickDefaultFolder(): Promise<void> {
    const ws = store.workspace();
    if (!ws || folderPicking) return;
    folderPicking = true;
    folderError = null;
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: "Choose the default folder",
      });
      if (typeof path === "string") store.setWorkspaceDefaultCwd(ws.id, path);
    } catch (error) {
      console.error("ubra: default folder picker failed", error);
      folderError = "Couldn't open the folder picker. Try again.";
    } finally {
      folderPicking = false;
    }
  }

  function onThemeSelect(id: string): void {
    if (isThemeId(id)) store.setTheme(id);
  }

  /**
   * Ensure system-notification permission, prompting the OS dialog when the
   * verdict is still undecided. Returns true when showing is allowed. The
   * backend answers from the real OS state; only this gesture may prompt.
   */
  async function ensureNotifyPermission(): Promise<boolean> {
    try {
      notifyPermission = parseNotifyPermission(
        await invoke<unknown>("notification_permission"),
      );
    } catch (e) {
      console.error("ubra: notification permission check failed", e);
      notifyPermission = "unknown";
      return false;
    }
    if (notifyPermission === "granted") return true;
    if (notifyPermission !== "prompt") return false;
    try {
      notifyPermission = parseNotifyPermission(
        await invoke<unknown>("request_notification_permission"),
      );
      return notifyPermission === "granted";
    } catch (e) {
      console.error("ubra: notification permission request failed", e);
      notifyPermission = "unknown";
      return false;
    }
  }

  /**
   * Fire one sample agent-finish through the current delivery/sound/mute
   * settings, exactly like a real finish (muted Codex stays silent).
   * Failures surface inline instead of only in the console.
   */
  async function sendTestNotification(): Promise<void> {
    if (testNotifyBusy) return;
    testNotifyBusy = true;
    testNotifyError = null;
    testNotifyNote = null;
    try {
      const test = testNotificationPayload();
      const route = routeNotification({
        delivery: store.notifyDelivery,
        soundEnabled: store.soundEnabled,
        mutedClis: store.mutedAgents,
        cli: test.cli,
      });
      if (route.toast) {
        toasts.push(test.title, test.body, test.nodeId);
      }
      if (route.system) {
        const allowed = await ensureNotifyPermission();
        if (!allowed) {
          testNotifyError =
            notifyPermission === "denied"
              ? "System notifications are blocked. Enable them in System Settings → Notifications, then try again."
              : notifyPermission === "prompt"
                ? "Permission isn't decided yet — answer the system prompt, then press Test again."
                : "Couldn't check notification permission. Try again.";
        } else {
          try {
            const raw = await invoke<unknown>("notify_agent", {
              title: test.title,
              body: test.body,
              kind: test.kind,
            });
            const outcome = parseNotifyOutcome(raw);
            if (outcome.status === "denied") notifyPermission = "denied";
            const report = describeNotifyOutcome(outcome, "test");
            if (report.ok) {
              testNotifyNote = report.message;
            } else {
              testNotifyError = report.message;
            }
          } catch (e) {
            console.error("ubra: test notification failed", e);
            testNotifyError =
              "Couldn't show the system notification. Try again.";
          }
        }
      }
      if (route.sound) {
        invoke("play_sound", playbackPayload(test.kind)).catch((e) =>
          console.error("ubra: test sound failed", e),
        );
      }
    } finally {
      testNotifyBusy = false;
    }
  }

  function onDeliveryChange(e: Event): void {
    const value = (e.target as HTMLSelectElement).value;
    if (value === "off" || value === "inapp" || value === "system") {
      const delivery: NotifyDelivery = value;
      store.setNotifyDelivery(delivery);
    }
  }

  function onToastPositionChange(e: Event): void {
    const value = (e.target as HTMLSelectElement).value;
    if (
      value === "top-left" ||
      value === "top-right" ||
      value === "bottom-left" ||
      value === "bottom-right"
    ) {
      const position: ToastPosition = value;
      store.setToastPosition(position);
    }
  }

  function onSoundEnabledChange(e: Event): void {
    store.setSoundEnabled((e.target as HTMLInputElement).checked);
  }

  function onAutoLaunchAgentChange(e: Event): void {
    store.setAutoLaunchAgent((e.target as HTMLInputElement).checked);
  }

  function onTestSound(): void {
    invoke("play_sound", playbackPayload("done"))
      .catch((e) => console.error("ubra: test sound failed", e));
  }

  const REPO_URL = "https://github.com/stackwares/ubra-tauri";

  function openOnboarding(): void {
    store.settingsOpen = false;
    store.onboardingOpen = true;
  }

  function openExternal(url: string): void {
    invoke("plugin:opener|open_url", { url }).catch((e) =>
      console.error("ubra: failed to open link", e),
    );
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
  class="backdrop"
  onclick={(e) => {
    if (e.target === e.currentTarget) close();
  }}
>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Settings"
    tabindex="-1"
    data-keyboard-overlay
    use:overlayFocus
    onkeydown={onDialogKeydown}
  >
    <div class="header">
      <span>Settings</span>
      <button onclick={close} title="Close settings" aria-label="Close settings">
        <Icon name="x" size={14} />
      </button>
    </div>
    <div class="layout">
      <nav class="nav" aria-label="Settings sections">
        {#each SECTIONS as s}
          <button
            class="nav-item"
            class:active={section === s.id}
            aria-current={section === s.id ? "true" : undefined}
            onclick={() => (section = s.id)}
          >
            <Icon name={s.icon} size={14} />
            <span>{s.label}</span>
          </button>
        {/each}
      </nav>
      <div class="content">
        {#if section === "appearance"}
          <section aria-label="Appearance">
            <h2>Appearance</h2>
            <div class="subtabs" role="tablist" aria-label="Appearance settings">
              <button
                role="tab"
                aria-selected={appearanceTab === "theme"}
                class:active={appearanceTab === "theme"}
                onclick={() => (appearanceTab = "theme")}
              >
                Theme
              </button>
              <button
                role="tab"
                aria-selected={appearanceTab === "text"}
                class:active={appearanceTab === "text"}
                onclick={() => (appearanceTab = "text")}
              >
                Text
              </button>
            </div>
            {#if appearanceTab === "theme"}
            <div class="group">
              <h3 class="group-label" id="theme-label">Theme</h3>
              <div class="card flush" role="group" aria-labelledby="theme-label">
                <div class="swatches">
                  {#each THEME_IDS as id}
                    {@const theme = THEMES[id]}
                    <button
                      class="swatch"
                      class:selected={store.themeId === id}
                      aria-pressed={store.themeId === id}
                      aria-label={`${theme.name}${store.themeId === id ? " (current)" : ""}`}
                      title={theme.name}
                      onclick={() => onThemeSelect(id)}
                    >
                      <span
                        class="swatch-preview"
                        style={`background:${theme.ui.appBg};border-color:${theme.ui.border}`}
                        aria-hidden="true"
                      >
                        <span
                          class="swatch-accent"
                          style={`background:${theme.ui.accent}`}
                        ></span>
                        <span class="swatch-dots">
                          <span
                            style={`background:${theme.terminal.red}`}
                          ></span>
                          <span
                            style={`background:${theme.terminal.green}`}
                          ></span>
                          <span
                            style={`background:${theme.terminal.blue}`}
                          ></span>
                        </span>
                      </span>
                      <span class="swatch-name">{theme.name}</span>
                    </button>
                  {/each}
                </div>
              </div>
            </div>
            {:else}
            <div class="group">
              <h3 class="group-label" id="font-label">Interface font</h3>
              <div class="card flush">
                <div class="font-search-row">
                  <input
                    type="search"
                    class="font-search"
                    bind:value={fontQuery}
                    placeholder="Search fonts"
                    aria-label="Search interface fonts"
                  />
                  <button
                    class="btn btn-sm"
                    onclick={() => store.resetUiFont()}
                    disabled={store.uiFontId === DEFAULT_UI_FONT_ID}
                  >
                    Reset
                  </button>
                </div>
                <div
                  class="font-list"
                  role="listbox"
                  aria-label="Interface font"
                  aria-busy={store.uiFontApplying !== null}
                >
                  {#each matchUiFonts(fontQuery) as id (id)}
                    {@const font = UI_FONTS[id]}
                    {@const applying = store.uiFontApplying === id}
                    <button
                      class="font-option"
                      class:selected={store.uiFontId === id}
                      class:applying={applying}
                      role="option"
                      aria-selected={store.uiFontId === id}
                      aria-label={`${font.name}${store.uiFontId === id ? " (current)" : ""}${applying ? " (applying)" : ""}`}
                      onclick={() => store.setUiFont(id)}
                    >
                      <span
                        class="font-sample"
                        style={`font-family:${font.stack}`}
                        aria-hidden="true"
                      >
                        Ag
                      </span>
                      <span
                        class="font-name"
                        style={`font-family:${font.stack}`}
                      >
                        {font.name}
                      </span>
                      <span class="font-category">
                        {applying ? "Applying…" : font.category}
                      </span>
                    </button>
                  {:else}
                    <div class="font-empty">
                      No fonts match &ldquo;{fontQuery}&rdquo;.
                    </div>
                  {/each}
                </div>
              </div>
            </div>
            <div class="group">
              <h3 class="group-label">Interface</h3>
              <div class="card">
                <div class="row">
                  <span class="label">Text size</span>
                  <span class="stepper">
                    <span class="seg">
                      <button
                        onclick={() => store.bumpUiScale(-1)}
                        disabled={store.uiScale <= MIN_UI_SCALE}
                        title="Smaller interface text"
                        aria-label="Smaller interface text"
                      >
                        &minus;
                      </button>
                      <button
                        onclick={() => store.bumpUiScale(1)}
                        disabled={store.uiScale >= MAX_UI_SCALE}
                        title="Bigger interface text"
                        aria-label="Bigger interface text"
                      >
                        +
                      </button>
                    </span>
                    <span class="value">{store.uiScale}%</span>
                    <button
                      class="btn btn-sm"
                      onclick={() => store.resetUiScale()}
                      disabled={store.uiScale === DEFAULT_UI_SCALE}
                    >
                      Reset
                    </button>
                  </span>
                </div>
              </div>
            </div>
            <div class="group">
              <h3 class="group-label">Terminal</h3>
              <div class="card">
                <div class="row">
                  <span class="label">Font size</span>
                  <span class="stepper">
                    <span class="seg">
                      <button
                        onclick={() => store.bumpTermFontSize(-1)}
                        disabled={store.termFontSize <= MIN_TERM_FONT_SIZE}
                        title="Smaller terminal font"
                        aria-label="Smaller terminal font"
                      >
                        &minus;
                      </button>
                      <button
                        onclick={() => store.bumpTermFontSize(1)}
                        disabled={store.termFontSize >= MAX_TERM_FONT_SIZE}
                        title="Bigger terminal font"
                        aria-label="Bigger terminal font"
                      >
                        +
                      </button>
                    </span>
                    <span class="value">{store.termFontSize}px</span>
                    <button
                      class="btn btn-sm"
                      onclick={() => store.resetTermFontSize()}
                      disabled={store.termFontSize === DEFAULT_TERM_FONT_SIZE}
                    >
                      Reset
                    </button>
                  </span>
                </div>
                <label class="row">
                  <span class="label">Scrollback</span>
                  <span class="select-wrap">
                    <select
                      value={store.termScrollback}
                      onchange={(e) =>
                        store.setTermScrollback(
                          Number((e.target as HTMLSelectElement).value),
                        )}
                      aria-label="Terminal scrollback"
                    >
                      {#each SCROLLBACK_OPTIONS as lines (lines)}
                        <option value={lines}>{lines} lines</option>
                      {/each}
                    </select>
                  </span>
                </label>
                <div class="row">
                  <span class="label">Background opacity</span>
                  <span class="stepper">
                    <input
                      type="range"
                      class="opacity-slider"
                      min={MIN_TERM_OPACITY}
                      max={MAX_TERM_OPACITY}
                      step="1"
                      value={store.termOpacity}
                      oninput={(e) =>
                        store.setTermOpacity(
                          Number((e.target as HTMLInputElement).value),
                        )}
                      aria-label="Background opacity"
                    />
                    <span class="value">{store.termOpacity}%</span>
                  </span>
                </div>
              </div>
            </div>
            {/if}
          </section>
        {:else if section === "alerts"}
          <section aria-label="Alerts">
            <h2>Alerts</h2>
            <div class="group">
              <h3 class="group-label">Notifications</h3>
              <div class="card">
                <label class="row">
                  <span class="label">Agent finished</span>
                  <span class="select-wrap">
                    <select
                      value={store.notifyDelivery}
                      onchange={onDeliveryChange}
                      aria-label="Notification delivery"
                    >
                      <option value="system">System notification</option>
                      <option value="inapp">In-app toast</option>
                      <option value="off">Off</option>
                    </select>
                  </span>
                </label>
                {#if store.notifyDelivery === "inapp"}
                  <label class="row">
                    <span class="label">Toast position</span>
                    <span class="select-wrap">
                      <select
                        value={store.toastPosition}
                        onchange={onToastPositionChange}
                        aria-label="Toast position"
                      >
                        <option value="top-left">Top left</option>
                        <option value="top-right">Top right</option>
                        <option value="bottom-left">Bottom left</option>
                        <option value="bottom-right">Bottom right</option>
                      </select>
                    </span>
                  </label>
                {/if}
                <div class="row">
                  <span class="label">Test with current settings</span>
                  <button
                    class="btn"
                    disabled={testNotifyBusy}
                    onclick={() => void sendTestNotification()}
                  >
                    <Icon name="bell" size={12} />
                    <span>{testNotifyBusy ? "Waiting…" : "Test"}</span>
                  </button>
                </div>
                {#if store.notifyDelivery === "system"}
                  <div class="hint">System permission: {notifyPermissionLabel}</div>
                {/if}
                {#if testNotifyError}
                  <div class="hint">
                    <span class="error-hint" role="alert">{testNotifyError}</span>
                  </div>
                {:else if testNotifyNote}
                  <div class="hint">
                    <span class="ok-hint" role="status">{testNotifyNote}</span>
                  </div>
                {/if}
              </div>
              <div class="hint">
                Fires a sample Codex finish through the settings above.
              </div>
            </div>
            <div class="group">
              <h3 class="group-label">Sounds</h3>
              <div class="card">
                <label class="row switch">
                  <span class="label">Play sound when an agent finishes</span>
                  <input
                    type="checkbox"
                    checked={store.soundEnabled}
                    onchange={onSoundEnabledChange}
                  />
                  <span class="track" aria-hidden="true">
                    <span class="thumb"></span>
                  </span>
                </label>
                <div class="row">
                  <span class="label">Preview</span>
                  <button class="btn" onclick={onTestSound}>
                    <Icon name="play" size={12} />
                    <span>Play</span>
                  </button>
                </div>
              </div>
            </div>
          </section>
        {:else if section === "shortcuts"}
          <section aria-label="Shortcuts">
            <h2>Shortcuts</h2>
            <div class="group">
              <div class="card">
                {#each shortcuts as s}
                  <div class="row">
                    <span class="label">{s.blurb}</span>
                    <span><kbd>{s.keys}</kbd></span>
                  </div>
                {/each}
              </div>
            </div>
          </section>
        {:else if section === "app"}
          <section aria-label="App">
            <h2>App</h2>
            <div class="group">
              <h3 class="group-label">General</h3>
              <div class="card">
                <label class="row switch">
                  <span class="label">Launch at login</span>
                  <input
                    type="checkbox"
                    checked={autostart}
                    disabled={!autostartLoaded}
                    onchange={onAutostartChange}
                  />
                  <span class="track" aria-hidden="true">
                    <span class="thumb"></span>
                  </span>
                </label>
                {#if autostartError}
                  <div class="hint error-hint" role="alert">
                    {autostartError}
                  </div>
                {/if}
                <label class="row switch">
                  <span class="label">Auto-launch agent in new terminals</span>
                  <input
                    type="checkbox"
                    checked={store.autoLaunchAgent}
                    onchange={onAutoLaunchAgentChange}
                  />
                  <span class="track" aria-hidden="true">
                    <span class="thumb"></span>
                  </span>
                </label>
                <div class="row">
                  <span class="label">Setup walkthrough</span>
                  <button class="btn" onclick={openOnboarding}>
                    <span>Open onboarding</span>
                  </button>
                </div>
              </div>
              <div class="hint">
                When on, new panes, tabs, and workspaces launch the default
                agent automatically.
              </div>
            </div>
            <div class="group">
              <h3 class="group-label">Updates</h3>
              <div class="card">
                <div class="row">
                  <span class="label">
                    {#if updater.phase === "available" && updater.version}
                      Ubra {updater.version} is available
                    {:else if updater.phase === "downloading"}
                      {#if updater.totalBytes}
                        Downloading… {formatBytes(updater.downloadedBytes)} of
                        {formatBytes(updater.totalBytes)}
                      {:else}
                        Downloading… {formatBytes(updater.downloadedBytes)}
                      {/if}
                    {:else if updater.phase === "ready"}
                      Update installed — relaunch to apply it
                    {:else if appVersion}
                      Ubra {appVersion}
                    {:else}
                      Check for updates
                    {/if}
                  </span>
                  {#if updater.phase === "ready"}
                    <button
                      class="btn"
                      onclick={() => void updater.relaunchApp()}
                    >
                      <Icon name="refresh" size={12} />
                      <span>Relaunch</span>
                    </button>
                  {:else if updater.phase === "available"}
                    <button
                      class="btn"
                      onclick={() => void updater.downloadAndInstall()}
                    >
                      Download and install
                    </button>
                  {:else}
                    <button
                      class="btn"
                      disabled={updater.phase === "checking" ||
                        updater.phase === "downloading"}
                      onclick={() => void updater.checkForUpdates()}
                    >
                      {#if updater.phase === "checking"}
                        <Spinner size={12} />
                      {:else}
                        <Icon name="refresh" size={12} />
                      {/if}
                      <span>
                        {updater.phase === "checking"
                          ? "Checking…"
                          : "Check for updates"}
                      </span>
                    </button>
                  {/if}
                </div>
                {#if updater.phase === "error" && updater.error}
                  <div class="hint error-hint" role="alert">
                    {updater.error}
                  </div>
                {:else if updater.phase === "idle" && updater.checked}
                  <div class="hint">You&rsquo;re up to date.</div>
                {:else if updater.phase === "available" && updater.notes}
                  <div class="hint">{updater.notes}</div>
                {/if}
              </div>
            </div>
            {#if telemetrySupported}
              <div class="group">
                <h3 class="group-label">Telemetry</h3>
                <div class="card">
                  <label class="row switch">
                    <span class="label">Share anonymous usage and crash reports</span>
                    <input
                      type="checkbox"
                      checked={telemetryConsented}
                      disabled={!telemetryLoaded}
                      onchange={onTelemetryChange}
                    />
                    <span class="track" aria-hidden="true">
                      <span class="thumb"></span>
                    </span>
                  </label>
                  {#if telemetryError}
                    <div class="hint error-hint" role="alert">
                      {telemetryError}
                    </div>
                  {/if}
                </div>
                <div class="hint">
                  Helps improve Ubra. Anonymous events and crash reports only —
                  never code, file paths, or commands. Takes effect immediately.
                </div>
              </div>
            {/if}
            <div class="group about-block">
              <img
                class="about-logo"
                src="/logo.png"
                alt="Ubra"
                width="2172"
                height="724"
              />
              <div class="about">
                {appName}{#if appVersion} v{appVersion}{/if}
              </div>
              <div class="about-sub">Agent runtime desktop app</div>
              <div class="links">
                <button class="link" onclick={() => openExternal(REPO_URL)}>
                  GitHub
                </button>
                <button
                  class="link"
                  onclick={() => openExternal(`${REPO_URL}/issues`)}
                >
                  Report an issue
                </button>
                <button
                  class="link"
                  onclick={() => openExternal(`${REPO_URL}/releases`)}
                >
                  Releases
                </button>
                <button
                  class="link"
                  onclick={() => openExternal(`${REPO_URL}/blob/main/LICENSE`)}
                >
                  MIT License
                </button>
              </div>
              <div class="about-sub">© 2026 Ubra</div>
            </div>
          </section>
        {:else if section === "workspace"}
          <section aria-label="Workspace">
            <h2>Workspace</h2>
            {#if store.workspace()}
              {@const ws = store.workspace()!}
              <div class="hint intro">
                Defaults for <strong>{ws.name}</strong> — every new tab and
                pane in this workspace starts here.
              </div>
              <div class="group">
                <h3 class="group-label">Defaults</h3>
                <div class="card">
                  {#if agentClis.clis === null}
                    <div class="row">
                      <span class="label">Default agent CLI</span>
                      <span class="hint">Detecting installed agents…</span>
                    </div>
                  {:else}
                    <div class="row">
                      <span class="label">Default agent CLI</span>
                      <span class="cli-select">
                        <AgentCliSelect
                          entries={agentClis.clis}
                          bind:value={cliSelection}
                          noneLabel="None (plain shells)"
                          ariaLabel="Default agent CLI"
                          onChange={onDefaultCliSelect}
                        />
                      </span>
                    </div>
                    {#if cliSelection === CUSTOM_COMMAND}
                      <label class="row">
                        <span class="label">Custom command</span>
                        <input
                          type="text"
                          value={cliCustom}
                          oninput={onDefaultCliCustom}
                          placeholder="my-agent --yes"
                          autocomplete="off"
                          autocapitalize="off"
                          spellcheck="false"
                          aria-label="Custom default command"
                          class="file-input"
                        />
                      </label>
                    {/if}
                  {/if}
                  <div class="folder-block">
                    <div class="row">
                      <span class="label">Default folder</span>
                      <span class="folder-actions">
                        <button
                          class="btn"
                          onclick={pickDefaultFolder}
                          disabled={folderPicking}
                        >
                          {folderPicking ? "Opening…" : "Change…"}
                        </button>
                      </span>
                    </div>
                    <div class="folder-path" title={ws.defaultCwd ?? ""}>
                      {ws.defaultCwd ?? "Default directory"}
                    </div>
                  </div>
                  {#if folderError}
                    <div class="hint">
                      <span class="error-hint" role="alert">
                        {folderError}
                      </span>
                    </div>
                  {/if}
                </div>
                <div class="hint">
                  When auto-launch is on, the default CLI runs automatically
                  in new tabs and panes. Applies to new panes only.
                </div>
              </div>
            {:else}
              <div class="hint">No workspace open.</div>
            {/if}
          </section>
        {:else if section === "usage"}
          <section aria-label="Usage">
            <div class="section-head">
              <h2>Usage</h2>
              {#if usageShowable && usageShowable.length > 0}
                <button
                  class="btn btn-icon"
                  title="Refresh all usage"
                  aria-label="Refresh all usage"
                  aria-busy={usageAnyLoading}
                  disabled={usageAnyLoading}
                  onclick={refreshAllUsage}
                >
                  {#if usageAnyLoading}
                    <Spinner size={12} />
                  {:else}
                    <Icon name="refresh" size={12} />
                  {/if}
                </button>
              {/if}
            </div>
            {#if usageShowable === null}
              <div class="hint">Loading supported usage providers…</div>
            {:else}
              {@const showable = usageShowable}
              {@const supportedLabels = (usage.supported ?? []).map(
                (entry) => entry.label,
              )}
              {#if showable.length === 0}
                <div class="hint">
                  Plan usage is available for {joinLabels(
                    supportedLabels,
                  )}. Sign in with a supported CLI to view it.
                </div>
              {:else}
                {#each showable as entry (entry.cli)}
                  {@const snap = usage.entries[entry.cli]}
                  {@const busy = usage.loading[entry.cli] === true}
                  <div class="group">
                    <h3 class="group-label">{entry.label}</h3>
                    <div class="card" aria-busy={busy}>
                      <div class="row">
                        <span class="usage-path" title={entry.path}>
                          {entry.path}
                        </span>
                      </div>
                      {#if !snap}
                        <div class="hint">Loading usage…</div>
                      {:else if snap.status === "ready" && snap.snapshot}
                        {@const shot = snap.snapshot}
                        {#each shot.windows as window (window.label)}
                          <div class="usage-window">
                            <div class="usage-head">
                              <span class="label">{window.label}</span>
                              <span class="usage-value">
                                {#if window.percentUsed !== undefined}
                                  {window.percentUsed.toFixed(0)}% used
                                {/if}
                                {#if window.percentUsed !== undefined && window.resetsAt !== undefined}
                                  ·
                                {/if}
                                {#if window.resetsAt !== undefined}
                                  resets {formatResetCountdown(window.resetsAt)}
                                {/if}
                              </span>
                            </div>
                            {#if window.percentUsed !== undefined}
                              <div
                                class="usage-bar"
                                role="progressbar"
                                aria-label={window.label}
                                aria-valuenow={Math.round(window.percentUsed)}
                                aria-valuemin={0}
                                aria-valuemax={100}
                              >
                                <span
                                  style={`width:${Math.min(100, window.percentUsed)}%`}
                                ></span>
                              </div>
                            {/if}
                          </div>
                        {/each}
                        <div class="hint">
                          {shot.source}{#if shot.plan} · {shot.plan}{/if} · Updated
                          {formatUpdatedAgo(shot.fetchedAt)}
                        </div>
                      {:else}
                        <div class="hint">
                          {snap.message ?? "Usage unavailable."}
                        </div>
                      {/if}
                    </div>
                  </div>
                {/each}
              {/if}
            {/if}
          </section>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 2000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
  }
  .dialog {
    width: 660px;
    /* Fixed size so switching tabs never resizes the dialog: sized to fit
       the Appearance tab (theme grid + font row); shorter tabs get
       whitespace, longer ones scroll inside .content. */
    height: 560px;
    max-width: calc(100vw - 48px);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--app-bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
    font: 12px var(--font-ui);
    color: var(--text);
    overflow: hidden;
    outline: none;
  }
  .dialog :focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 8px 10px 16px;
    border-bottom: 1px solid var(--border);
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 600;
  }
  .header button {
    display: inline-flex;
    align-items: center;
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 4px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .header button:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .layout {
    display: flex;
    min-height: 0;
    flex: 1;
  }
  .nav {
    flex: 0 0 184px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 10px;
    border-right: 1px solid var(--border);
    background: var(--sidebar-bg);
    overflow-y: auto;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    text-align: left;
    padding: 8px 10px;
    cursor: pointer;
  }
  .nav-item:hover {
    background: var(--surface-bg);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--surface-active);
    color: var(--text-strong);
    font-weight: 600;
  }
  .content {
    flex: 1;
    min-width: 0;
    padding: 18px 20px 24px;
    overflow-y: auto;
  }
  h2 {
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.011em;
    color: var(--text-strong);
    margin: 2px 0 14px;
  }
  /* Section title row with a trailing action (Usage refresh). */
  .section-head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }
  .section-head h2 {
    flex: 1;
  }
  .section-head .btn-icon {
    margin-top: 4px;
  }
  /* In-section sub-tabs (Appearance Theme/Text). */
  .subtabs {
    display: flex;
    gap: 2px;
    padding: 2px;
    margin: 0 0 14px;
    background: var(--surface-bg);
    border: 1px solid var(--separator);
    border-radius: 8px;
  }
  .subtabs button {
    flex: 1 1 0;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-weight: 600;
    text-align: center;
    padding: 5px 10px;
    cursor: pointer;
  }
  .subtabs button:hover {
    color: var(--text);
  }
  .subtabs button.active {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  /* Grouped card layout in the macOS System Settings idiom: a small label
     above each card, hairline dividers between rows, helper text below. */
  .group {
    margin: 0 0 18px;
  }
  .group:last-child {
    margin-bottom: 0;
  }
  .group-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-strong);
    margin: 0 14px 6px;
  }
  .card {
    background: var(--surface-bg);
    border: 1px solid var(--separator);
    border-radius: 10px;
    padding: 4px 14px;
  }
  .card.flush {
    padding: 8px;
  }
  .card > .row + .row,
  .card > .row + .folder-block,
  .card > .hint + .row {
    border-top: 1px solid var(--separator);
  }
  .card > .row + .usage-window,
  .card > .usage-window + .usage-window {
    border-top: 1px solid var(--separator);
  }
  .usage-window {
    padding: 8px 0;
  }
  .usage-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .usage-head .label {
    flex: 1;
  }
  .usage-value {
    color: var(--text-muted);
    font-size: 11px;
    white-space: nowrap;
  }
  .usage-bar {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-active);
    margin-top: 6px;
    overflow: hidden;
  }
  .usage-bar > span {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 0;
  }
  label.row {
    cursor: pointer;
  }
  .row .label {
    flex: 1;
  }
  /* Trailing switch row: native checkbox, custom track on the right. */
  .switch input {
    position: absolute;
    opacity: 0;
    width: 1px;
    height: 1px;
  }
  .switch .track {
    flex: 0 0 auto;
    width: 32px;
    height: 18px;
    border-radius: 9px;
    background: var(--surface-active);
    border: 1px solid var(--input-border);
    position: relative;
    transition: background 120ms ease;
  }
  .switch .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-muted);
    transition:
      left 120ms ease,
      background 120ms ease;
  }
  .switch input:checked + .track {
    background: var(--accent);
    border-color: var(--accent);
  }
  .switch input:checked + .track .thumb {
    left: 16px;
    background: #fff;
  }
  .switch input:focus-visible + .track {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .switch:has(input:disabled) {
    opacity: 0.55;
    cursor: default;
  }
  /* Native select with themed frame and chevron. */
  .select-wrap {
    position: relative;
    display: inline-flex;
  }
  .select-wrap::after {
    content: "";
    position: absolute;
    right: 10px;
    top: 50%;
    width: 7px;
    height: 7px;
    border-right: 1.5px solid var(--text-muted);
    border-bottom: 1.5px solid var(--text-muted);
    transform: translateY(-70%) rotate(45deg);
    pointer-events: none;
  }
  select {
    appearance: none;
    border: 1px solid var(--input-border);
    border-radius: 6px;
    padding: 5px 26px 5px 9px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    max-width: 220px;
    cursor: pointer;
  }
  .stepper {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  /* Joined -/+ segment. */
  .seg {
    display: inline-flex;
  }
  .seg button {
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    min-width: 30px;
    padding: 3px 10px;
    cursor: pointer;
  }
  .seg button:first-child {
    border-radius: 6px 0 0 6px;
  }
  .seg button:last-child {
    border-radius: 0 6px 6px 0;
    margin-left: -1px;
  }
  .seg button:hover:not(:disabled) {
    background: var(--surface-bg);
    color: var(--text-strong);
  }
  .seg button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .stepper .value {
    min-width: 44px;
    text-align: center;
    color: var(--text-strong);
  }
  .opacity-slider {
    width: 120px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  /* Single button style for every row action. */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid var(--input-border);
    border-radius: 6px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    padding: 4px 12px;
    cursor: pointer;
    white-space: nowrap;
  }
  .btn:hover:not(:disabled) {
    background: var(--surface-bg);
    color: var(--text-strong);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .btn-sm {
    padding: 2px 10px;
    font-size: 11px;
  }
  /* Square icon-only action; keeps its accessible name in markup. */
  .btn-icon {
    padding: 5px 8px;
  }
  .file-input {
    border: 1px solid var(--input-border);
    border-radius: 6px;
    padding: 5px 9px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    width: 220px;
  }
  /* Capped dropdown width so the row label keeps its natural width. */
  .cli-select {
    display: flex;
    flex: 0 1 220px;
    min-width: 0;
  }
  /* Default folder: label + actions on one line, full-width path below. */
  .folder-block {
    padding: 8px 0;
  }
  .folder-block .row {
    padding: 0 0 6px;
  }
  .folder-actions {
    display: flex;
    gap: 8px;
    flex: 0 0 auto;
  }
  .folder-path,
  .usage-path {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--text-muted);
    font: 11px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .usage-path {
    flex: 1;
    min-width: 0;
  }
  .hint {
    font-size: 11px;
    color: var(--text-muted);
    padding: 2px 0 4px;
  }
  .hint.intro {
    font-size: 12px;
    color: var(--text);
    margin: 0 0 12px;
    padding: 0;
  }
  /* Helper text under a card aligns with the card's content. */
  .card + .hint {
    margin: 6px 14px 0;
    padding: 0;
  }
  .card > .hint {
    padding: 4px 0 8px;
  }
  .error-hint {
    color: var(--error-text);
  }
  .ok-hint {
    color: var(--success);
  }
  /* Theme swatches. */
  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: 8px;
    margin: 0;
  }
  .swatch {
    display: flex;
    flex-direction: column;
    gap: 5px;
    align-items: stretch;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    padding: 6px;
    cursor: pointer;
    font: inherit;
    color: var(--text-muted);
  }
  .swatch:hover {
    background: var(--surface-active);
    color: var(--text);
  }
  .swatch.selected {
    border-color: var(--accent);
    color: var(--text-strong);
  }
  .swatch-preview {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 34px;
    border: 1px solid;
    border-radius: 5px;
    padding: 0 8px;
  }
  .swatch-accent {
    width: 22px;
    height: 8px;
    border-radius: 4px;
  }
  .swatch-dots {
    display: flex;
    gap: 4px;
  }
  .swatch-dots span {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .swatch-name {
    font-size: 11px;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Interface font picker. */
  .font-search-row {
    display: flex;
    gap: 8px;
    padding: 0 2px 8px;
  }
  .font-search {
    flex: 1;
    min-width: 0;
    border: 1px solid var(--input-border);
    border-radius: 6px;
    padding: 5px 9px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
  }
  .font-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 168px;
    overflow-y: auto;
    border-top: 1px solid var(--separator);
    padding: 8px 2px 2px;
    margin: 0;
  }
  .font-option {
    display: flex;
    align-items: baseline;
    gap: 10px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    padding: 5px 10px;
    cursor: pointer;
  }
  .font-option:hover {
    background: var(--surface-active);
  }
  .font-option.selected {
    border-color: var(--accent);
    color: var(--text-strong);
  }
  .font-sample {
    flex: 0 0 auto;
    width: 26px;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-strong);
  }
  .font-name {
    flex: 1;
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .font-category {
    flex: 0 0 auto;
    font-size: 11px;
    color: var(--text-muted);
  }
  .font-option.applying .font-category {
    color: var(--accent);
  }
  .font-empty {
    padding: 10px;
    color: var(--text-muted);
    font-size: 11px;
    text-align: center;
  }
  kbd {
    display: inline-block;
    min-width: 110px;
    text-align: center;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 11px;
    color: var(--text-strong);
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 5px;
    padding: 2px 8px;
  }
  .about-block {
    text-align: center;
    padding-top: 4px;
  }
  .about {
    color: var(--text-strong);
    font-size: 13px;
  }
  .about-logo {
    display: block;
    height: 42px;
    width: auto;
    margin: 2px auto 10px;
  }
  .about-block .links {
    justify-content: center;
  }
  .about-sub {
    color: var(--text-subtle);
    margin-top: 2px;
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    margin: 10px 0;
  }
  .link {
    background: none;
    border: none;
    padding: 2px 0;
    font: inherit;
    color: var(--accent);
    cursor: pointer;
  }
  .link:hover {
    color: var(--accent-hover);
    text-decoration: underline;
  }
  @media (max-width: 600px) {
    .layout {
      flex-direction: column;
    }
    .nav {
      flex: none;
      flex-direction: row;
      border-right: none;
      border-bottom: 1px solid var(--border);
      overflow-x: auto;
      padding: 8px;
    }
    .nav-item {
      white-space: nowrap;
    }
  }
</style>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { agent } from "./agent.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import { overlayFocus } from "./overlayFocus";
  import {
    CUSTOM_CHIME_ID,
    playbackPayload,
    routeNotification,
    testNotificationPayload,
    type NotifyDelivery,
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
  import { toasts } from "./toasts.svelte.ts";
  import { THEMES, THEME_IDS, isThemeId } from "./themes";

  type SectionId =
    | "appearance"
    | "alerts"
    | "shortcuts"
    | "app";

  const SECTIONS: { id: SectionId; label: string; icon: IconName }[] = [
    { id: "appearance", label: "Appearance", icon: "palette" },
    { id: "alerts", label: "Alerts", icon: "bell" },
    { id: "shortcuts", label: "Shortcuts", icon: "command" },
    { id: "app", label: "App", icon: "info" },
  ];

  let section = $state<SectionId>("app");

  let autostart = $state(false);
  let autostartLoaded = $state(false);
  let autostartError = $state<string | null>(null);
  let appName = $state("Ubra");
  let appVersion = $state("");
  let soundFile = $state(store.soundFile);
  /** null = blank (default chime) or unchecked; otherwise backend verdict. */
  let soundFileValid = $state<boolean | null>(null);
  let soundFileChecking = $state(false);

  const shortcuts = cheatSheet(isMacPlatform(navigator.platform));

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
    invoke<boolean>("autostart_enabled")
      .then((v) => {
        autostart = v;
        autostartLoaded = true;
      })
      .catch((e) => console.error("ubra: autostart check failed", e));
    invoke<{ name: string; version: string }>("app_info")
      .then((info) => {
        appName = info.name;
        appVersion = info.version;
      })
      .catch((e) => console.error("ubra: app info failed", e));
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

  function onThemeSelect(id: string): void {
    if (isThemeId(id)) store.setTheme(id);
  }

  /**
   * Fire one sample agent-finish through the current delivery/sound/mute
   * settings, exactly like a real finish (muted Codex stays silent).
   */
  function sendTestNotification(): void {
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
      invoke("notify_agent", {
        title: test.title,
        body: test.body,
        kind: test.kind,
      }).catch((e) => console.error("ubra: test notification failed", e));
    }
    if (route.sound) {
      invoke("play_sound", playbackPayload(test.kind, store.soundStyle, store.soundFile))
        .catch((e) => console.error("ubra: test sound failed", e));
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

  function onSoundFileChange(): void {
    store.setSoundFile(soundFile);
    if (soundFile.trim() === "") {
      soundFileValid = null;
      soundFileChecking = false;
      return;
    }
    soundFileValid = null;
    soundFileChecking = true;
    invoke<boolean>("check_sound_file", { path: soundFile })
      .then((ok) => {
        soundFileValid = ok;
        soundFileChecking = false;
      })
      .catch(() => {
        soundFileValid = false;
        soundFileChecking = false;
      });
  }

  function onTestSound(): void {
    invoke("play_sound", playbackPayload("done", store.soundStyle, store.soundFile))
      .catch((e) => console.error("ubra: test sound failed", e));
  }

  function onSoundStyleChange(e: Event): void {
    store.setSoundStyle((e.target as HTMLSelectElement).value);
  }

  const REPO_URL = "https://github.com/stackwares/ubra-tauri";

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
      <button onclick={close} aria-label="Close settings">
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
            <Icon name={s.icon} size={13} />
            <span>{s.label}</span>
          </button>
        {/each}
      </nav>
      <div class="content">
        {#if section === "appearance"}
          <section aria-label="Appearance">
            <h2>Appearance</h2>
            <div class="row">
              <span class="label">Terminal font size</span>
              <span class="stepper">
                <button
                  onclick={() => store.bumpTermFontSize(-1)}
                  disabled={store.termFontSize <= MIN_TERM_FONT_SIZE}
                  aria-label="Smaller terminal font"
                >
                  &minus;
                </button>
                <span class="value">{store.termFontSize}px</span>
                <button
                  onclick={() => store.bumpTermFontSize(1)}
                  disabled={store.termFontSize >= MAX_TERM_FONT_SIZE}
                  aria-label="Bigger terminal font"
                >
                  +
                </button>
                <button
                  class="reset"
                  onclick={() => store.resetTermFontSize()}
                  disabled={store.termFontSize === DEFAULT_TERM_FONT_SIZE}
                >
                  Reset
                </button>
              </span>
            </div>
            <div class="row">
              <span class="label">Interface scale</span>
              <span class="stepper">
                <button
                  onclick={() => store.bumpUiScale(-1)}
                  disabled={store.uiScale <= MIN_UI_SCALE}
                  aria-label="Smaller interface text"
                >
                  &minus;
                </button>
                <span class="value">{store.uiScale}%</span>
                <button
                  onclick={() => store.bumpUiScale(1)}
                  disabled={store.uiScale >= MAX_UI_SCALE}
                  aria-label="Bigger interface text"
                >
                  +
                </button>
                <button
                  class="reset"
                  onclick={() => store.resetUiScale()}
                  disabled={store.uiScale === DEFAULT_UI_SCALE}
                >
                  Reset
                </button>
              </span>
            </div>
            <label class="row">
              <span class="label">Terminal scrollback</span>
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
            <div class="field-label" id="theme-label">Theme</div>
            <div
              class="swatches"
              role="group"
              aria-labelledby="theme-label"
            >
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
                      <span style={`background:${theme.terminal.red}`}></span>
                      <span style={`background:${theme.terminal.green}`}></span>
                      <span style={`background:${theme.terminal.blue}`}></span>
                    </span>
                  </span>
                  <span class="swatch-name">{theme.name}</span>
                </button>
              {/each}
            </div>
          </section>
        {:else if section === "alerts"}
          <section aria-label="Alerts">
            <h2>Alerts</h2>
            <h3 class="subheading">Notifications</h3>
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
              <button class="test-btn" onclick={sendTestNotification}>
                <Icon name="bell" size={12} />
                <span>Send test notification</span>
              </button>
            </div>
            <div class="hint">
              Fires a sample Codex finish through the settings above.
            </div>
            <div class="subsection">
              <h3 class="subheading">Sounds</h3>
              <label class="toggle">
                <input
                  type="checkbox"
                  checked={store.soundEnabled}
                  onchange={onSoundEnabledChange}
                />
                <span class="track" aria-hidden="true">
                  <span class="thumb"></span>
                </span>
                <span>Play sound when an agent finishes</span>
              </label>
              <label class="row">
                <span class="label">Chime</span>
                <span class="select-wrap">
                  <select
                    value={store.soundStyle}
                    onchange={onSoundStyleChange}
                    aria-label="Notification chime"
                  >
                    <option value="default">Default chime</option>
                    <option value="bright">Bright</option>
                    <option value="soft">Soft</option>
                    <option value="pop">Pop</option>
                    <option value={CUSTOM_CHIME_ID}>Custom audio file…</option>
                  </select>
                </span>
              </label>
              {#if store.soundStyle === CUSTOM_CHIME_ID}
                <label class="row">
                  <span class="label">Custom sound</span>
                  <input
                    type="text"
                    bind:value={soundFile}
                    onchange={onSoundFileChange}
                    placeholder="~/Music/chime.mp3"
                    aria-label="Custom sound file path"
                    aria-invalid={soundFileValid === false}
                    aria-describedby={soundFile.trim() !== "" ? "sound-hint" : undefined}
                    class="file-input"
                  />
                </label>
                {#if soundFile.trim() !== ""}
                  <div class="hint" id="sound-hint">
                    {#if soundFileChecking}
                      <span class="checking">Checking file…</span>
                    {:else if soundFileValid === false}
                      <span class="error-hint" role="alert">
                        File not found or not playable audio; default chime is used.
                      </span>
                    {:else if soundFileValid === true}
                      <span class="ok-hint">Custom sound ready.</span>
                    {/if}
                  </div>
                {/if}
              {/if}
              <div class="row">
                <span class="label">Preview</span>
                <button class="test-button" onclick={onTestSound}>
                  <Icon name="play" size={12} />
                  <span>Play test sound</span>
                </button>
              </div>
            </div>
          </section>
        {:else if section === "shortcuts"}
          <section aria-label="Shortcuts">
            <h2>Shortcuts</h2>
            <div class="shortcuts">
              {#each shortcuts as s}
                <div class="shortcut">
                  <span><kbd>{s.keys}</kbd></span>
                  <span>{s.blurb}</span>
                </div>
              {/each}
            </div>
          </section>
        {:else if section === "app"}
          <section aria-label="App">
            <h2>App</h2>
            <label class="toggle">
              <input
                type="checkbox"
                checked={autostart}
                disabled={!autostartLoaded}
                onchange={onAutostartChange}
              />
              <span class="track" aria-hidden="true">
                <span class="thumb"></span>
              </span>
              <span>Launch at login</span>
            </label>
            {#if autostartError}
              <div class="hint error-hint" role="alert">{autostartError}</div>
            {/if}
            <div class="subsection">
              <h3 class="subheading">Onboarding</h3>
              <div class="row">
                <span class="label">Set up another project and agent session</span>
                <button class="test-btn" onclick={() => store.openOnboarding()}>
                  <Icon name="layers" size={12} />
                  <span>Run onboarding</span>
                </button>
              </div>
              <div class="hint">
                Completing setup opens a new workspace and keeps your current work.
              </div>
            </div>
            <div class="subsection">
              <h3 class="subheading">Quit</h3>
              <div class="row">
                <span class="label">Quit Ubra and terminate owned pane processes</span>
                <button class="test-btn" onclick={() => {
                  invoke("quit_app").catch((error) =>
                    toasts.push("Quit failed", String(error), "", { kind: "copy" }));
                }}>Quit Ubra</button>
              </div>
            </div>
            <div class="subsection">
              <h3 class="subheading">About</h3>
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
              <div class="about-sub">© 2026 Oliver Martinez</div>
            </div>
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
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
    font: 12px system-ui, sans-serif;
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
    flex: 0 0 172px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 8px;
    border-right: 1px solid var(--border);
    background: var(--sidebar-bg);
    overflow-y: auto;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    text-align: left;
    padding: 7px 10px;
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
    padding: 14px 18px 18px;
    overflow-y: auto;
  }
  h2 {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-strong);
    margin: 0 0 10px;
  }
  .subheading {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
    margin: 0 0 4px;
  }
  .subsection {
    margin-top: 16px;
    padding-top: 12px;
    border-top: 1px solid var(--separator);
  }
  .field-label {
    font-size: 12px;
    color: var(--text);
    margin: 0 0 8px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 7px 0;
  }
  label.row {
    cursor: pointer;
  }
  .row .label {
    flex: 1;
  }
  /* Toggle switch: native checkbox, custom track. */
  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 0;
    cursor: pointer;
  }
  .toggle input {
    position: absolute;
    opacity: 0;
    width: 1px;
    height: 1px;
  }
  .toggle .track {
    flex: 0 0 auto;
    width: 32px;
    height: 18px;
    border-radius: 9px;
    background: var(--surface-active);
    border: 1px solid var(--input-border);
    position: relative;
    transition: background 120ms ease;
  }
  .toggle .thumb {
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
  .toggle input:checked + .track {
    background: var(--accent);
    border-color: var(--accent);
  }
  .toggle input:checked + .track .thumb {
    left: 16px;
    background: #fff;
  }
  .toggle input:focus-visible + .track {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .toggle input:disabled ~ span:last-child {
    opacity: 0.5;
  }
  .toggle input:disabled + .track {
    opacity: 0.5;
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
    gap: 6px;
  }
  .stepper button {
    border: 1px solid var(--input-border);
    border-radius: 6px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    min-width: 28px;
    padding: 3px 9px;
    cursor: pointer;
  }
  .stepper button:hover:not(:disabled) {
    background: var(--surface-bg);
    color: var(--text-strong);
  }
  .stepper button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .stepper .value {
    min-width: 40px;
    text-align: center;
    color: var(--text-strong);
  }
  .opacity-slider {
    width: 120px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .test-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    padding: 4px 12px;
    cursor: pointer;
  }
  .test-btn:hover {
    background: var(--surface-bg);
    color: var(--text-strong);
  }
  .stepper .reset {
    margin-left: 4px;
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
  .file-input[aria-invalid="true"] {
    border-color: var(--error-text);
  }
  .hint {
    font-size: 11px;
    padding: 2px 0 4px;
  }
  .error-hint {
    color: var(--error-text);
  }
  .checking {
    color: var(--text-muted);
  }
  .ok-hint {
    color: var(--success);
  }
  .test-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--input-border);
    border-radius: 6px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    padding: 4px 12px;
    cursor: pointer;
  }
  .test-button:hover {
    background: var(--surface-bg);
    color: var(--text-strong);
  }
  /* Theme swatches. */
  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: 8px;
    margin-bottom: 8px;
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
    background: var(--surface-bg);
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
  .shortcuts {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .shortcut {
    display: flex;
    align-items: center;
    gap: 12px;
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
  .about {
    color: var(--text-strong);
    font-size: 13px;
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

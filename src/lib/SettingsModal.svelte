<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { agent } from "./agent.svelte";
  import type { NotifyDelivery, ToastPosition } from "./notify";
  import { cheatSheet, isMacPlatform } from "./shortcuts";
  import {
    DEFAULT_TERM_FONT_SIZE,
    MAX_TERM_FONT_SIZE,
    MIN_TERM_FONT_SIZE,
    store,
  } from "./store.svelte";
  import { THEMES, THEME_IDS, isThemeId } from "./themes";

  let autostart = $state(false);
  let autostartLoaded = $state(false);
  let appName = $state("Ubra");
  let appVersion = $state("");
  let soundFile = $state(store.soundFile);
  /** null = blank (default chime) or unchecked; otherwise backend verdict. */
  let soundFileValid = $state<boolean | null>(null);

  const shortcuts = cheatSheet(isMacPlatform(navigator.platform));

  function focus(el: HTMLButtonElement): void {
    el.focus();
  }

  function close(): void {
    store.settingsOpen = false;
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
    invoke("autostart_set", { enabled: checked }).catch((err) => {
      console.error("ubra: autostart update failed", err);
      autostart = !checked;
    });
  }

  function onThemeChange(e: Event): void {
    const value = (e.target as HTMLSelectElement).value;
    if (isThemeId(value)) store.setTheme(value);
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
      return;
    }
    soundFileValid = null;
    invoke<boolean>("check_sound_file", { path: soundFile })
      .then((ok) => {
        soundFileValid = ok;
      })
      .catch(() => {
        soundFileValid = false;
      });
  }

  function onTestSound(): void {
    invoke("play_sound", {
      kind: "done",
      file: store.soundFile.trim() === "" ? null : store.soundFile,
    }).catch((e) => console.error("ubra: test sound failed", e));
  }

  function onMuteChange(cli: string, e: Event): void {
    store.setAgentMuted(cli, (e.target as HTMLInputElement).checked);
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") close();
  }}
/>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
  class="backdrop"
  onclick={(e) => {
    if (e.target === e.currentTarget) close();
  }}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Settings">
    <div class="header">
      <span>Settings</span>
      <button use:focus onclick={close} aria-label="Close settings">
        &times;
      </button>
    </div>
    <div class="body">
      <section>
        <h2>General</h2>
        <label class="row">
          <input
            type="checkbox"
            checked={autostart}
            disabled={!autostartLoaded}
            onchange={onAutostartChange}
          />
          Launch at login
        </label>
      </section>
      <section>
        <h2>Appearance</h2>
        <label class="row">
          <span>Theme</span>
          <select
            value={store.themeId}
            onchange={onThemeChange}
            aria-label="App theme"
          >
            {#each THEME_IDS as id}
              <option value={id}>{THEMES[id].name}</option>
            {/each}
          </select>
        </label>
        <div class="row">
          <span>Terminal font size</span>
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
      </section>
      <section>
        <h2>Notifications</h2>
        <label class="row">
          <span>Agent finished</span>
          <select
            value={store.notifyDelivery}
            onchange={onDeliveryChange}
            aria-label="Notification delivery"
          >
            <option value="system">System notification</option>
            <option value="inapp">In-app toast</option>
            <option value="off">Off</option>
          </select>
        </label>
        {#if store.notifyDelivery === "inapp"}
          <label class="row">
            <span>Toast position</span>
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
          </label>
        {/if}
      </section>
      <section>
        <h2>Sounds</h2>
        <label class="row">
          <input
            type="checkbox"
            checked={store.soundEnabled}
            onchange={onSoundEnabledChange}
          />
          Play sound when an agent finishes
        </label>
        <label class="row">
          <span>Custom sound</span>
          <input
            type="text"
            bind:value={soundFile}
            onchange={onSoundFileChange}
            placeholder="Default chime"
            aria-label="Custom sound file path"
            class="file-input"
          />
        </label>
        {#if soundFile.trim() !== "" && soundFileValid === false}
          <div class="hint error-hint">
            File not found or not playable audio; default chime is used.
          </div>
        {/if}
        <div class="row">
          <span>Preview</span>
          <button class="test-button" onclick={onTestSound}>Play test sound</button>
        </div>
        {#if agent.knownClis().length > 0}
          <div class="mute-list">
            <span class="mute-heading">Mute sounds per agent</span>
            {#each agent.knownClis() as entry (entry.cli)}
              <label class="row mute-row">
                <input
                  type="checkbox"
                  checked={store.isAgentMuted(entry.cli)}
                  onchange={(e) => onMuteChange(entry.cli, e)}
                />
                {entry.label}
                {#if entry.label.toLowerCase() !== entry.cli}
                  <span class="cli">({entry.cli})</span>
                {/if}
              </label>
            {/each}
          </div>
        {/if}
      </section>
      <section>
        <h2>Status indicators</h2>
        <div class="legend">
          <div class="legend-row">
            <span class="dot working"></span>
            <span>Working — agent is running</span>
          </div>
          <div class="legend-row">
            <span class="done-check">&#10003;</span>
            <span>Done — agent finished its task</span>
          </div>
          <div class="legend-row">
            <span class="dot attention"></span>
            <span>Needs review — agent stopped unexpectedly</span>
          </div>
          <div class="legend-row">
            <span class="dot idle"></span>
            <span>Idle — no agent running, last agent remembered</span>
          </div>
        </div>
      </section>
      <section>
        <h2>Shortcuts</h2>
        <div class="shortcuts">
          {#each shortcuts as s}
            <div class="shortcut">
              <span class="keys">{s.keys}</span>
              <span>{s.blurb}</span>
            </div>
          {/each}
        </div>
      </section>
      <section>
        <h2>About</h2>
        <div class="about">
          {appName}{#if appVersion} {appVersion}{/if}
        </div>
        <div class="about-sub">Agent runtime desktop app</div>
      </section>
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
    width: 420px;
    max-width: calc(100vw - 48px);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--app-bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
    font: 12px system-ui, sans-serif;
    color: var(--text);
    overflow: hidden;
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 8px 10px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--text-strong);
    font-size: 13px;
  }
  .header button {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 16px;
    padding: 2px 10px;
    border-radius: 4px;
    cursor: pointer;
  }
  .header button:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .body {
    padding: 6px 14px 14px;
    overflow-y: auto;
  }
  section {
    margin-top: 10px;
  }
  h2 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    margin: 0 0 6px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 5px 0;
  }
  label.row {
    cursor: pointer;
  }
  label.row:has(input[type="checkbox"]) {
    justify-content: flex-start;
  }
  select {
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 4px 7px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    max-width: 200px;
  }
  .stepper {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .stepper button {
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    min-width: 26px;
    padding: 2px 8px;
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
    min-width: 38px;
    text-align: center;
    color: var(--text-strong);
  }
  .file-input {
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 4px 7px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    width: 200px;
  }
  .hint {
    font-size: 11px;
    padding: 2px 0 4px;
  }
  .error-hint {
    color: var(--error-text);
  }
  .test-button {
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    padding: 3px 10px;
    cursor: pointer;
  }
  .test-button:hover {
    background: var(--surface-bg);
    color: var(--text-strong);
  }
  .mute-list {
    display: flex;
    flex-direction: column;
    padding-top: 4px;
  }
  .mute-heading {
    font-size: 11px;
    color: var(--text-muted);
    padding: 2px 0;
  }
  .mute-row {
    padding: 3px 0;
  }
  .cli {
    color: var(--text-subtle);
  }
  .legend {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px 0;
  }
  .legend-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    flex: 0 0 auto;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .dot.working {
    background: var(--success);
  }
  .dot.attention {
    background: var(--attention);
  }
  .dot.idle {
    background: transparent;
    border: 1px solid var(--text-subtle);
    box-sizing: border-box;
  }
  .done-check {
    flex: 0 0 auto;
    width: 8px;
    text-align: center;
    color: var(--success);
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
  }
  .shortcuts {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .shortcut {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .keys {
    flex: 0 0 110px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 11px;
    color: var(--text-strong);
  }
  .about {
    color: var(--text-strong);
    font-size: 13px;
  }
  .about-sub {
    color: var(--text-subtle);
    margin-top: 2px;
  }
</style>

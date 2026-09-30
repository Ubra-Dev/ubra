<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { agent } from "./agent.svelte";
  import Icon from "./Icon.svelte";
  import {
    baseName,
    findPane,
    findTabByPane,
    paneDisplayTitle,
  } from "./layout";
  import { store } from "./store.svelte";
  import {
    deliveryLabel,
    fleetSummary,
    layoutTotalsLabel,
    platformLabel,
    saveState,
    soundLabel,
    type FleetCounts,
  } from "./statusBar";

  let appVersion = $state("");

  onMount(() => {
    invoke<{ name: string; version: string }>("app_info")
      .then((info) => {
        appVersion = info.version;
      })
      .catch((e) => console.error("ubra: app info failed", e));
  });

  const osLabel = platformLabel(navigator.platform);

  interface PaneContext {
    paneId: string;
    title: string;
    crumb: string;
    cwd: string | null;
    cwdBase: string | null;
    agent: string | null;
    cli: string | null;
    statusTitle: string;
  }

  // Fleet counts span all workspaces: background agents needing review are
  // the signal this bar adds over the visible pane headers.
  const fleet = $derived.by((): FleetCounts => {
    let working = 0;
    let blocked = 0;
    for (const group of agent.activeAgents()) {
      for (const row of group.agents) {
        if (row.status === "working") working += 1;
        else if (row.status === "blocked") blocked += 1;
      }
    }
    return { working, blocked, review: agent.attention.length + agent.done.length };
  });
  const fleetText = $derived(fleetSummary(fleet));
  const fleetDot = $derived(
    fleet.blocked > 0 ? "blocked" : fleet.review > 0 ? "attention" : "working",
  );

  // The shortcut-target pane in the active tab: focused when it is there,
  // else the tab's first pane.
  const context = $derived.by((): PaneContext | null => {
    const layout = store.layout;
    const current = store.currentPane();
    if (!layout || !current) return null;
    const found = findTabByPane(layout, current.paneId);
    const node = findPane(current.tab.root, current.paneId);
    if (!found || !node) return null;
    const state = agent.paneState(current.paneId);
    const cwd = node.cwd ?? state?.cwd ?? null;
    const title = paneDisplayTitle(node);
    return {
      paneId: current.paneId,
      title,
      crumb: `${found.ws.name} › ${current.tab.name} › ${title}`,
      cwd,
      cwdBase: cwd ? baseName(cwd) || null : null,
      agent: agent.paneAgentLabel(current.paneId) ?? null,
      cli: state?.cli ?? null,
      statusTitle: agent.paneStatusTitle(current.paneId),
    };
  });

  const save = $derived(saveState({ saving: store.saving, error: store.saveError }));
  const totals = $derived(layoutTotalsLabel(store.layout));
  const zoomedId = $derived(store.tab()?.zoomedPaneId ?? null);
  const mutedCount = $derived(store.mutedAgents.length);

  const notifyTitle = $derived(
    `Notifications: ${deliveryLabel(store.notifyDelivery)} · ` +
      `Sound ${soundLabel(store.soundEnabled)}` +
      (mutedCount > 0
        ? ` · ${mutedCount} muted agent${mutedCount === 1 ? "" : "s"}`
        : "") +
      " — open Settings",
  );

  function focusContextPane(): void {
    if (!context) return;
    store.focusPane(context.paneId);
    store.paneFocusTarget = context.paneId;
  }
</script>

<footer class="statusbar" aria-label="Status bar">
  <div class="cluster left">
    {#if fleetText}
      {#if fleet.review > 0}
        <button
          class="seg action"
          title="{fleetText} — jump to review"
          onclick={() => agent.jumpToReview()}
        >
          <span class={"dot " + fleetDot}></span>
          <span class="text">{fleetText}</span>
        </button>
      {:else}
        <span class="seg" title={fleetText}>
          <span class={"dot " + fleetDot}></span>
          <span class="text">{fleetText}</span>
        </span>
      {/if}
    {:else}
      <span class="seg dim" title="No panes are running agent CLIs">No active agents</span>
    {/if}
    {#if context}
      <button
        class="seg action crumb"
        title="Focus pane — {context.crumb}{context.cwd ? ` (${context.cwd})` : ""}"
        onclick={focusContextPane}
      >
        <span class="text">{context.crumb}</span>
      </button>
      {#if context.cwdBase && context.cwdBase !== context.title}
        <span class="seg dim" title={context.cwd}>
          <span class="text">{context.cwdBase}</span>
        </span>
      {/if}
      {#if context.agent}
        <span
          class="seg agent"
          title="{context.statusTitle} — {context.agent}{context.cli
            ? ` (${context.cli})`
            : ""}"
        >
          <span class="text">{context.agent}</span>
        </span>
      {/if}
    {/if}
  </div>
  <div class="cluster right">
    {#if zoomedId}
      <button
        class="seg action"
        title="Exit pane zoom"
        onclick={() => store.toggleZoomPane(zoomedId)}
      >
        <Icon name="minimize" size={11} />
        <span class="text">Zoomed</span>
      </button>
    {/if}
    {#if store.recoveryBackupPath}
      <span class="seg" title="Original layout preserved at: {store.recoveryBackupPath}">
        <Icon name="info" size={11} />
        <span class="text">Layout preserved</span>
      </span>
    {/if}
    {#if save === "error"}
      <span class="seg error" role="alert" title="Layout save failed: {store.saveError}">
        <Icon name="alert" size={11} />
        <span class="text">Save failed</span>
      </span>
    {:else if save === "saving"}
      <span class="seg dim" title="Saving layout…">
        <span class="text">Saving…</span>
      </span>
    {:else}
      <span class="seg dim" title="Layout saved">
        <Icon name="check" size={11} />
        <span class="text">Saved</span>
      </span>
    {/if}
    {#if totals}
      <span class="seg dim" title="Workspaces · tabs · panes">
        <span class="text">{totals}</span>
      </span>
    {/if}
    <button class="seg action" title={notifyTitle} onclick={() => (store.settingsOpen = true)}>
      <Icon name="bell" size={11} />
      <span class="text">{deliveryLabel(store.notifyDelivery)}</span>
      <span class="vol" class:off={!store.soundEnabled}>
        <Icon name="volume" size={11} />
      </span>
      <span class="text">{soundLabel(store.soundEnabled)}</span>
    </button>
    {#if appVersion}
      <span class="seg dim" title="Ubra {appVersion}{osLabel ? ` · ${osLabel}` : ""}">
        <span class="text">v{appVersion}{osLabel ? ` · ${osLabel}` : ""}</span>
      </span>
    {/if}
  </div>
</footer>

<style>
  .statusbar {
    display: flex;
    align-items: stretch;
    flex: 0 0 auto;
    min-width: 0;
    background: var(--sidebar-bg);
    border-top: 1px solid var(--border);
    font: 12px var(--font-ui);
    color: var(--text);
    user-select: none;
  }
  .cluster {
    display: flex;
    align-items: center;
    min-width: 0;
    overflow: hidden;
    padding: 3px 4px;
  }
  .cluster.left {
    flex: 1 1 auto;
  }
  .cluster.right {
    flex: 0 0 auto;
    border-left: 1px solid var(--separator);
  }
  .seg {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: 0 0 auto;
    padding: 2px 8px;
    white-space: nowrap;
  }
  button.seg {
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
    border-radius: 4px;
  }
  .seg + .seg {
    border-left: 1px solid var(--separator);
  }
  button.seg:hover {
    background: var(--surface-bg);
    color: var(--text-strong);
  }
  .seg.crumb {
    flex: 0 1 auto;
    min-width: 0;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dim {
    color: var(--text-muted);
  }
  .error {
    color: var(--error-text);
  }
  .vol {
    display: inline-flex;
  }
  .vol.off {
    opacity: 0.35;
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
  .dot.blocked {
    background: var(--error-text);
  }
  .dot.attention {
    background: var(--attention);
  }
</style>

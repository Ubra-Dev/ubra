<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";
  import { agent } from "./agent.svelte";
  import { changeCount, gitStatus } from "./git";
  import Icon from "./Icon.svelte";
  import {
    findPane,
    findTabByPane,
    paneDisplayTitle,
  } from "./layout";
  import { store } from "./store.svelte";
  import {
    branchTooltip,
    crumbDotClass,
    fleetSummary,
    notifyTooltip,
    saveState,
    soundLabel,
    updateSegment,
    usageWarning,
    type FleetCounts,
    type UpdateAction,
  } from "./statusBar";
  import { updater } from "./updater.svelte";
  import { usage } from "./usage.svelte";
  import { workspaceGit } from "./workspaceGit.svelte";

  let appVersion = $state("");

  /** Dirty-state refresh for the active workspace; matches branch polling. */
  const DIRTY_POLL_MS = 10_000;
  /** Usage refresh; matches the backend cache TTL. */
  const USAGE_POLL_MS = 5 * 60 * 1000;

  interface PaneContext {
    paneId: string;
    wsId: string;
    wsRoot: string | null;
    title: string;
    crumb: string;
    cwd: string | null;
    agent: string | null;
    cli: string | null;
    statusTitle: string;
    dot: "blocked" | "attention" | "working" | null;
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
    const agentLabel = agent.paneAgentLabel(current.paneId) ?? null;
    return {
      paneId: current.paneId,
      wsId: found.ws.id,
      wsRoot: found.ws.root ?? null,
      title: paneDisplayTitle(node),
      crumb: `${found.ws.name} › ${current.tab.name} › ${paneDisplayTitle(node)}`,
      cwd: node.cwd ?? state?.cwd ?? null,
      agent: agentLabel,
      cli: state?.cli ?? null,
      statusTitle: agent.paneStatusTitle(current.paneId),
      dot: crumbDotClass(agent.paneStatus(current.paneId), agentLabel !== null),
    };
  });

  const crumbTitle = $derived.by((): string | null => {
    if (!context) return null;
    let title = `Focus pane — ${context.crumb}`;
    if (context.cwd) title += ` (${context.cwd})`;
    if (context.agent) {
      title += ` · ${context.statusTitle} — ${context.agent}${context.cli ? ` (${context.cli})` : ""}`;
    }
    return title;
  });

  const branch = $derived(context ? workspaceGit.branchFor(context.wsId) : null);

  let dirtyCount = $state<number | null>(null);
  let dirtyRun = 0;

  async function refreshDirty(root: string | null): Promise<void> {
    const run = ++dirtyRun;
    if (!root) {
      if (run === dirtyRun) dirtyCount = null;
      return;
    }
    try {
      const status = await gitStatus(root);
      if (run === dirtyRun) dirtyCount = status.isRepo ? changeCount(status) : null;
    } catch {
      if (run === dirtyRun) dirtyCount = null;
    }
  }

  // Active workspace changed: re-resolve dirty state immediately.
  $effect(() => {
    void refreshDirty(context?.wsRoot ?? null);
  });

  const usageLabels = $derived(
    Object.fromEntries((usage.supported ?? []).map((s) => [s.cli, s.label] as const)),
  );
  const warn = $derived(usageWarning(usage.entries, usageLabels));

  const update = $derived(
    updateSegment({
      phase: updater.phase,
      checked: updater.checked,
      version: updater.version,
      downloadedBytes: updater.downloadedBytes,
      totalBytes: updater.totalBytes,
      error: updater.error,
    }),
  );

  const save = $derived(saveState({ saving: store.saving, error: store.saveError }));
  const zoomedId = $derived(store.tab()?.zoomedPaneId ?? null);
  const mutedCount = $derived(store.mutedAgents.length);

  const notifyTitle = $derived(
    notifyTooltip({
      delivery: store.notifyDelivery,
      soundEnabled: store.soundEnabled,
      mutedCount,
    }),
  );

  let dirtyTimer: ReturnType<typeof setInterval> | null = null;
  let usageTimer: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    invoke<{ name: string; version: string }>("app_info")
      .then((info) => {
        appVersion = info.version;
      })
      .catch((e) => console.error("ubra: app info failed", e));
    void usage
      .ensureSupported()
      .then((supported) => usage.refreshAll(supported.map((s) => s.cli)));
    dirtyTimer = setInterval(() => void refreshDirty(context?.wsRoot ?? null), DIRTY_POLL_MS);
    usageTimer = setInterval(() => {
      const supported = usage.supported;
      if (supported) usage.refreshAll(supported.map((s) => s.cli));
    }, USAGE_POLL_MS);
  });

  onDestroy(() => {
    if (dirtyTimer) clearInterval(dirtyTimer);
    if (usageTimer) clearInterval(usageTimer);
  });

  function focusContextPane(): void {
    if (!context) return;
    store.focusPane(context.paneId);
    store.paneFocusTarget = context.paneId;
  }

  function runUpdateAction(): void {
    const action: UpdateAction | null = update?.action ?? null;
    if (action === "check") void updater.checkForUpdates();
    else if (action === "download") void updater.downloadAndInstall();
    else if (action === "relaunch") void updater.relaunchApp();
  }

  function openUsage(): void {
    store.settingsOpenSection = "usage";
    store.settingsOpen = true;
  }

  function openSourceControl(): void {
    store.setRightPanelOpen(true);
    store.setRightPanelView("source-control");
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
        title={crumbTitle}
        onclick={focusContextPane}
      >
        {#if context.dot}
          <span class={"dot " + context.dot}></span>
        {/if}
        <span class="text">{context.crumb}</span>
      </button>
      {#if branch}
        <button
          class="seg action"
          title={branchTooltip({ branch, changes: dirtyCount })}
          onclick={openSourceControl}
        >
          <Icon name="git-branch" size={11} />
          <span class="text">{branch}</span>
          {#if dirtyCount !== null && dirtyCount > 0}
            <span class="dot attention"></span>
          {/if}
        </button>
      {/if}
    {/if}
    {#if warn}
      <button class="seg action" title={warn.title} onclick={openUsage}>
        <Icon name="alert" size={11} />
        <span class="text">{warn.text}</span>
      </button>
    {/if}
  </div>
  <div class="cluster right">
    {#if update}
      {#if update.action}
        <button
          class="seg action"
          class:error={updater.phase === "error"}
          class:dim={updater.phase === "idle"}
          title={update.title}
          onclick={runUpdateAction}
        >
          {#if updater.phase === "available" || updater.phase === "downloading"}
            <Icon name="download" size={11} />
          {:else if updater.phase === "ready"}
            <Icon name="refresh" size={11} />
          {:else if updater.phase === "error"}
            <Icon name="alert" size={11} />
          {/if}
          <span class="text">{update.text}</span>
        </button>
      {:else}
        <span class="seg dim" title={update.title}>
          <span class="text">{update.text}</span>
        </span>
      {/if}
    {/if}
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
    {/if}
    <button
      class="seg action"
      title={notifyTitle}
      aria-pressed={store.soundEnabled}
      onclick={() => store.setSoundEnabled(!store.soundEnabled)}
    >
      <Icon name="bell" size={11} />
      <span class="vol" class:off={!store.soundEnabled}>
        <Icon name="volume" size={11} />
      </span>
      <span class="text">{soundLabel(store.soundEnabled)}</span>
    </button>
    {#if appVersion}
      <span class="seg dim" title="Ubra {appVersion}">
        <span class="text">v{appVersion}</span>
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

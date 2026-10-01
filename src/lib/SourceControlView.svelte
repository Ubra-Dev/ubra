<script module lang="ts">
  // Commit drafts survive Explorer / Source Control switches per folder.
  const draftCache = new Map<string, string>();
</script>

<script lang="ts">
  import { PUBLIC_POSTHOG_HOST, PUBLIC_POSTHOG_PROJECT_TOKEN } from "$env/static/public";
  import posthog from "posthog-js";
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Spinner from "./Spinner.svelte";
  import { COPY_TOAST_DISMISS_MS } from "./clipboard";
  import {
    dirName,
    gitBranches,
    gitCommit,
    gitDiffFile,
    gitInit,
    gitPull,
    gitPush,
    gitStage,
    gitStatus,
    gitSwitch,
    gitUnstage,
    gitWorktrees,
    isClean,
    shortWorktreePath,
    statusLabel,
    worktreeLabel,
    type GitStatus,
    type GitWorktree,
  } from "./git";
  import { baseName } from "./layout";
  import { fileIconFor } from "./fileIcons";
  import { toasts } from "./toasts.svelte.ts";

  interface Props {
    root: string;
  }
  let { root }: Props = $props();

  interface FileRow {
    path: string;
    status: string;
    oldPath: string | null;
    /** true = staged, false = unstaged, null = untracked. */
    staged: boolean | null;
  }

  let status = $state<GitStatus | null>(null);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let busy = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let message = $state("");
  let diffSel = $state<{ path: string; staged: boolean | null } | null>(null);
  let diffBody = $state<string | null>(null);
  let diffLoading = $state(false);
  let diffError = $state<string | null>(null);
  let diffTruncated = $state(false);
  let diffToken = 0;
  let showBranches = $state(false);
  let branchList = $state<string[]>([]);
  let branchCurrent = $state<string | null>(null);
  let branchesLoading = $state(false);
  let branchesLoaded = false;
  let worktrees = $state<GitWorktree[]>([]);
  let extrasError = $state<string | null>(null);

  $effect(() => {
    draftCache.set(root, message);
  });

  const syncLabel = $derived(
    [
      status && status.ahead > 0 ? `↑${status.ahead}` : "",
      status && status.behind > 0 ? `↓${status.behind}` : "",
    ]
      .filter((p) => p !== "")
      .join(" "),
  );

  const commitTitle = $derived(
    (status?.staged.length ?? 0) === 0
      ? "Stage changes to commit"
      : "Commit staged changes (Ctrl+Enter)",
  );

  const groups = $derived.by(() => {
    if (!status) return [];
    const rows: { id: string; title: string; rows: FileRow[] }[] = [
      {
        id: "staged",
        title: "Staged Changes",
        rows: status.staged.map((c) => ({
          path: c.path,
          status: c.status,
          oldPath: c.oldPath,
          staged: true,
        })),
      },
      {
        id: "unstaged",
        title: "Changes",
        rows: status.unstaged.map((c) => ({
          path: c.path,
          status: c.status,
          oldPath: c.oldPath,
          staged: false,
        })),
      },
      {
        id: "untracked",
        title: "Untracked",
        rows: status.untracked.map((path) => ({
          path,
          status: "?",
          oldPath: null,
          staged: null,
        })),
      },
    ];
    return rows.filter((g) => g.rows.length > 0);
  });

  function fail(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }

  function stillPresent(
    s: GitStatus,
    sel: { path: string; staged: boolean | null },
  ): boolean {
    if (sel.staged === null) return s.untracked.includes(sel.path);
    const list = sel.staged ? s.staged : s.unstaged;
    return list.some((c) => c.path === sel.path);
  }

  async function loadStatus(): Promise<void> {
    loadError = null;
    try {
      status = await gitStatus(root);
      if (diffSel && status && !stillPresent(status, diffSel)) {
        diffSel = null;
        diffBody = null;
      }
    } catch (e) {
      loadError = fail(e);
    } finally {
      loading = false;
    }
    if (loadError || !status?.isRepo) {
      worktrees = [];
      extrasError = null;
      return;
    }
    try {
      const [branches, trees] = await Promise.all([gitBranches(root), gitWorktrees(root)]);
      branchList = branches.branches;
      branchCurrent = branches.current;
      branchesLoaded = true;
      worktrees = trees;
      extrasError = null;
    } catch (e) {
      extrasError = fail(e);
    }
  }

  async function refresh(): Promise<void> {
    if (busy) return;
    busy = "refresh";
    actionError = null;
    try {
      await loadStatus();
    } finally {
      busy = null;
    }
  }

  async function runAction(label: string, action: () => Promise<string>): Promise<void> {
    if (busy) return;
    busy = label;
    actionError = null;
    try {
      const summary = await action();
      if (PUBLIC_POSTHOG_PROJECT_TOKEN && PUBLIC_POSTHOG_HOST) {
        if (label === "init") posthog.capture("git_repository_initialized");
        else if (label === "commit") posthog.capture("git_commit_created");
        else if (label === "stage" || label === "stage-all") {
          posthog.capture("git_changes_staged", {
            scope: label === "stage-all" ? "all" : "single",
          });
        } else if (label === "unstage" || label === "unstage-all") {
          posthog.capture("git_changes_unstaged", {
            scope: label === "unstage-all" ? "all" : "single",
          });
        } else if (label === "pull") posthog.capture("git_pull_completed");
        else if (label === "push") posthog.capture("git_push_completed");
        else if (label === "switch") posthog.capture("git_branch_switched");
      }
      toasts.push(summary, "", "", {
        dismissMs: COPY_TOAST_DISMISS_MS,
        kind: "copy",
      });
      await loadStatus();
    } catch (e) {
      actionError = fail(e);
    } finally {
      busy = null;
    }
  }

  async function loadDiff(path: string, staged: boolean | null): Promise<void> {
    const token = ++diffToken;
    diffSel = { path, staged };
    diffBody = null;
    diffError = null;
    diffTruncated = false;
    if (staged === null) return;
    diffLoading = true;
    try {
      const d = await gitDiffFile(root, path, staged);
      if (token !== diffToken) return;
      diffBody = d.diff;
      diffTruncated = d.truncated;
    } catch (e) {
      if (token !== diffToken) return;
      diffError = fail(e);
    } finally {
      if (token === diffToken) diffLoading = false;
    }
  }

  function toggleDiff(path: string, staged: boolean | null): void {
    if (diffSel?.path === path && diffSel.staged === staged) {
      diffSel = null;
      diffBody = null;
    } else {
      void loadDiff(path, staged);
    }
  }

  function commit(): void {
    const text = message.trim();
    if (!text || !status || status.staged.length === 0 || busy) return;
    void runAction("commit", async () => {
      const summary = await gitCommit(root, text);
      message = "";
      return summary;
    });
  }

  async function toggleBranches(): Promise<void> {
    showBranches = !showBranches;
    if (!showBranches || branchesLoaded || branchesLoading) return;
    branchesLoading = true;
    try {
      const b = await gitBranches(root);
      branchList = b.branches;
      branchCurrent = b.current;
      branchesLoaded = true;
    } catch (e) {
      actionError = fail(e);
      showBranches = false;
    } finally {
      branchesLoading = false;
    }
  }

  function switchBranch(branch: string): void {
    showBranches = false;
    branchesLoaded = false;
    void runAction("switch", () => gitSwitch(root, branch));
  }

  function diffClass(line: string): string {
    if (line.startsWith("@@")) return "hunk";
    if (line.startsWith("+") && !line.startsWith("+++")) return "add";
    if (line.startsWith("-") && !line.startsWith("---")) return "del";
    if (
      line.startsWith("diff ") ||
      line.startsWith("index ") ||
      line.startsWith("+++") ||
      line.startsWith("---") ||
      line.startsWith("\\")
    ) {
      return "meta";
    }
    return "ctx";
  }

  function badgeClass(letter: string): string {
    switch (letter) {
      case "A":
        return "st-added";
      case "M":
        return "st-modified";
      case "D":
      case "U":
        return "st-removed";
      case "R":
      case "C":
        return "st-renamed";
      default:
        return "st-other";
    }
  }

  onMount(() => {
    message = draftCache.get(root) ?? "";
    void loadStatus();
  });
</script>

<div class="sc">
  {#if loading}
    <div class="status dim">Loading git status…</div>
  {:else if loadError}
    <div class="status error" role="alert">{loadError}</div>
    <button class="btn" onclick={() => void loadStatus()}>Retry</button>
  {:else if status && !status.isRepo}
    <div class="empty">
      <p>This folder is not a git repository.</p>
      <button
        class="btn"
        disabled={busy !== null}
        onclick={() => void runAction("init", () => gitInit(root))}
      >
        {busy === "init" ? "Initializing…" : "Initialize Repository"}
      </button>
      {#if actionError}
        <p class="status error" role="alert">{actionError}</p>
      {/if}
    </div>
  {:else if status}
    <div class="head">
      <button
        class="branch"
        title={status.upstream ? `Tracking ${status.upstream}` : "No upstream branch"}
        disabled={busy !== null || branchesLoading}
        onclick={toggleBranches}
      >
        <Icon name="git-branch" size={13} />
        <span class="branch-name">{status.branch ?? "detached"}</span>
        {#if syncLabel}
          <span class="sync">{syncLabel}</span>
        {/if}
      </button>
      <button
        class="icon-btn"
        class:busy={busy === "pull"}
        title={busy === "pull" ? "Pulling…" : "Pull (fast-forward only)"}
        aria-label={busy === "pull" ? "Pulling…" : "Pull (fast-forward only)"}
        aria-busy={busy === "pull"}
        disabled={busy !== null}
        onclick={() => void runAction("pull", () => gitPull(root))}
      >
        {#if busy === "pull"}
          <Spinner size={13} />
        {:else}
          <Icon name="download" size={13} />
        {/if}
      </button>
      <button
        class="icon-btn"
        class:busy={busy === "push"}
        title={busy === "push" ? "Pushing…" : "Push"}
        aria-label={busy === "push" ? "Pushing…" : "Push"}
        aria-busy={busy === "push"}
        disabled={busy !== null}
        onclick={() => void runAction("push", () => gitPush(root))}
      >
        {#if busy === "push"}
          <Spinner size={13} />
        {:else}
          <Icon name="upload" size={13} />
        {/if}
      </button>
      <button
        class="icon-btn"
        class:busy={busy === "refresh"}
        title={busy === "refresh" ? "Refreshing…" : "Refresh"}
        aria-label={busy === "refresh" ? "Refreshing…" : "Refresh"}
        aria-busy={busy === "refresh"}
        disabled={busy !== null}
        onclick={() => void refresh()}
      >
        {#if busy === "refresh"}
          <Spinner size={13} />
        {:else}
          <Icon name="refresh" size={13} />
        {/if}
      </button>
    </div>
    {#if showBranches}
      <div class="branches" role="listbox" aria-label="Branches">
        {#if branchesLoading}
          <div class="status dim">Loading branches…</div>
        {:else if branchList.length === 0}
          <div class="status dim">No branches yet.</div>
        {:else}
          {#each branchList as branch (branch)}
            <button
              role="option"
              aria-selected={branch === branchCurrent}
              class="branch-row"
              disabled={busy !== null || branch === branchCurrent}
              onclick={() => switchBranch(branch)}
            >
              {#if branch === branchCurrent}
                <Icon name="check" size={12} />
              {:else}
                <span class="check-sp"></span>
              {/if}
              <span class="branch-name">{branch}</span>
            </button>
          {/each}
        {/if}
      </div>
    {/if}
    {#if actionError}
      <div class="action-error" role="alert">
        <span>{actionError}</span>
        <button class="icon-btn" title="Dismiss error" aria-label="Dismiss error" onclick={() => (actionError = null)}>
          <Icon name="x" size={12} />
        </button>
      </div>
    {/if}
    <div class="commit">
      <textarea
        bind:value={message}
        rows={3}
        placeholder="Commit message"
        aria-label="Commit message"
        disabled={busy !== null}
        onkeydown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key === "Enter") commit();
        }}
      ></textarea>
      <button
        class="btn primary"
        title={commitTitle}
        disabled={busy !== null || message.trim() === "" || status.staged.length === 0}
        onclick={commit}
      >
        {busy === "commit" ? "Committing…" : "Commit"}
      </button>
    </div>
    <div class="changes">
      {#if isClean(status)}
        <div class="status dim clean">No changes.</div>
      {/if}
      {#if status.truncated}
        <div class="status dim">Status truncated — too many entries to show.</div>
      {/if}
      {#each groups as group (group.id)}
        <div class="group">
          <div class="group-head">
            <span class="group-title">{group.title} · {group.rows.length}</span>
            {#if group.id === "staged"}
              <button
                class="mini"
                title="Unstage all"
                disabled={busy !== null}
                onclick={() => void runAction("unstage-all", () => gitUnstage(root, []))}
              >
                Unstage all
              </button>
            {:else}
              <button
                class="mini"
                title="Stage all"
                disabled={busy !== null}
                onclick={() =>
                  void runAction("stage-all", () =>
                    gitStage(
                      root,
                      group.rows.map((r) => r.path),
                    ),
                  )}
              >
                Stage all
              </button>
            {/if}
          </div>
          {#each group.rows as row (row.path + "|" + group.id)}
            {@const fic = fileIconFor(baseName(row.path) || row.path, false)}
            <div class="row">
              <button
                class="main"
                class:open={diffSel?.path === row.path && diffSel.staged === row.staged}
                title={row.oldPath ? `${row.oldPath} → ${row.path}` : row.path}
                onclick={() => toggleDiff(row.path, row.staged)}
              >
                <span class={"badge " + badgeClass(row.status)} title={statusLabel(row.status)}>
                  {row.status}
                </span>
                <span class="fic" style:color={fic.color}>
                  <Icon name={fic.name} size={14} />
                </span>
                <span class="fname">{baseName(row.path) || row.path}</span>
                {#if dirName(row.path)}
                  <span class="fdir">{dirName(row.path)}</span>
                {/if}
              </button>
              {#if row.staged}
                <button
                  class="act"
                  title="Unstage"
                  aria-label={`Unstage ${row.path}`}
                  disabled={busy !== null}
                  onclick={() => void runAction("unstage", () => gitUnstage(root, [row.path]))}
                >
                  <Icon name="minus" size={12} />
                </button>
              {:else}
                <button
                  class="act"
                  title="Stage"
                  aria-label={`Stage ${row.path}`}
                  disabled={busy !== null}
                  onclick={() => void runAction("stage", () => gitStage(root, [row.path]))}
                >
                  <Icon name="plus" size={12} />
                </button>
              {/if}
            </div>
            {#if diffSel?.path === row.path && diffSel.staged === row.staged}
              <div class="diff" role="region" aria-label={`Diff for ${row.path}`}>
                {#if diffLoading}
                  <div class="status dim">Loading diff…</div>
                {:else if diffError}
                  <div class="status error">{diffError}</div>
                {:else if diffBody === null}
                  <div class="status dim">Untracked file — stage it to see a diff.</div>
                {:else if diffBody === ""}
                  <div class="status dim">No changes.</div>
                {:else}
                  <div class="code">
                    {#each diffBody.split("\n") as line, i (i)}
                      <div class={diffClass(line)}>{line || " "}</div>
                    {/each}
                  </div>
                  {#if diffTruncated}
                    <div class="status dim">Diff truncated.</div>
                  {/if}
                {/if}
              </div>
            {/if}
          {/each}
        </div>
      {/each}
      {#if extrasError}
        <div class="status error">{extrasError}</div>
      {:else}
        <div class="group">
          <div class="group-head">
            <span class="group-title">Worktrees · {worktrees.length}</span>
          </div>
          {#if worktrees.length === 0}
            <div class="status dim">No worktrees.</div>
          {:else}
            {#each worktrees as w (w.path)}
              <div class="wt-row" title={w.path}>
                <span class="wt-icon">
                  <Icon name="git-branch" size={12} />
                </span>
                <span class="wt-text">
                  <span class="fname">{worktreeLabel(w)}</span>
                  <span class="fdir">{shortWorktreePath(w.path)}</span>
                </span>
                {#if w.locked !== null}
                  <span class="flag" title={w.locked === "" ? "Locked" : `Locked: ${w.locked}`}>
                    locked
                  </span>
                {/if}
                {#if w.prunable !== null}
                  <span
                    class="flag warn"
                    title={w.prunable === "" ? "Prunable" : `Prunable: ${w.prunable}`}
                  >
                    prunable
                  </span>
                {/if}
              </div>
            {/each}
          {/if}
        </div>
        <div class="group">
          <div class="group-head">
            <span class="group-title">Branches · {branchList.length}</span>
          </div>
          {#if branchList.length === 0}
            <div class="status dim">No branches yet.</div>
          {:else}
            {#each branchList as branch (branch)}
              <div class="wt-row" class:current={branch === branchCurrent}>
                {#if branch === branchCurrent}
                  <Icon name="check" size={12} />
                {:else}
                  <span class="check-sp"></span>
                {/if}
                <span class="fname">{branch}</span>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .sc {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1 1 auto;
    font: 12px system-ui, sans-serif;
    color: var(--text);
  }
  .head {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 4px;
    padding: 6px;
    border-bottom: 1px solid var(--border);
  }
  .branch {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: 1 1 auto;
    min-width: 0;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-strong);
    font: inherit;
    font-weight: 600;
    padding: 5px 8px;
    cursor: pointer;
  }
  .branch:hover:not(:disabled) {
    background: var(--surface-bg);
  }
  .branch:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .branch-name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  .sync {
    flex: 0 0 auto;
    color: var(--text-muted);
    font-weight: 400;
    font-size: 11px;
  }
  .mini {
    flex: 0 0 auto;
    background: var(--surface-bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 5px;
    font: inherit;
    font-size: 11px;
    padding: 3px 8px;
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--text-strong);
  }
  .mini:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 26px;
    height: 26px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .icon-btn:hover:not(:disabled) {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .icon-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  /* The running action keeps full opacity while its spinner runs. */
  .icon-btn.busy {
    opacity: 1;
  }
  .branches {
    flex: 0 0 auto;
    max-height: 150px;
    overflow-y: auto;
    border-bottom: 1px solid var(--border);
    padding: 4px;
  }
  .branch-row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    box-sizing: border-box;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--text);
    font: inherit;
    text-align: left;
    padding: 4px 8px;
    cursor: pointer;
  }
  .branch-row:hover:not(:disabled) {
    background: var(--surface-bg);
  }
  .branch-row:disabled {
    cursor: default;
  }
  .branch-row[aria-selected="true"] {
    color: var(--text-strong);
  }
  .check-sp {
    flex: 0 0 auto;
    width: 12px;
  }
  .action-error {
    display: flex;
    flex: 0 0 auto;
    align-items: flex-start;
    gap: 6px;
    margin: 6px 6px 0;
    padding: 6px 4px 6px 10px;
    background: var(--error-bg);
    color: var(--error-text);
    border-radius: 6px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .action-error span {
    flex: 1 1 auto;
    min-width: 0;
  }
  .commit {
    display: flex;
    flex: 0 0 auto;
    flex-direction: column;
    gap: 6px;
    padding: 8px 6px;
    border-bottom: 1px solid var(--border);
  }
  .commit textarea {
    resize: vertical;
    min-height: 44px;
    max-height: 140px;
    box-sizing: border-box;
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 6px;
    color: var(--text-strong);
    font: inherit;
    line-height: 1.4;
    padding: 6px 8px;
  }
  .commit textarea:focus {
    border-color: var(--accent);
    outline: none;
  }
  .btn {
    background: var(--surface-bg);
    color: var(--text-strong);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 12px;
    font: inherit;
    cursor: pointer;
  }
  .btn:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .btn.primary:disabled {
    cursor: not-allowed;
  }
  .changes {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: thin;
    padding-bottom: 8px;
  }
  .group {
    padding-top: 6px;
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 6px 4px 10px;
  }
  .group-title {
    flex: 1 1 auto;
    min-width: 0;
    color: var(--text-muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 4px;
  }
  .main {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1 1 auto;
    min-width: 0;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--text);
    font: inherit;
    text-align: left;
    padding: 3px 4px 3px 10px;
    cursor: pointer;
    white-space: nowrap;
  }
  .main:hover {
    background: var(--surface-bg);
  }
  .main.open {
    background: var(--surface-active);
  }
  .badge {
    flex: 0 0 auto;
    width: 14px;
    text-align: center;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 11px;
    font-weight: 700;
  }
  .st-added {
    color: var(--success);
  }
  .st-modified {
    color: var(--attention);
  }
  .st-removed {
    color: var(--danger, #f87171);
  }
  .st-renamed {
    color: var(--accent);
  }
  .st-other {
    color: var(--text-subtle);
  }
  .fic {
    display: inline-flex;
    flex: 0 0 auto;
  }
  .fname {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fdir {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-subtle);
    font-size: 11px;
  }
  .act {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 22px;
    height: 22px;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0;
  }
  .row:hover .act,
  .row:focus-within .act {
    opacity: 1;
  }
  .act:hover:not(:disabled) {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .act:disabled {
    cursor: default;
  }
  .diff {
    margin: 2px 6px 6px 10px;
    max-height: 260px;
    overflow: auto;
    background: var(--input-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font: 11px ui-monospace, Menlo, Consolas, monospace;
  }
  .code {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .code .add {
    color: var(--success);
  }
  .code .del {
    color: var(--danger, #f87171);
  }
  .code .hunk {
    color: var(--accent);
  }
  .code .meta {
    color: var(--text-subtle);
  }
  .code .ctx {
    color: var(--text-muted);
  }
  .status {
    padding: 6px 10px;
    color: var(--text-muted);
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .status.dim {
    opacity: 0.8;
  }
  .status.clean {
    padding-top: 10px;
  }
  .status.error {
    color: var(--error-text, #f87171);
  }
  .empty {
    padding: 18px 14px;
    color: var(--text-muted);
    line-height: 1.5;
  }
  .empty p {
    margin: 0 0 12px;
  }
  .wt-row {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 3px 6px 3px 10px;
    border-radius: 4px;
    white-space: nowrap;
  }
  .wt-icon {
    display: inline-flex;
    flex: 0 0 auto;
    margin-top: 2px;
  }
  .wt-text {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-width: 0;
    gap: 1px;
  }
  .wt-row.current {
    color: var(--text-strong);
  }
  .flag {
    flex: 0 0 auto;
    font-size: 10px;
    color: var(--text-muted);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 5px;
    line-height: 1.6;
  }
  .flag.warn {
    color: var(--attention);
    border-color: var(--attention);
  }
</style>

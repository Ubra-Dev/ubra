import { invoke } from "@tauri-apps/api/core";

export interface ChangeEntry {
  path: string;
  status: string;
  oldPath: string | null;
}

export interface GitStatus {
  isRepo: boolean;
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  staged: ChangeEntry[];
  unstaged: ChangeEntry[];
  untracked: string[];
  truncated: boolean;
}

export interface GitDiff {
  diff: string;
  truncated: boolean;
}

export interface GitBranches {
  current: string | null;
  branches: string[];
}

export interface GitWorktree {
  path: string;
  head: string | null;
  branch: string | null;
  detached: boolean;
  bare: boolean;
  locked: string | null;
  prunable: string | null;
}

export function gitStatus(root: string): Promise<GitStatus> {
  return invoke<GitStatus>("git_status", { root });
}

export function gitDiffFile(root: string, path: string, staged: boolean): Promise<GitDiff> {
  return invoke<GitDiff>("git_diff_file", { root, path, staged });
}

/** Stage paths; an empty list stages everything (`git add -A`). */
export function gitStage(root: string, paths: string[]): Promise<string> {
  return invoke<string>("git_stage", { root, paths });
}

/** Unstage paths; an empty list unstages everything. */
export function gitUnstage(root: string, paths: string[]): Promise<string> {
  return invoke<string>("git_unstage", { root, paths });
}

export function gitCommit(root: string, message: string): Promise<string> {
  return invoke<string>("git_commit", { root, message });
}

export function gitPush(root: string): Promise<string> {
  return invoke<string>("git_push", { root });
}

export function gitPull(root: string): Promise<string> {
  return invoke<string>("git_pull", { root });
}

export function gitBranches(root: string): Promise<GitBranches> {
  return invoke<GitBranches>("git_branches", { root });
}

export function gitWorktrees(root: string): Promise<GitWorktree[]> {
  return invoke<GitWorktree[]>("git_worktrees", { root });
}

/** Short human label: branch name, "(detached HEAD)", or "(bare)". */
export function worktreeLabel(w: GitWorktree): string {
  if (w.branch) return w.branch;
  if (w.bare) return "(bare)";
  return "(detached HEAD)";
}

/**
 * Display path for a worktree row: the last two segments with a "…/" prefix
 * when longer, so panel-width ellipsis never eats the distinctive tail.
 * Short paths pass through unchanged; the full path stays in the tooltip.
 */
export function shortWorktreePath(path: string): string {
  const trimmed = path.replace(/[/\\]+$/, "");
  const parts = trimmed.split(/[/\\]+/).filter((part) => part !== "");
  if (parts.length <= 2) return trimmed || path;
  return `…/${parts[parts.length - 2]}/${parts[parts.length - 1]}`;
}

export function gitSwitch(root: string, branch: string): Promise<string> {
  return invoke<string>("git_switch", { root, branch });
}

export function gitInit(root: string): Promise<string> {
  return invoke<string>("git_init", { root });
}

/** Total changed paths across staged, unstaged, and untracked. */
export function changeCount(status: GitStatus): number {
  return status.staged.length + status.unstaged.length + status.untracked.length;
}

/** True when the repo has no staged, unstaged, or untracked changes. */
export function isClean(status: GitStatus): boolean {
  return changeCount(status) === 0;
}

/** Compact per-workspace git state for sidebar subtitles. */
export interface GitSummary {
  changed: number;
  staged: number;
  unstaged: number;
  untracked: number;
  ahead: number;
  behind: number;
  /** True when the backend capped the lists; counts are lower bounds. */
  truncated: boolean;
}

/** Reduce a full status to sidebar counts. */
export function summarizeStatus(status: GitStatus): GitSummary {
  return {
    changed: changeCount(status),
    staged: status.staged.length,
    unstaged: status.unstaged.length,
    untracked: status.untracked.length,
    ahead: status.ahead,
    behind: status.behind,
    truncated: status.truncated,
  };
}

/** Compact ahead/behind label for sidebar rows, e.g. "↑2 ↓1". Empty when synced. */
export function gitSyncLabel(summary: GitSummary): string {
  const parts: string[] = [];
  if (summary.ahead > 0) parts.push(`↑${summary.ahead}`);
  if (summary.behind > 0) parts.push(`↓${summary.behind}`);
  return parts.join(" ");
}

/** Tooltip for the sidebar summary: breakdown plus ahead/behind. */
export function gitSummaryTitle(summary: GitSummary): string {
  const parts: string[] = [];
  if (summary.changed === 0) {
    parts.push("Clean");
  } else {
    const bits: string[] = [];
    if (summary.staged > 0) bits.push(`${summary.staged} staged`);
    if (summary.unstaged > 0) bits.push(`${summary.unstaged} unstaged`);
    if (summary.untracked > 0) bits.push(`${summary.untracked} untracked`);
    const plus = summary.truncated ? "+" : "";
    const files = summary.changed === 1 ? "file" : "files";
    parts.push(`${summary.changed}${plus} changed ${files} (${bits.join(" · ")})`);
  }
  if (summary.ahead > 0) parts.push(`↑${summary.ahead} ahead`);
  if (summary.behind > 0) parts.push(`↓${summary.behind} behind`);
  return parts.join(" · ");
}

/** Human label for a porcelain status letter. */
export function statusLabel(status: string): string {
  switch (status) {
    case "M":
      return "Modified";
    case "A":
      return "Added";
    case "D":
      return "Deleted";
    case "R":
      return "Renamed";
    case "C":
      return "Copied";
    case "U":
      return "Unmerged";
    case "T":
      return "Type changed";
    case "?":
      return "Untracked";
    default:
      return status;
  }
}

/** "src/lib/x.ts" gives "src/lib"; a bare name gives "". */
export function dirName(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
}

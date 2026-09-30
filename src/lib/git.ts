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

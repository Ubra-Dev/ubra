import { invoke } from "@tauri-apps/api/core";

export interface DirEntry {
  name: string;
  isDir: boolean;
  isSymlink: boolean;
  size: number;
  modifiedMs: number | null;
}

export interface DirListing {
  entries: DirEntry[];
  truncated: boolean;
}

/** One-level listing; `path` is "" for the root, "/"-separated below it. */
export function listDir(root: string, path: string): Promise<DirListing> {
  return invoke<DirListing>("fs_list_dir", { root, path });
}

export interface FileContent {
  content: string;
  truncated: boolean;
  binary: boolean;
  size: number;
}

/** File bytes for the preview dialog; refuses directories and escapes. */
export function readFile(root: string, path: string): Promise<FileContent> {
  return invoke<FileContent>("fs_read_file", { root, path });
}

/** Child relative path: parent "" gives just the name. */
export function childRel(parent: string, name: string): string {
  return parent === "" ? name : `${parent}/${name}`;
}

/** Dotfiles and dot-directories, hidden unless the viewer opts in. */
export function isHiddenName(name: string): boolean {
  return name.startsWith(".");
}

/** "1536" -> "1.5 KiB"; binary units like the backend caps. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const text = unit === 0 || Number.isInteger(value) ? value.toFixed(0) : value.toFixed(1);
  return `${text} ${units[unit]}`;
}

/**
 * Absolute filesystem path for a tree row. Listings use "/" separators on
 * every platform, so translate them to the root's own separator style.
 */
export function joinFsPath(root: string, rel: string): string {
  if (rel === "") return root;
  const sep = root.includes("\\") ? "\\" : "/";
  const base =
    root.endsWith("/") || root.endsWith("\\") ? root.slice(0, -1) : root;
  return `${base}${sep}${rel.replace(/\//g, sep)}`;
}

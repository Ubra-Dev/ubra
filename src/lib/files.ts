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

/** Child relative path: parent "" gives just the name. */
export function childRel(parent: string, name: string): string {
  return parent === "" ? name : `${parent}/${name}`;
}

/** Dotfiles and dot-directories, hidden unless the viewer opts in. */
export function isHiddenName(name: string): boolean {
  return name.startsWith(".");
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

import { invoke } from "@tauri-apps/api/core";

export type NoteScope = "global" | "workspace";

export interface NoteEntry {
  name: string;
  title: string;
  updatedMs: number | null;
  size: number;
}

export interface NoteListing {
  entries: NoteEntry[];
  truncated: boolean;
}

export interface NoteContent {
  content: string;
  truncated: boolean;
  size: number;
}

export interface NoteHit {
  scope: string;
  name: string;
  title: string;
  snippet: string;
  updatedMs: number | null;
}

export interface NoteSearchResults {
  hits: NoteHit[];
  truncated: boolean;
}

function scopeArgs(scope: NoteScope, workspaceId: string | null) {
  return { scope, workspaceId: scope === "workspace" ? workspaceId : null };
}

/** Newest-first listing for one scope. */
export function listNotes(scope: NoteScope, workspaceId: string | null): Promise<NoteListing> {
  return invoke<NoteListing>("notes_list", scopeArgs(scope, workspaceId));
}

/** Full note text; refuses oversize/binary notes. */
export function readNote(
  scope: NoteScope,
  workspaceId: string | null,
  name: string,
): Promise<NoteContent> {
  return invoke<NoteContent>("notes_read", { ...scopeArgs(scope, workspaceId), name });
}

/** Create or overwrite a note atomically. */
export function writeNote(
  scope: NoteScope,
  workspaceId: string | null,
  name: string,
  content: string,
): Promise<NoteEntry> {
  return invoke<NoteEntry>("notes_write", { ...scopeArgs(scope, workspaceId), name, content });
}

/** Rename a note within its scope. */
export function renameNote(
  scope: NoteScope,
  workspaceId: string | null,
  oldName: string,
  newName: string,
): Promise<NoteEntry> {
  return invoke<NoteEntry>("notes_rename", {
    ...scopeArgs(scope, workspaceId),
    oldName,
    newName,
  });
}

/** Delete a note; missing notes succeed silently. */
export function deleteNote(
  scope: NoteScope,
  workspaceId: string | null,
  name: string,
): Promise<void> {
  return invoke<void>("notes_delete", { ...scopeArgs(scope, workspaceId), name });
}

/** Substring search across global notes plus one workspace scope. */
export function searchNotes(query: string, workspaceId: string | null): Promise<NoteSearchResults> {
  return invoke<NoteSearchResults>("notes_search", { query, workspaceId });
}

/**
 * "My Great Idea!" -> "my-great-idea". Output always satisfies the
 * backend name rules (lowercase alnum, dash, underscore, dot, space).
 */
export function slugify(title: string): string {
  const slug = title
    .toLowerCase()
    .replace(/[^a-z0-9 _.-]+/g, "-")
    .replace(/[\s_.-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
  return slug === "" ? "untitled" : slug;
}

/**
 * Unique file name for a new note: slug + random suffix, regenerated
 * while `taken` reports a collision.
 */
export function newNoteName(title: string, taken: (name: string) => boolean): string {
  const slug = slugify(title);
  for (let attempt = 0; attempt < 10; attempt++) {
    const suffix = Math.floor(Math.random() * 0xffffff)
      .toString(16)
      .padStart(6, "0");
    const name = `${slug}-${suffix}`;
    if (!taken(name)) return name;
  }
  return `${slug}-${Date.now().toString(36)}`;
}

/**
 * Client-side title mirror of the backend rule: first `# ` heading, else
 * the first non-empty line, else the file name. Used for optimistic
 * list updates before the backend round-trips.
 */
export function titleFromContent(content: string, fallback: string): string {
  for (const raw of content.split("\n")) {
    const line = raw.trim();
    if (line.startsWith("# ")) {
      const heading = line.slice(2).trim();
      if (heading !== "") return heading.slice(0, 120);
    }
  }
  for (const raw of content.split("\n")) {
    const line = raw.trim().replace(/^#+/, "").trim();
    if (line !== "") return line.slice(0, 120);
  }
  return fallback;
}

/** "just now" / "5m ago" / "Yesterday" / "12 Mar" / "12 Mar 2024". */
export function formatNoteDate(updatedMs: number | null, nowMs = Date.now()): string {
  if (updatedMs === null) return "";
  const diff = nowMs - updatedMs;
  if (diff < 0) return "just now";
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diff < minute) return "just now";
  if (diff < hour) return `${Math.floor(diff / minute)}m ago`;
  if (diff < day) return `${Math.floor(diff / hour)}h ago`;
  const date = new Date(updatedMs);
  const now = new Date(nowMs);
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) return "Yesterday";
  const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const dayMonth = `${date.getDate()} ${months[date.getMonth()]}`;
  return date.getFullYear() === now.getFullYear() ? dayMonth : `${dayMonth} ${date.getFullYear()}`;
}

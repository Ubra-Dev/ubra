// App keyboard shortcuts: pure module (no Svelte/Tauri imports) so the
// binding table, matching, and labels can be unit-tested with node:test.
// The window-level dispatcher (+page.svelte) and the settings cheat sheet
// both derive from SHORTCUTS: single source of truth.
import type { Direction } from "./layout";

export type ShortcutAction =
  | "new-tab"
  | "close-pane-or-tab"
  | "new-workspace"
  | "split-right"
  | "split-down"
  | "toggle-zoom"
  | "prev-tab"
  | "next-tab"
  | "jump-tab"
  | "prev-workspace"
  | "next-workspace"
  | "font-bigger"
  | "font-smaller"
  | "font-reset"
  | "open-settings"
  | "rename-pane"
  | "find-in-pane"
  | "focus-neighbor"
  | "swap-neighbor"
  | "resize-pane"
  | "move-pane-to-new-tab"
  | "move-pane-to-new-workspace";

export interface ShortcutBinding {
  action: ShortcutAction;
  /** e.key to match (lowercase). */
  key: string;
  /** Mod = Cmd on macOS, Ctrl elsewhere. */
  mod: boolean;
  shift: boolean;
  alt: boolean;
  /** Direction for neighbor/resize actions. */
  dir?: Direction;
  /** Key cap for display, e.g. "T", "+", "Enter". */
  cap: string;
  /** Cheat-sheet description. */
  blurb: string;
}

export const SHORTCUTS: ShortcutBinding[] = [
  { action: "new-tab", key: "t", mod: true, shift: false, alt: false, cap: "T", blurb: "New tab" },
  { action: "close-pane-or-tab", key: "w", mod: true, shift: false, alt: false, cap: "W", blurb: "Close pane (last pane closes the tab)" },
  { action: "new-workspace", key: "n", mod: true, shift: false, alt: false, cap: "N", blurb: "New workspace" },
  { action: "split-right", key: "d", mod: true, shift: false, alt: false, cap: "D", blurb: "Split pane right" },
  { action: "split-down", key: "d", mod: true, shift: true, alt: false, cap: "D", blurb: "Split pane down" },
  { action: "toggle-zoom", key: "enter", mod: true, shift: false, alt: false, cap: "Enter", blurb: "Zoom / unzoom pane" },
  { action: "prev-tab", key: "[", mod: true, shift: false, alt: false, cap: "[", blurb: "Previous tab" },
  { action: "next-tab", key: "]", mod: true, shift: false, alt: false, cap: "]", blurb: "Next tab" },
  { action: "prev-workspace", key: "[", mod: true, shift: true, alt: false, cap: "[", blurb: "Previous workspace" },
  { action: "next-workspace", key: "]", mod: true, shift: true, alt: false, cap: "]", blurb: "Next workspace" },
  { action: "font-bigger", key: "=", mod: true, shift: false, alt: false, cap: "+", blurb: "Bigger terminal font" },
  { action: "font-smaller", key: "-", mod: true, shift: false, alt: false, cap: "-", blurb: "Smaller terminal font" },
  { action: "font-reset", key: "0", mod: true, shift: false, alt: false, cap: "0", blurb: "Reset terminal font" },
  { action: "open-settings", key: ",", mod: true, shift: false, alt: false, cap: ",", blurb: "Open settings" },
  { action: "rename-pane", key: "f2", mod: false, shift: false, alt: false, cap: "F2", blurb: "Rename focused pane" },
  { action: "find-in-pane", key: "f", mod: true, shift: false, alt: false, cap: "F", blurb: "Find in focused pane" },
  { action: "focus-neighbor", key: "arrowleft", mod: true, shift: false, alt: true, dir: "left", cap: "←", blurb: "Focus pane left" },
  { action: "focus-neighbor", key: "arrowright", mod: true, shift: false, alt: true, dir: "right", cap: "→", blurb: "Focus pane right" },
  { action: "focus-neighbor", key: "arrowup", mod: true, shift: false, alt: true, dir: "up", cap: "↑", blurb: "Focus pane up" },
  { action: "focus-neighbor", key: "arrowdown", mod: true, shift: false, alt: true, dir: "down", cap: "↓", blurb: "Focus pane down" },
  { action: "swap-neighbor", key: "arrowleft", mod: true, shift: true, alt: true, dir: "left", cap: "←", blurb: "Swap pane left" },
  { action: "swap-neighbor", key: "arrowright", mod: true, shift: true, alt: true, dir: "right", cap: "→", blurb: "Swap pane right" },
  { action: "swap-neighbor", key: "arrowup", mod: true, shift: true, alt: true, dir: "up", cap: "↑", blurb: "Swap pane up" },
  { action: "swap-neighbor", key: "arrowdown", mod: true, shift: true, alt: true, dir: "down", cap: "↓", blurb: "Swap pane down" },
  { action: "resize-pane", key: "arrowleft", mod: false, shift: true, alt: true, dir: "left", cap: "←", blurb: "Grow pane left" },
  { action: "resize-pane", key: "arrowright", mod: false, shift: true, alt: true, dir: "right", cap: "→", blurb: "Grow pane right" },
  { action: "resize-pane", key: "arrowup", mod: false, shift: true, alt: true, dir: "up", cap: "↑", blurb: "Grow pane up" },
  { action: "resize-pane", key: "arrowdown", mod: false, shift: true, alt: true, dir: "down", cap: "↓", blurb: "Grow pane down" },
  { action: "move-pane-to-new-tab", key: "t", mod: true, shift: false, alt: true, cap: "T", blurb: "Move pane to new tab" },
  { action: "move-pane-to-new-workspace", key: "n", mod: true, shift: false, alt: true, cap: "N", blurb: "Move pane to new workspace" },
];

/** Minimal key-event shape for matching (subset of KeyboardEvent). */
export interface KeyShape {
  key: string;
  mod: boolean;
  shift: boolean;
  alt: boolean;
}

export interface ShortcutMatch {
  action: ShortcutAction;
  /** 0-based tab index for jump-tab. */
  index?: number;
  /** Direction for neighbor/resize actions. */
  dir?: Direction;
}

/**
 * Match a key press against the table. All modifiers must match exactly.
 * Mod+1..8 (no Alt) jumps to tabs by number. "+" normalizes to "=" so both
 * bare and shifted equals zoom the font.
 */
export function matchShortcut(shape: KeyShape): ShortcutMatch | null {
  const key = shape.key.toLowerCase();
  if (shape.mod && !shape.shift && !shape.alt && /^[1-8]$/.test(key)) {
    return { action: "jump-tab", index: Number(key) - 1 };
  }
  const norm = key === "+" ? "=" : key;
  const found = SHORTCUTS.find(
    (b) =>
      b.key === norm &&
      b.mod === shape.mod &&
      b.shift === shape.shift &&
      b.alt === shape.alt,
  );
  if (!found) return null;
  const out: ShortcutMatch = { action: found.action };
  if (found.dir) out.dir = found.dir;
  return out;
}

/** True for text inputs, textareas, and selects that should swallow keys. */
export function isEditableTarget(
  el: { tagName?: string; isContentEditable?: boolean } | null,
): boolean {
  if (!el) return false;
  const tag = (el.tagName ?? "").toUpperCase();
  return (
    tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable === true
  );
}

export function isMacPlatform(platform: string): boolean {
  return platform.toLowerCase().includes("mac");
}

export function modLabel(isMac: boolean): string {
  return isMac ? "⌘" : "Ctrl";
}

/** "⌘⇧D" on Mac, "Ctrl+Shift+D" elsewhere. */
export function formatBinding(
  b: Pick<ShortcutBinding, "mod" | "shift" | "alt" | "cap">,
  isMac: boolean,
): string {
  if (isMac)
    return `${b.mod ? "⌘" : ""}${b.alt ? "⌥" : ""}${b.shift ? "⇧" : ""}${b.cap}`;
  const parts: string[] = [];
  if (b.mod) parts.push("Ctrl");
  if (b.alt) parts.push("Alt");
  if (b.shift) parts.push("Shift");
  parts.push(b.cap);
  return parts.join("+");
}

export interface CheatRow {
  keys: string;
  blurb: string;
}

/** Ordered cheat-sheet rows: the table plus the Mod+1..8 jump row. */
export function cheatSheet(isMac: boolean): CheatRow[] {
  const rows: CheatRow[] = [];
  for (const b of SHORTCUTS) {
    rows.push({ keys: formatBinding(b, isMac), blurb: b.blurb });
    if (b.action === "next-tab") {
      rows.push({
        keys: isMac ? "⌘1–8" : "Ctrl+1–8",
        blurb: "Jump to tab by number",
      });
    }
  }
  return rows;
}

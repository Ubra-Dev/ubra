// App keyboard shortcuts: pure module (no Svelte/Tauri imports) so the
// binding table, matching, and labels can be unit-tested with node:test.
// The window-level dispatcher (+page.svelte) and the settings cheat sheet
// both derive from SHORTCUTS: single source of truth.

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
  | "rename-pane";

export interface ShortcutBinding {
  action: ShortcutAction;
  /** e.key to match (lowercase). */
  key: string;
  /** Mod = Cmd on macOS, Ctrl elsewhere. */
  mod: boolean;
  shift: boolean;
  /** Key cap for display, e.g. "T", "+", "Enter". */
  cap: string;
  /** Cheat-sheet description. */
  blurb: string;
}

export const SHORTCUTS: ShortcutBinding[] = [
  { action: "new-tab", key: "t", mod: true, shift: false, cap: "T", blurb: "New tab" },
  { action: "close-pane-or-tab", key: "w", mod: true, shift: false, cap: "W", blurb: "Close pane (last pane closes the tab)" },
  { action: "new-workspace", key: "n", mod: true, shift: false, cap: "N", blurb: "New workspace" },
  { action: "split-right", key: "d", mod: true, shift: false, cap: "D", blurb: "Split pane right" },
  { action: "split-down", key: "d", mod: true, shift: true, cap: "D", blurb: "Split pane down" },
  { action: "toggle-zoom", key: "enter", mod: true, shift: false, cap: "Enter", blurb: "Zoom / unzoom pane" },
  { action: "prev-tab", key: "[", mod: true, shift: false, cap: "[", blurb: "Previous tab" },
  { action: "next-tab", key: "]", mod: true, shift: false, cap: "]", blurb: "Next tab" },
  { action: "prev-workspace", key: "[", mod: true, shift: true, cap: "[", blurb: "Previous workspace" },
  { action: "next-workspace", key: "]", mod: true, shift: true, cap: "]", blurb: "Next workspace" },
  { action: "font-bigger", key: "=", mod: true, shift: false, cap: "+", blurb: "Bigger terminal font" },
  { action: "font-smaller", key: "-", mod: true, shift: false, cap: "-", blurb: "Smaller terminal font" },
  { action: "font-reset", key: "0", mod: true, shift: false, cap: "0", blurb: "Reset terminal font" },
  { action: "open-settings", key: ",", mod: true, shift: false, cap: ",", blurb: "Open settings" },
  { action: "rename-pane", key: "f2", mod: false, shift: false, cap: "F2", blurb: "Rename focused pane" },
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
}

/**
 * Match a key press against the table. Alt always disqualifies (no binding
 * uses it). Mod+1..8 jumps to tabs by number. "+" normalizes to "=" so both
 * bare and shifted equals zoom the font.
 */
export function matchShortcut(shape: KeyShape): ShortcutMatch | null {
  if (shape.alt) return null;
  const key = shape.key.toLowerCase();
  if (shape.mod && !shape.shift && /^[1-8]$/.test(key)) {
    return { action: "jump-tab", index: Number(key) - 1 };
  }
  const norm = key === "+" ? "=" : key;
  const found = SHORTCUTS.find(
    (b) => b.key === norm && b.mod === shape.mod && b.shift === shape.shift,
  );
  return found ? { action: found.action } : null;
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
  b: Pick<ShortcutBinding, "mod" | "shift" | "cap">,
  isMac: boolean,
): string {
  if (isMac) return `${b.mod ? "⌘" : ""}${b.shift ? "⇧" : ""}${b.cap}`;
  const parts: string[] = [];
  if (b.mod) parts.push("Ctrl");
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

/**
 * Clipboard helpers for terminal select-to-copy.
 *
 * The web Clipboard API is primary (works in the Tauri webview); a hidden
 * textarea + execCommand fallback covers webviews where the async API is
 * unavailable or denied.
 */

/** Max chars of the selection shown in the copy toast body. */
export const COPY_TOAST_PREVIEW_LEN = 80;

/** How long the "Copied to clipboard" toast stays up. */
export const COPY_TOAST_DISMISS_MS = 2000;

/** Debounce for xterm selection-change events (fires continuously mid-drag). */
export const SELECTION_DEBOUNCE_MS = 400;

/**
 * Whether a fresh selection should trigger an auto-copy: non-blank and
 * different from the last copied text (prevents toast spam while dragging).
 */
export function shouldAutoCopy(selection: string, lastCopied: string): boolean {
  if (selection.length === 0 || selection === lastCopied) return false;
  return selection.trim().length > 0;
}

/** Single-line preview of the copied text for the toast body. */
export function truncatePreview(text: string, maxLen = COPY_TOAST_PREVIEW_LEN): string {
  const flat = text.replace(/\s+/g, " ").trim();
  if (flat.length <= maxLen) return flat;
  return `${flat.slice(0, maxLen - 1).trimEnd()}…`;
}

/** Write text to the system clipboard. Resolves false when unavailable. */
export async function copyTextToClipboard(text: string): Promise<boolean> {
  if (text.length === 0) return false;
  try {
    if (
      typeof navigator !== "undefined" &&
      navigator.clipboard?.writeText !== undefined
    ) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // Fall through to the execCommand fallback below.
  }
  return legacyCopy(text);
}

function legacyCopy(text: string): boolean {
  try {
    if (typeof document === "undefined") return false;
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(area);
    return ok;
  } catch {
    return false;
  }
}

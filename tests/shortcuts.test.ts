import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  SHORTCUTS,
  cheatSheet,
  formatBinding,
  isEditableTarget,
  isMacPlatform,
  matchShortcut,
  matchShortcutEvent,
  modLabel,
} from "../src/lib/shortcuts.ts";

function shape(
  key: string,
  opts: { mod?: boolean; shift?: boolean; alt?: boolean } = {},
): { key: string; mod: boolean; shift: boolean; alt: boolean } {
  return {
    key,
    mod: opts.mod ?? false,
    shift: opts.shift ?? false,
    alt: opts.alt ?? false,
  };
}

describe("matchShortcut", () => {

  it("carries direction for neighbor actions", () => {
    assert.deepEqual(
      matchShortcut(shape("ArrowRight", { mod: true, alt: true })),
      { action: "focus-neighbor", dir: "right" },
    );
    assert.deepEqual(
      matchShortcut(shape("ArrowUp", { mod: true, shift: true, alt: true })),
      { action: "swap-neighbor", dir: "up" },
    );
    assert.deepEqual(
      matchShortcut(shape("ArrowLeft", { shift: true, alt: true })),
      { action: "resize-pane", dir: "left" },
    );
    assert.deepEqual(matchShortcut(shape("t", { mod: true, alt: true })), {
      action: "move-pane-to-new-tab",
    });
    // Alt must match exactly: no binding, no match.
    assert.equal(matchShortcut(shape("ArrowRight", { mod: true })), null);
    assert.equal(matchShortcut(shape("ArrowRight", { alt: true })), null);
  });

  it("matches uppercase keys (shift-held letters)", () => {
    assert.deepEqual(matchShortcut(shape("T", { mod: true })), {
      action: "new-tab",
    });
    assert.deepEqual(matchShortcut(shape("D", { mod: true, shift: true })), {
      action: "split-down",
    });
  });

  it("jumps to tabs 1-8 by number", () => {
    assert.deepEqual(matchShortcut(shape("1", { mod: true })), {
      action: "jump-tab",
      index: 0,
    });
    assert.deepEqual(matchShortcut(shape("8", { mod: true })), {
      action: "jump-tab",
      index: 7,
    });
    assert.equal(matchShortcut(shape("9", { mod: true })), null);
    assert.equal(matchShortcut(shape("1", { mod: true, shift: true })), null);
    assert.equal(matchShortcut(shape("1")), null);
  });

  it("treats + and = as font-bigger", () => {
    assert.deepEqual(matchShortcut(shape("=", { mod: true })), {
      action: "font-bigger",
    });
    assert.deepEqual(matchShortcut(shape("+", { mod: true })), {
      action: "font-bigger",
    });
  });

  it("rejects plain keys and unbound combos", () => {
    assert.equal(matchShortcut(shape("t")), null);
    assert.equal(matchShortcut(shape("Enter")), null);
    assert.equal(matchShortcut(shape("t", { mod: true, shift: true })), null);
    assert.equal(matchShortcut(shape("q", { mod: true })), null);
    assert.equal(matchShortcut(shape("F2", { mod: true })), null);
    assert.equal(matchShortcut(shape("1", { mod: true, alt: true })), null);
  });

  it("matches Mod+F for find in pane", () => {
    assert.deepEqual(matchShortcut(shape("f", { mod: true })), {
      action: "find-in-pane",
    });
    assert.equal(matchShortcut(shape("f", { mod: true, shift: true })), null);
  });
});

describe("native shortcut events", () => {
  const event = (key: string, code: string, opts: Partial<{
    metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; isComposing: boolean;
  }> = {}) => ({
    key, code, metaKey: false, ctrlKey: false, shiftKey: false,
    altKey: false, isComposing: false, ...opts,
  });

  it("matches shifted plus and workspace braces from real keyboard values", () => {
    assert.deepEqual(matchShortcutEvent(event("+", "Equal", { metaKey: true, shiftKey: true }), true), { action: "font-bigger" });
    assert.deepEqual(matchShortcutEvent(event("{", "BracketLeft", { metaKey: true, shiftKey: true }), true), { action: "prev-workspace" });
    assert.deepEqual(matchShortcutEvent(event("}", "BracketRight", { ctrlKey: true, shiftKey: true }), false), { action: "next-workspace" });
    assert.deepEqual(matchShortcutEvent(event("[", "BracketLeft", { metaKey: true }), true), { action: "prev-tab" });
  });

  it("uses only the platform Mod and leaves ordinary macOS terminal Ctrl untouched", () => {
    assert.equal(matchShortcutEvent(event("d", "KeyD", { ctrlKey: true }), true), null);
    assert.equal(matchShortcutEvent(event("t", "KeyT", { ctrlKey: true }), true), null);
    assert.equal(matchShortcutEvent(event("d", "KeyD", { ctrlKey: true, metaKey: true }), true), null);
    assert.equal(matchShortcutEvent(event("t", "KeyT", { metaKey: true }), false), null);
    assert.deepEqual(matchShortcutEvent(event("t", "KeyT", { metaKey: true }), true), { action: "new-tab" });
    assert.deepEqual(matchShortcutEvent(event("d", "KeyD", { ctrlKey: true }), false), { action: "split-right" });
    assert.deepEqual(matchShortcutEvent(event("ArrowLeft", "ArrowLeft", { altKey: true, shiftKey: true }), true), { action: "resize-pane", dir: "left" });
  });

  it("allows no background mutation while any overlay owns the keyboard or IME is composing", () => {
    for (const key of ["t", "w", "d", "n", "F2"]) {
      const native = event(key, key === "F2" ? "F2" : `Key${key.toUpperCase()}`, { metaKey: key !== "F2" });
      assert.notEqual(matchShortcutEvent(native, true), null);
      assert.equal(matchShortcutEvent(native, true, true), null);
    }
    assert.equal(matchShortcutEvent(event("t", "KeyT", { metaKey: true, isComposing: true }), true), null);
  });
});

describe("isEditableTarget", () => {
  it("detects inputs, textareas, selects, and editable nodes", () => {
    assert.equal(isEditableTarget({ tagName: "INPUT" }), true);
    assert.equal(isEditableTarget({ tagName: "textarea" }), true);
    assert.equal(isEditableTarget({ tagName: "SELECT" }), true);
    assert.equal(
      isEditableTarget({ tagName: "DIV", isContentEditable: true }),
      true,
    );
  });

  it("ignores buttons, divs, body, and null", () => {
    assert.equal(isEditableTarget({ tagName: "BUTTON" }), false);
    assert.equal(isEditableTarget({ tagName: "DIV" }), false);
    assert.equal(isEditableTarget({ tagName: "BODY" }), false);
    assert.equal(isEditableTarget(null), false);
    assert.equal(isEditableTarget({}), false);
  });
});

describe("labels", () => {
  it("detects mac platforms", () => {
    assert.equal(isMacPlatform("MacIntel"), true);
    assert.equal(isMacPlatform("macOS"), true);
    assert.equal(isMacPlatform("Win32"), false);
    assert.equal(isMacPlatform("Linux x86_64"), false);
  });

  it("formats mac and windows bindings", () => {
    assert.equal(modLabel(true), "⌘");
    assert.equal(modLabel(false), "Ctrl");
    const split = SHORTCUTS.find((b) => b.action === "split-down")!;
    assert.equal(formatBinding(split, true), "⌘⇧D");
    assert.equal(formatBinding(split, false), "Ctrl+Shift+D");
    const f2 = SHORTCUTS.find((b) => b.action === "rename-pane")!;
    assert.equal(formatBinding(f2, true), "F2");
    assert.equal(formatBinding(f2, false), "F2");
    const focus = SHORTCUTS.find(
      (b) => b.action === "focus-neighbor" && b.dir === "right",
    )!;
    assert.equal(formatBinding(focus, true), "⌘⌥→");
    assert.equal(formatBinding(focus, false), "Ctrl+Alt+→");
    const resize = SHORTCUTS.find(
      (b) => b.action === "resize-pane" && b.dir === "left",
    )!;
    assert.equal(formatBinding(resize, true), "⌥⇧←");
    assert.equal(formatBinding(resize, false), "Alt+Shift+←");
  });

  it("builds a cheat sheet with a jump-tab row", () => {
    const mac = cheatSheet(true);
    assert.equal(mac.length, SHORTCUTS.length + 1);
    assert.ok(mac.some((r) => r.keys === "⌘1–8" && r.blurb.includes("Jump")));
    const win = cheatSheet(false);
    assert.ok(win.some((r) => r.keys === "Ctrl+1–8"));
  });
});

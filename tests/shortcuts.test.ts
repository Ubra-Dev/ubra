import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  SHORTCUTS,
  cheatSheet,
  formatBinding,
  isEditableTarget,
  isMacPlatform,
  matchShortcut,
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
  it("matches every table binding", () => {
    for (const b of SHORTCUTS) {
      const got = matchShortcut(shape(b.key, { mod: b.mod, shift: b.shift }));
      assert.deepEqual(got, { action: b.action });
    }
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

  it("rejects plain keys, alt combos, and unbound mod combos", () => {
    assert.equal(matchShortcut(shape("t")), null);
    assert.equal(matchShortcut(shape("Enter")), null);
    assert.equal(matchShortcut(shape("t", { mod: true, alt: true })), null);
    assert.equal(matchShortcut(shape("t", { mod: true, shift: true })), null);
    assert.equal(matchShortcut(shape("q", { mod: true })), null);
    assert.equal(matchShortcut(shape("F2", { mod: true })), null);
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
  });

  it("builds a cheat sheet with a jump-tab row", () => {
    const mac = cheatSheet(true);
    assert.equal(mac.length, SHORTCUTS.length + 1);
    assert.ok(mac.some((r) => r.keys === "⌘1–8" && r.blurb.includes("Jump")));
    const win = cheatSheet(false);
    assert.ok(win.some((r) => r.keys === "Ctrl+1–8"));
  });
});

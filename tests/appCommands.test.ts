import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { acceleratorFor, canDispatch, matchEditingShortcut, menuGroups, shouldDispatchDom,
  type AppCommand, type CommandContext } from "../src/lib/appCommands.ts";

const context = (overrides: Partial<CommandContext> = {}): CommandContext => ({
  ready: true, blocked: false, settingsOpen: false, textFocus: false, writable: false,
  textSelection: false, terminalFocus: true, terminalAvailable: true, terminalSelection: true,
  terminalRunning: true, hasPane: true, hasWorkspace: true, tabCount: 2, workspaceIds: ["one", "two"],
  restartable: false, savedCommand: false, zoomed: false, ...overrides,
});

describe("application command eligibility", () => {
  it("blocks layout mutations during overlays, editing, and recovery", () => {
    const actions: AppCommand[] = ["new-tab", "close-workspace", "split-right", "move-pane-to-new-tab",
      "new-empty-workspace"];
    for (const action of actions) {
      assert.equal(canDispatch({ action }, context()), true, action);
      for (const invalid of [{ blocked: true }, { textFocus: true }, { ready: false }]) {
        assert.equal(canDispatch({ action }, context(invalid)), false, action);
      }
    }
    assert.equal(canDispatch({ action: "quit" }, context({ blocked: true, ready: false })), true);
    assert.equal(canDispatch({ action: "quit-stop-agents" }, context({ blocked: true, ready: false })), true);
    assert.equal(canDispatch({ action: "open-settings" }, context({ blocked: true })), false);
    assert.equal(canDispatch({ action: "keyboard-shortcuts" }, context({ blocked: true, settingsOpen: true })), true);
  });

  it("keeps workspace creation usable in the empty state and rejects missing close targets", () => {
    const empty = context({ hasPane: false, hasWorkspace: false, tabCount: 0, workspaceIds: [] });
    assert.equal(canDispatch({ action: "new-empty-workspace" }, empty), true);
    assert.equal(canDispatch({ action: "open-project" }, empty), true);
    for (const action of ["new-tab", "close-pane-or-tab", "close-workspace", "split-right"] as const) {
      assert.equal(canDispatch({ action }, empty), false);
    }
    assert.equal(canDispatch({ action: "switch-workspace", workspaceId: "missing" }, context()), false);
    assert.equal(canDispatch({ action: "switch-workspace", workspaceId: "one" }, context()), true);
    assert.equal(canDispatch({ action: "jump-tab", index: 2 }, context()), false);
    assert.equal(canDispatch({ action: "next-tab" }, context({ tabCount: 1 })), false);
  });

  it("allows restart only for exited panes", () => {
    assert.equal(canDispatch({ action: "restart-terminal" }, context()), false);
    assert.equal(canDispatch({ action: "restart-terminal" }, context({ restartable: true })), true);
    assert.equal(canDispatch({ action: "restart-terminal" }, context({ restartable: true, hasPane: false })), false);
  });

  it("uses terminal selection and never cuts or undoes terminal output", () => {
    assert.equal(canDispatch({ action: "copy" }, context({ terminalSelection: false })), false);
    assert.equal(canDispatch({ action: "copy" }, context()), true);
    assert.equal(canDispatch({ action: "paste" }, context({ terminalRunning: false })), false);
    for (const action of ["cut", "undo", "redo"] as const) assert.equal(canDispatch({ action }, context()), false);
    assert.equal(canDispatch({ action: "copy" }, context({ blocked: true })), false);
    const input = context({ blocked: true, textFocus: true, writable: true, textSelection: true });
    for (const action of ["copy", "paste", "cut", "undo", "redo", "select-all"] as const) {
      assert.equal(canDispatch({ action }, input), true, action);
    }
    assert.equal(canDispatch({ action: "paste" }, { ...input, writable: false }), false);
    assert.equal(canDispatch({ action: "copy" }, { ...input, writable: false }), true);
  });
});

describe("native menus and accelerators", () => {
  it("places application commands in the appropriate platform menus without duplicate IDs", () => {
    for (const mac of [false, true]) {
      const groups = menuGroups(mac);
      const ids = groups.flatMap((group) => group.items.flatMap((item) => item ? [item.id] : []));
      assert.equal(new Set(ids).size, ids.length);
      assert.ok(!ids.includes("saved-setups"));
      assert.ok(!ids.includes("save-template"));
      const application = groups.find((group) => group.label === (mac ? "Ubra" : "File"))!;
      assert.ok(application.items.some((item) => item?.action === "quit"));
      assert.ok(application.items.some((item) => item?.action === "quit-stop-agents"));
      assert.ok(application.items.some((item) => item?.action === "open-settings"));
      const about = groups.find((group) => group.items.some((item) => item?.system === "About"))!;
      assert.equal(about.label, mac ? "Ubra" : "Help");
      assert.equal(groups.some((group) => group.items.some((item) => item?.system === "Services")), mac);
      assert.equal(groups.some((group) => group.items.some((item) => item?.system === "BringAllToFront")), mac);
    }
  });

  it("derives app accelerators and keeps terminal control keys untouched on Windows/Linux", () => {
    assert.equal(acceleratorFor("split-down", false), "CmdOrCtrl+Shift+D");
    assert.equal(acceleratorFor("open-settings", true), "CmdOrCtrl+,");
    assert.equal(acceleratorFor("new-empty-workspace", true), null);
    for (const [action, key] of [["copy", "C"], ["paste", "V"], ["select-all", "A"]] as const) {
      assert.equal(acceleratorFor(action, true), `Cmd+${key}`);
      assert.equal(acceleratorFor(action, false), `Ctrl+Shift+${key}`);
      assert.equal(matchEditingShortcut({ key, mod: true, shift: false, alt: false }, false, true), null);
      assert.equal(matchEditingShortcut({ key, mod: true, shift: true, alt: false }, false, true), action);
    }
    assert.equal(matchEditingShortcut({ key: "c", mod: false, shift: false, alt: false }, true, true), null);
    assert.equal(matchEditingShortcut({ key: "z", mod: true, shift: false, alt: false }, false, true), null);
    for (const action of ["undo", "redo", "cut"] as const) assert.equal(acceleratorFor(action, false), null);
    assert.equal(acceleratorFor("undo", true), "Cmd+Z");
  });

  it("gives native accelerators sole dispatch ownership while retaining fallback shortcuts", () => {
    assert.equal(shouldDispatchDom("new-tab", true), false);
    assert.equal(shouldDispatchDom("new-tab", false), true);
    assert.equal(shouldDispatchDom("jump-tab", false), true);
    assert.equal(shouldDispatchDom("focus-neighbor", false), true);
    assert.equal(shouldDispatchDom("font-bigger", true, true), true);
    assert.equal(shouldDispatchDom("font-bigger", true, false), false);
  });
});

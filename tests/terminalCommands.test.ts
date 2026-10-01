import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { TerminalCommandRegistry, type TerminalControls } from "../src/lib/terminalCommands.ts";

const controls = (): TerminalControls => ({ selection: () => "selected", paste: () => {}, selectAll: () => {}, running: () => true });

describe("terminal menu registrations", () => {
  it("ignores stale disposal after remount and removes the current registration on close", () => {
    const registry = new TerminalCommandRegistry();
    const old = registry.register("pane", controls());
    const replacement = controls();
    const current = registry.register("pane", replacement);
    old();
    assert.equal(registry.get("pane"), replacement);
    current();
    assert.equal(registry.get("pane"), undefined);
  });

  it("rejects restart while running and preserves a replacement restart registration", () => {
    const registry = new TerminalCommandRegistry();
    let exited = false;
    let restarted = 0;
    const old = registry.registerRestart("pane", { available: () => true, restart: () => { throw new Error("stale restart"); } });
    const current = registry.registerRestart("pane", {
      available: () => exited,
      restart: () => { restarted++; exited = false; },
    });
    old();
    assert.equal(registry.restart("pane"), false);
    exited = true;
    assert.equal(registry.canRestart("pane"), true);
    assert.equal(registry.restart("pane"), true);
    assert.equal(registry.restart("pane"), false);
    assert.equal(restarted, 1);
    current();
    assert.equal(registry.canRestart("pane"), false);
  });

  it("notifies menu updates and stops notifying disposed subscribers", () => {
    const registry = new TerminalCommandRegistry();
    let changes = 0;
    const stop = registry.subscribe(() => { changes++; });
    const unregister = registry.register("pane", controls());
    registry.changed();
    assert.equal(changes, 2);
    stop();
    unregister();
    assert.equal(changes, 2);
  });
});

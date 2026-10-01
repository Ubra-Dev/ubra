import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { implicitLaunchCommand } from "../src/lib/agentLaunch.ts";

describe("implicitLaunchCommand", () => {
  it("returns the trimmed candidate when auto-launch is on", () => {
    assert.equal(implicitLaunchCommand(true, "codex"), "codex");
    assert.equal(implicitLaunchCommand(true, "  claude --yes "), "claude --yes");
  });

  it("returns null when auto-launch is off, even with a candidate", () => {
    assert.equal(implicitLaunchCommand(false, "codex"), null);
    assert.equal(implicitLaunchCommand(false, "  claude "), null);
  });

  it("returns null for a blank or missing candidate when on", () => {
    assert.equal(implicitLaunchCommand(true, ""), null);
    assert.equal(implicitLaunchCommand(true, "   "), null);
    assert.equal(implicitLaunchCommand(true, null), null);
    assert.equal(implicitLaunchCommand(true, undefined), null);
  });
});

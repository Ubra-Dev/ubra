import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { CUSTOM_COMMAND, resolveAgentCommand } from "../src/lib/agentClis.ts";

describe("resolveAgentCommand", () => {
  it("returns the selected cli verbatim", () => {
    assert.equal(resolveAgentCommand("codex", ""), "codex");
    assert.equal(resolveAgentCommand("claude", "ignored"), "claude");
  });

  it("returns the trimmed custom command for the custom option", () => {
    assert.equal(resolveAgentCommand(CUSTOM_COMMAND, "  my-agent --yes "), "my-agent --yes");
    assert.equal(resolveAgentCommand(CUSTOM_COMMAND, "   "), "");
  });
});

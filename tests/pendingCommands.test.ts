import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { PendingCommands } from "../src/lib/pendingCommands.ts";

describe("PendingCommands", () => {
  it("queues and takes a trimmed command per pane", () => {
    const pending = new PendingCommands();
    pending.queue("pane-1", "  codex ");
    pending.queue("pane-2", "claude");
    assert.equal(pending.take("pane-1"), "codex");
    assert.equal(pending.take("pane-2"), "claude");
  });

  it("takes once and misses unknown panes", () => {
    const pending = new PendingCommands();
    pending.queue("pane-1", "codex");
    assert.equal(pending.take("pane-1"), "codex");
    assert.equal(pending.take("pane-1"), null);
    assert.equal(pending.take("pane-9"), null);
  });

  it("ignores blank commands and overwrites on re-queue", () => {
    const pending = new PendingCommands();
    pending.queue("pane-1", "   ");
    assert.equal(pending.take("pane-1"), null);
    pending.queue("pane-1", "codex");
    pending.queue("pane-1", "claude");
    assert.equal(pending.take("pane-1"), "claude");
  });

  it("drops single and multiple panes", () => {
    const pending = new PendingCommands();
    pending.queue("pane-1", "codex");
    pending.queue("pane-2", "claude");
    pending.queue("pane-3", "gemini");
    pending.drop("pane-1");
    pending.dropMany(["pane-2", "pane-9"]);
    assert.equal(pending.take("pane-1"), null);
    assert.equal(pending.take("pane-2"), null);
    assert.equal(pending.take("pane-3"), "gemini");
  });
});

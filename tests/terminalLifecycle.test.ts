import { it } from "node:test";
import assert from "node:assert/strict";
import { TerminalAttachment } from "../src/lib/terminalLifecycle.ts";

it("snapshot excludes overlapping operations but preserves newer output and buffered exit", () => {
  const attachment = new TerminalAttachment();
  attachment.output({ id: 8, sequence: 2, data: "overlap\x1b[2J" });
  attachment.output({ id: 9, sequence: 99, data: "unrelated" });
  attachment.output({ id: 8, sequence: 4, data: "newest" });
  attachment.output({ id: 8, sequence: 3, data: "new" });
  attachment.exit({ id: 8, success: true, code: 0 });
  const restored = attachment.restore(8, { sequence: 2, data: "screen", cols: 80, rows: 24 });
  assert.deepEqual(restored, { chunks: ["screen", "new", "newest"], exit: { id: 8, success: true, code: 0 } });
  assert.equal(attachment.output({ id: 8, sequence: 4, data: "duplicate" }), null);
  assert.equal(attachment.output({ id: 8, sequence: 5, data: "later" }), "later");
  assert.equal(attachment.output({ id: 9, sequence: 100, data: "unrelated" }), null);
});

it("a short-lived spawn with no surviving snapshot retains its output and exit", () => {
  const attachment = new TerminalAttachment();
  attachment.output({ id: 1, sequence: 1, data: "result" });
  attachment.exit({ id: 1, success: false, code: 7 });
  assert.deepEqual(attachment.restore(1, null), {
    chunks: ["result"], exit: { id: 1, success: false, code: 7 },
  });
  assert.equal(attachment.exit({ id: 2, success: true, code: 0 }), null);
  assert.deepEqual(attachment.exit({ id: 1, success: false, code: 7 }), { id: 1, success: false, code: 7 });
});

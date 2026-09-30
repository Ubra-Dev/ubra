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

it("assembleSnapshot concatenates pages and keeps the watermark", async () => {
  const { assembleSnapshot } = await import("../src/lib/terminalLifecycle.ts");
  const fetched: number[] = [];
  const snapshot = await assembleSnapshot(
    { pane: 3, page: 0, pages: 3, data: "a", sequence: 9, incarnation: 4, epoch: 7, cols: 80, rows: 24 },
    async (page) => {
      fetched.push(page);
      return ["b", "c"][page - 1];
    },
  );
  assert.deepEqual(snapshot, { data: "abc", sequence: 9, cols: 80, rows: 24 });
  assert.deepEqual(fetched, [1, 2]);
});

it("assembleSnapshot skips fetching for single-page snapshots", async () => {
  const { assembleSnapshot } = await import("../src/lib/terminalLifecycle.ts");
  const snapshot = await assembleSnapshot(
    { pane: 3, page: 0, pages: 1, data: "solo", sequence: 2, incarnation: 4, epoch: 7, cols: 80, rows: 24 },
    async () => {
      throw new Error("must not fetch");
    },
  );
  assert.equal(snapshot.data, "solo");
});

it("assembleReplay uses inline history unless truncated, then pages fully", async () => {
  const { assembleReplay } = await import("../src/lib/terminalLifecycle.ts");
  assert.equal(
    await assembleReplay("inline", 1, false, async () => {
      throw new Error("must not fetch");
    }),
    "inline",
  );
  const full = await assembleReplay("tail", 3, true, async (page) => `p${page};`);
  assert.equal(full, "p0;p1;p2;");
});

it("snapshotFailurePlan retries bounded times, then surfaces unavailable", async () => {
  const { snapshotFailurePlan } = await import("../src/lib/terminalLifecycle.ts");
  assert.equal(snapshotFailurePlan(0), "retry");
  assert.equal(snapshotFailurePlan(1), "retry");
  assert.equal(snapshotFailurePlan(2), "retry");
  assert.equal(snapshotFailurePlan(3), "unavailable");
  assert.equal(snapshotFailurePlan(99), "unavailable");
});

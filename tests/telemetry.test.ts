import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { bucketizeDuration } from "../src/lib/telemetry.ts";

describe("bucketizeDuration", () => {
  it("buckets session lengths coarsely", () => {
    assert.equal(bucketizeDuration(0), "under_minute");
    assert.equal(bucketizeDuration(59_999), "under_minute");
    assert.equal(bucketizeDuration(60_000), "under_5m");
    assert.equal(bucketizeDuration(5 * 60_000 - 1), "under_5m");
    assert.equal(bucketizeDuration(5 * 60_000), "under_30m");
    assert.equal(bucketizeDuration(30 * 60_000 - 1), "under_30m");
    assert.equal(bucketizeDuration(30 * 60_000), "over_30m");
    assert.equal(bucketizeDuration(8 * 3_600_000), "over_30m");
  });

  it("treats non-finite and negative input as the smallest bucket", () => {
    assert.equal(bucketizeDuration(-5), "under_minute");
    assert.equal(bucketizeDuration(NaN), "under_minute");
    assert.equal(bucketizeDuration(Infinity), "over_30m");
  });
});

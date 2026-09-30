import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { formatResetCountdown, formatUpdatedAgo } from "../src/lib/usage.ts";

const NOW = 1790743269;

describe("formatResetCountdown", () => {
  it("renders hours and minutes", () => {
    assert.equal(formatResetCountdown(NOW + 2 * 3600 + 14 * 60, NOW), "in 2h 14m");
  });

  it("renders minutes and seconds below an hour", () => {
    assert.equal(formatResetCountdown(NOW + 9 * 60 + 30, NOW), "in 9m");
    assert.equal(formatResetCountdown(NOW + 45, NOW), "in 45s");
  });

  it("renders days past 48 hours", () => {
    assert.equal(
      formatResetCountdown(NOW + 3 * 86400 + 5 * 3600, NOW),
      "in 3d 5h",
    );
  });

  it("reports resetting for past timestamps", () => {
    assert.equal(formatResetCountdown(NOW, NOW), "resetting…");
    assert.equal(formatResetCountdown(NOW - 10, NOW), "resetting…");
  });
});

describe("formatUpdatedAgo", () => {
  it("reports just now under a minute", () => {
    assert.equal(formatUpdatedAgo(NOW - 5, NOW), "just now");
    assert.equal(formatUpdatedAgo(NOW + 30, NOW), "just now");
  });

  it("renders minutes and hours", () => {
    assert.equal(formatUpdatedAgo(NOW - 5 * 60, NOW), "5m ago");
    assert.equal(formatUpdatedAgo(NOW - 2 * 3600, NOW), "2h ago");
  });
});

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  claimLiveId,
  dropLiveId,
  peekLiveId,
} from "../src/lib/ptySessions.ts";

describe("ptySessions", () => {
  it("returns null for unknown panes", () => {
    assert.equal(peekLiveId("missing-pane"), null);
  });

  it("claims and peeks a live id", () => {
    claimLiveId("pane-a", 7);
    assert.equal(peekLiveId("pane-a"), 7);
  });

  it("claim overwrites a stale entry (respawn replaces)", () => {
    claimLiveId("pane-b", 1);
    claimLiveId("pane-b", 2);
    assert.equal(peekLiveId("pane-b"), 2);
  });

  it("drop removes only the matching live id", () => {
    claimLiveId("pane-c", 10);
    dropLiveId("pane-c", 9);
    assert.equal(peekLiveId("pane-c"), 10);
    dropLiveId("pane-c", 10);
    assert.equal(peekLiveId("pane-c"), null);
  });

  it("drop of an unknown key is a no-op", () => {
    dropLiveId("never-claimed", 1);
    assert.equal(peekLiveId("never-claimed"), null);
  });
});

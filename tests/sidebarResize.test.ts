import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_RIGHT_PANEL_WIDTH,
  DEFAULT_SIDEBAR_WIDTH,
  DEFAULT_SPLIT_RATIO,
  MAX_SIDEBAR_WIDTH,
  MIN_SIDEBAR_WIDTH,
  MIN_SPLIT_PANE_PX,
  clampSidebarWidth,
  clampSplitRatio,
  effectiveSplitRatio,
  ratioFromPointer,
  stepSidebarWidth,
  stepSplitRatio,
} from "../src/lib/sidebarResize.ts";

describe("clampSidebarWidth", () => {
  it("falls back to the default for non-finite values", () => {
    assert.equal(clampSidebarWidth(NaN), DEFAULT_SIDEBAR_WIDTH);
    assert.equal(clampSidebarWidth(Infinity), DEFAULT_SIDEBAR_WIDTH);
  });

  it("accepts a custom fallback for other panels", () => {
    assert.equal(clampSidebarWidth(NaN, DEFAULT_RIGHT_PANEL_WIDTH), 250);
    assert.equal(clampSidebarWidth(250, DEFAULT_RIGHT_PANEL_WIDTH), 250);
    assert.equal(clampSidebarWidth(9999, DEFAULT_RIGHT_PANEL_WIDTH), MAX_SIDEBAR_WIDTH);
  });

  it("clamps to the min/max width", () => {
    assert.equal(clampSidebarWidth(0), MIN_SIDEBAR_WIDTH);
    assert.equal(clampSidebarWidth(9999), MAX_SIDEBAR_WIDTH);
  });

  it("rounds to whole pixels", () => {
    assert.equal(clampSidebarWidth(190.6), 191);
  });
});

describe("stepSidebarWidth", () => {
  it("steps by 10px and clamps at the bounds", () => {
    assert.equal(stepSidebarWidth(190, 1), 200);
    assert.equal(stepSidebarWidth(190, -1), 180);
    assert.equal(stepSidebarWidth(MAX_SIDEBAR_WIDTH, 1), MAX_SIDEBAR_WIDTH);
    assert.equal(stepSidebarWidth(MIN_SIDEBAR_WIDTH, -1), MIN_SIDEBAR_WIDTH);
  });
});

describe("clampSplitRatio", () => {
  it("falls back to the default for non-finite values", () => {
    assert.equal(clampSplitRatio(NaN), DEFAULT_SPLIT_RATIO);
  });

  it("clamps into 0..1", () => {
    assert.equal(clampSplitRatio(-2), 0);
    assert.equal(clampSplitRatio(2), 1);
  });

  it("rounds away float dust", () => {
    assert.equal(clampSplitRatio(0.15000000000000002), 0.15);
  });
});

describe("stepSplitRatio", () => {
  it("steps by 0.05 without float dust", () => {
    assert.equal(stepSplitRatio(0.5, -1), 0.45);
    assert.equal(stepSplitRatio(0.1, -1), 0.05);
    assert.equal(stepSplitRatio(0.05, -1), 0);
    assert.equal(stepSplitRatio(1, 1), 1);
  });
});

describe("ratioFromPointer", () => {
  it("maps pointer Y to a 0..1 ratio", () => {
    assert.equal(ratioFromPointer(150, 100, 100), 0.5);
    assert.equal(ratioFromPointer(100, 100, 100), 0);
  });

  it("falls back to the default for degenerate containers", () => {
    assert.equal(ratioFromPointer(150, 100, 0), DEFAULT_SPLIT_RATIO);
    assert.equal(ratioFromPointer(150, 100, -10), DEFAULT_SPLIT_RATIO);
  });
});

describe("effectiveSplitRatio", () => {
  it("keeps both panes above the minimum height", () => {
    assert.equal(effectiveSplitRatio(0.99, 400, 80), 0.8);
    assert.equal(effectiveSplitRatio(0.01, 400, 80), 0.2);
    assert.equal(effectiveSplitRatio(0.5, 400, 80), 0.5);
  });

  it("uses the module minimum by default", () => {
    assert.equal(
      effectiveSplitRatio(0.01, 400),
      MIN_SPLIT_PANE_PX / 400,
    );
  });

  it("falls back to the default when the container fits no split", () => {
    assert.equal(effectiveSplitRatio(0.9, 100, 80), DEFAULT_SPLIT_RATIO);
  });

  it("ignores invalid measurements", () => {
    assert.equal(effectiveSplitRatio(0.7, 0, 80), 0.7);
    assert.equal(effectiveSplitRatio(0.7, 400, 0), 0.7);
  });
});

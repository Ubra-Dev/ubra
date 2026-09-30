import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_UI_SCALE,
  MAX_UI_SCALE,
  MIN_UI_SCALE,
  clampUiScale,
  stepUiScale,
  uiZoomStyle,
} from "../src/lib/uiScale.ts";

describe("clampUiScale", () => {
  it("returns default for non-finite input", () => {
    assert.equal(clampUiScale(NaN), DEFAULT_UI_SCALE);
    assert.equal(clampUiScale(Infinity), DEFAULT_UI_SCALE);
    assert.equal(clampUiScale(-Infinity), DEFAULT_UI_SCALE);
  });

  it("clamps below min and above max", () => {
    assert.equal(clampUiScale(MIN_UI_SCALE - 25), MIN_UI_SCALE);
    assert.equal(clampUiScale(MAX_UI_SCALE + 25), MAX_UI_SCALE);
  });

  it("rounds to whole percent", () => {
    assert.equal(clampUiScale(104.6), 105);
  });
});

describe("stepUiScale", () => {
  it("steps up and down by one grid step", () => {
    assert.equal(stepUiScale(100, 1), 110);
    assert.equal(stepUiScale(100, -1), 90);
  });

  it("stays at the max when stepping past it", () => {
    assert.equal(stepUiScale(MAX_UI_SCALE, 1), MAX_UI_SCALE);
  });

  it("lands on the grid from a corrupt in-memory value", () => {
    assert.equal(stepUiScale(NaN, 1), 110);
    assert.equal(stepUiScale(MAX_UI_SCALE + 50, 1), MAX_UI_SCALE);
  });
});

describe("uiZoomStyle", () => {
  it("formats the clamped scale as a zoom style", () => {
    assert.equal(uiZoomStyle(100), "zoom:100%");
    assert.equal(uiZoomStyle(MAX_UI_SCALE + 50), `zoom:${MAX_UI_SCALE}%`);
  });
});

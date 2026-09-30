import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_UI_SCALE,
  MAX_UI_SCALE,
  MIN_UI_SCALE,
  clampUiScale,
  stepUiScale,
  uiTextScaleStyle,
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

describe("uiTextScaleStyle", () => {
  it("scales interface text independently of the viewport", () => {
    assert.equal(uiTextScaleStyle(75), "--ui-text-scale:0.75");
    assert.equal(uiTextScaleStyle(100), "--ui-text-scale:1");
    assert.equal(uiTextScaleStyle(150), "--ui-text-scale:1.5");
  });

  it("clamps saved values and defaults invalid input", () => {
    assert.equal(uiTextScaleStyle(MAX_UI_SCALE + 50), "--ui-text-scale:1.5");
    assert.equal(uiTextScaleStyle(MIN_UI_SCALE - 50), "--ui-text-scale:0.75");
    assert.equal(uiTextScaleStyle(NaN), "--ui-text-scale:1");
  });
});

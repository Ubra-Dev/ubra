import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  BRAND_GRADIENT_STOPS,
  DEFAULT_LOADING_TEXT,
  brandGradientImage,
} from "../src/lib/loadingGradient.ts";

describe("BRAND_GRADIENT_STOPS", () => {
  it("runs cyan to yellow across seven logo stops", () => {
    assert.deepEqual([...BRAND_GRADIENT_STOPS], [
      "#01eafc",
      "#0178fc",
      "#7717fc",
      "#ff00ff",
      "#fc3672",
      "#fb7c36",
      "#fcce05",
    ]);
  });

  it("uses lowercase hex colors only", () => {
    for (const stop of BRAND_GRADIENT_STOPS) {
      assert.match(stop, /^#[0-9a-f]{6}$/);
    }
  });
});

describe("DEFAULT_LOADING_TEXT", () => {
  it("defaults to Loading...", () => {
    assert.equal(DEFAULT_LOADING_TEXT, "Loading...");
  });
});

describe("brandGradientImage", () => {
  it("emits a horizontal gradient through every stop by default", () => {
    assert.equal(
      brandGradientImage(),
      "linear-gradient(90deg, #01eafc, #0178fc, #7717fc, #ff00ff, #fc3672, #fb7c36, #fcce05)",
    );
  });

  it("honors a custom angle", () => {
    assert.ok(brandGradientImage(45).startsWith("linear-gradient(45deg, "));
  });
});

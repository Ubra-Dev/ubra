import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  deliveryLabel,
  fleetSummary,
  layoutTotalsLabel,
  platformLabel,
  saveState,
  soundLabel,
} from "../src/lib/statusBar.ts";
import {
  defaultLayout,
  defaultWorkspace,
  gridTab,
} from "../src/lib/layout.ts";

describe("fleetSummary", () => {
  it("returns null when there is nothing to report", () => {
    assert.equal(fleetSummary({ working: 0, blocked: 0, review: 0 }), null);
  });

  it("lists each nonzero group in working/blocked/review order", () => {
    assert.equal(fleetSummary({ working: 2, blocked: 0, review: 0 }), "2 working");
    assert.equal(fleetSummary({ working: 0, blocked: 1, review: 0 }), "1 blocked");
    assert.equal(
      fleetSummary({ working: 2, blocked: 1, review: 3 }),
      "2 working · 1 blocked · 3 need review",
    );
  });

  it("uses singular grammar for one review", () => {
    assert.equal(fleetSummary({ working: 0, blocked: 0, review: 1 }), "1 needs review");
  });
});

describe("saveState", () => {
  it("prefers error over saving, and saving over saved", () => {
    assert.equal(saveState({ saving: false, error: null }), "saved");
    assert.equal(saveState({ saving: true, error: null }), "saving");
    assert.equal(saveState({ saving: false, error: "disk full" }), "error");
    assert.equal(saveState({ saving: true, error: "disk full" }), "error");
  });
});

describe("layoutTotalsLabel", () => {
  it("returns an empty label without a layout", () => {
    assert.equal(layoutTotalsLabel(null), "");
  });

  it("uses singular nouns for a fresh layout", () => {
    assert.equal(layoutTotalsLabel(defaultLayout()), "1 workspace · 1 tab · 1 pane");
  });

  it("counts tabs and panes across workspaces", () => {
    const layout = defaultLayout();
    const extra = defaultWorkspace("extra");
    extra.tabs = [gridTab(), gridTab("second")];
    layout.workspaces.push(extra);
    assert.equal(layoutTotalsLabel(layout), "2 workspaces · 3 tabs · 9 panes");
  });
});

describe("platformLabel", () => {
  it("maps navigator.platform values to display names", () => {
    assert.equal(platformLabel("MacIntel"), "macOS");
    assert.equal(platformLabel("Win32"), "Windows");
    assert.equal(platformLabel("Linux x86_64"), "Linux");
  });

  it("returns null when the platform is unrecognized", () => {
    assert.equal(platformLabel(""), null);
    assert.equal(platformLabel("FreeBSD x64"), null);
  });
});

describe("deliveryLabel", () => {
  it("labels each delivery mode", () => {
    assert.equal(deliveryLabel("off"), "Off");
    assert.equal(deliveryLabel("inapp"), "In-app");
    assert.equal(deliveryLabel("system"), "System");
  });
});

describe("soundLabel", () => {
  it("labels the sound switch", () => {
    assert.equal(soundLabel(true), "On");
    assert.equal(soundLabel(false), "Off");
  });
});

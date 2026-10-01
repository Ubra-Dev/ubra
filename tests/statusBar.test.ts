import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  branchTooltip,
  crumbDotClass,
  deliveryLabel,
  fleetSummary,
  notifyTooltip,
  saveState,
  soundLabel,
  updateSegment,
  USAGE_WARN_THRESHOLD,
  usageWarning,
} from "../src/lib/statusBar.ts";
import type { CliUsage } from "../src/lib/usage.ts";

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

describe("updateSegment", () => {
  const base = {
    version: null as string | null,
    downloadedBytes: 0,
    totalBytes: null as number | null,
    error: null as string | null,
  };

  it("offers a manual check until the first check completes", () => {
    assert.deepEqual(updateSegment({ ...base, phase: "idle", checked: false }), {
      text: "Check for updates",
      title: "Never checked — click to check for updates",
      action: "check",
    });
    assert.equal(updateSegment({ ...base, phase: "idle", checked: true }), null);
  });

  it("reports checking without an action", () => {
    assert.deepEqual(updateSegment({ ...base, phase: "checking", checked: false }), {
      text: "Checking for updates…",
      title: "Checking for updates…",
      action: null,
    });
  });

  it("names the version when one is available", () => {
    assert.deepEqual(
      updateSegment({ ...base, phase: "available", checked: true, version: "0.2.0" }),
      {
        text: "Update to 0.2.0",
        title: "Version 0.2.0 available — click to download and install",
        action: "download",
      },
    );
    assert.deepEqual(updateSegment({ ...base, phase: "available", checked: true }), {
      text: "Update available",
      title: "Version unknown available — click to download and install",
      action: "download",
    });
  });

  it("shows download progress capped below 100%", () => {
    assert.deepEqual(
      updateSegment({
        ...base,
        phase: "downloading",
        checked: true,
        downloadedBytes: 512,
        totalBytes: 1024,
      }),
      {
        text: "Downloading update 50%",
        title: "Downloading update…",
        action: null,
      },
    );
    assert.deepEqual(
      updateSegment({
        ...base,
        phase: "downloading",
        checked: true,
        downloadedBytes: 2048,
        totalBytes: 1024,
      }),
      {
        text: "Downloading update 99%",
        title: "Downloading update…",
        action: null,
      },
    );
    assert.deepEqual(updateSegment({ ...base, phase: "downloading", checked: true }), {
      text: "Downloading update…",
      title: "Downloading update…",
      action: null,
    });
  });

  it("asks for a restart when ready", () => {
    assert.deepEqual(
      updateSegment({ ...base, phase: "ready", checked: true, version: "0.2.0" }),
      {
        text: "Restart to update",
        title: "Update installed — click to restart",
        action: "relaunch",
      },
    );
  });

  it("offers a retry with the error in the tooltip", () => {
    assert.deepEqual(
      updateSegment({ ...base, phase: "error", checked: true, error: "net down" }),
      {
        text: "Update failed",
        title: "net down — click to retry",
        action: "check",
      },
    );
  });
});

describe("usageWarning", () => {
  function entry(cli: string, percents: (number | undefined)[], resetsAt = 7_200): CliUsage {
    return {
      cli,
      status: "ready",
      snapshot: {
        cli,
        source: "test",
        windows: percents.map((percentUsed, i) => ({
          label: `window ${i}`,
          percentUsed,
          resetsAt: resetsAt + i,
        })),
        fetchedAt: 0,
      },
    };
  }

  it("stays hidden below the threshold", () => {
    assert.equal(USAGE_WARN_THRESHOLD, 80);
    assert.equal(usageWarning({}, {}, 3_600), null);
    assert.equal(
      usageWarning({ codex: entry("codex", [79]) }, { codex: "Codex" }, 3_600),
      null,
    );
  });

  it("ignores entries that are not ready usage", () => {
    const offline: CliUsage = { cli: "codex", status: "offline" };
    const noPercent = entry("codex", [undefined]);
    assert.equal(usageWarning({ codex: offline }, {}, 3_600), null);
    assert.equal(usageWarning({ codex: noPercent }, {}, 3_600), null);
  });

  it("warns on the worst window with a reset countdown", () => {
    const warning = usageWarning(
      { codex: entry("codex", [42, 82]) },
      { codex: "Codex" },
      3_600,
    );
    assert.deepEqual(warning, {
      text: "Codex 82%",
      title: "Codex 82% (resets in 1h 0m) — open Usage",
    });
  });

  it("ranks clis worst-first and falls back to the cli stem", () => {
    const warning = usageWarning(
      {
        codex: entry("codex", [81]),
        claude: entry("claude", [95]),
      },
      { codex: "Codex" },
      3_600,
    );
    assert.deepEqual(warning, {
      text: "claude 95% +1",
      title: "claude 95% (resets in 1h 0m) · Codex 81% (resets in 1h 0m) — open Usage",
    });
  });
});

describe("crumbDotClass", () => {
  it("maps live rollups to dot classes", () => {
    assert.equal(crumbDotClass("working", true), "working");
    assert.equal(crumbDotClass("blocked", true), "blocked");
    assert.equal(crumbDotClass("attention", true), "attention");
  });

  it("hides the dot without an agent or a quiet rollup", () => {
    assert.equal(crumbDotClass("working", false), null);
    assert.equal(crumbDotClass("idle", true), null);
    assert.equal(crumbDotClass("done", true), null);
    assert.equal(crumbDotClass("unknown", true), null);
  });
});

describe("branchTooltip", () => {
  it("counts changes with singular grammar", () => {
    assert.equal(
      branchTooltip({ branch: "main", changes: 0 }),
      "main · clean — open Source Control",
    );
    assert.equal(
      branchTooltip({ branch: "main", changes: 1 }),
      "main · 1 change — open Source Control",
    );
    assert.equal(
      branchTooltip({ branch: "feat/x", changes: 3 }),
      "feat/x · 3 changes — open Source Control",
    );
  });

  it("reports unknown status", () => {
    assert.equal(
      branchTooltip({ branch: "main", changes: null }),
      "main · status unknown — open Source Control",
    );
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

describe("notifyTooltip", () => {
  it("describes delivery and sound with the toggle hint", () => {
    assert.equal(
      notifyTooltip({ delivery: "system", soundEnabled: true, mutedCount: 0 }),
      "Notifications: System · Sound On — click to toggle sound",
    );
    assert.equal(
      notifyTooltip({ delivery: "inapp", soundEnabled: false, mutedCount: 0 }),
      "Notifications: In-app · Sound Off — click to toggle sound",
    );
  });

  it("counts muted agents with singular grammar", () => {
    assert.equal(
      notifyTooltip({ delivery: "system", soundEnabled: true, mutedCount: 1 }),
      "Notifications: System · Sound On · 1 muted agent — click to toggle sound",
    );
    assert.equal(
      notifyTooltip({ delivery: "system", soundEnabled: true, mutedCount: 2 }),
      "Notifications: System · Sound On · 2 muted agents — click to toggle sound",
    );
  });
});

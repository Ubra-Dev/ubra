import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  FIXED_CHIME_FILE,
  describeNotifyOutcome,
  parseDelivery,
  parseNotifyOutcome,
  parseNotifyPermission,
  parseToastPosition,
  playbackPayload,
  routeNotification,
  testNotificationPayload,
} from "../src/lib/notify.ts";

describe("parseDelivery", () => {
  it("accepts known modes and falls back to system", () => {
    assert.equal(parseDelivery("off"), "off");
    assert.equal(parseDelivery("inapp"), "inapp");
    assert.equal(parseDelivery("system"), "system");
    assert.equal(parseDelivery("both"), "system");
    assert.equal(parseDelivery(null), "system");
    assert.equal(parseDelivery(undefined), "system");
    assert.equal(parseDelivery(42), "system");
  });
});

describe("parseToastPosition", () => {
  it("accepts corners and falls back to bottom-right", () => {
    assert.equal(parseToastPosition("top-left"), "top-left");
    assert.equal(parseToastPosition("bottom-right"), "bottom-right");
    assert.equal(parseToastPosition("center"), "bottom-right");
    assert.equal(parseToastPosition(null), "bottom-right");
  });
});

describe("routeNotification", () => {
  it("routes one visual channel per delivery mode", () => {
    const base = { soundEnabled: false, mutedClis: [] as string[] };
    assert.deepEqual(
      routeNotification({ ...base, delivery: "system" }),
      { toast: false, system: true, sound: false },
    );
    assert.deepEqual(
      routeNotification({ ...base, delivery: "inapp" }),
      { toast: true, system: false, sound: false },
    );
    assert.deepEqual(
      routeNotification({ ...base, delivery: "off" }),
      { toast: false, system: false, sound: false },
    );
  });

  it("plays sound independently of delivery, like Herdr", () => {
    assert.equal(
      routeNotification({ delivery: "off", soundEnabled: true, mutedClis: [] })
        .sound,
      true,
    );
    assert.equal(
      routeNotification({ delivery: "system", soundEnabled: false, mutedClis: [] })
        .sound,
      false,
    );
  });

  it("mutes per-agent cli, case-insensitively", () => {
    const base = {
      delivery: "system" as const,
      soundEnabled: true,
      mutedClis: ["droid"],
    };
    assert.equal(routeNotification({ ...base, cli: "droid" }).sound, false);
    assert.equal(routeNotification({ ...base, cli: "Droid" }).sound, false);
    assert.equal(routeNotification({ ...base, cli: "claude" }).sound, true);
    assert.equal(routeNotification({ ...base, cli: undefined }).sound, true);
  });
});

describe("playback selection", () => {
  it("always plays the fixed chime file", () => {
    assert.deepEqual(playbackPayload("done"), {
      kind: "done",
      file: FIXED_CHIME_FILE,
    });
    assert.deepEqual(playbackPayload("request"), {
      kind: "request",
      file: FIXED_CHIME_FILE,
    });
  });
});

describe("testNotificationPayload", () => {
  it("marks itself as a test with a safe empty jump target", () => {
    const t = testNotificationPayload();
    assert.equal(t.nodeId, "");
    assert.equal(t.kind, "done");
  });

  it("routes like a real finish, honoring mute and delivery", () => {
    const t = testNotificationPayload();
    assert.deepEqual(
      routeNotification({
        delivery: "system",
        soundEnabled: true,
        mutedClis: [],
        cli: t.cli,
      }),
      { toast: false, system: true, sound: true },
    );
    const muted = routeNotification({
      delivery: "system",
      soundEnabled: true,
      mutedClis: [t.cli],
      cli: t.cli,
    });
    assert.equal(muted.sound, false);
    assert.equal(muted.system, true);
  });
});

describe("parseNotifyPermission", () => {
  it("accepts backend states and falls back to unknown", () => {
    assert.equal(parseNotifyPermission("granted"), "granted");
    assert.equal(parseNotifyPermission("denied"), "denied");
    assert.equal(parseNotifyPermission("prompt"), "prompt");
    assert.equal(parseNotifyPermission("unknown"), "unknown");
    assert.equal(parseNotifyPermission("yes"), "unknown");
    assert.equal(parseNotifyPermission(true), "unknown");
    assert.equal(parseNotifyPermission(null), "unknown");
    assert.equal(parseNotifyPermission(undefined), "unknown");
  });
});

describe("parseNotifyOutcome", () => {
  it("passes backend reports through with their status", () => {
    assert.deepEqual(parseNotifyOutcome({ status: "delivered" }), {
      status: "delivered",
    });
    assert.deepEqual(parseNotifyOutcome({ status: "denied" }), {
      status: "denied",
    });
    assert.deepEqual(parseNotifyOutcome({ status: "attempted" }), {
      status: "attempted",
    });
    assert.deepEqual(
      parseNotifyOutcome({ status: "unavailable", reason: "not-determined" }),
      { status: "unavailable", reason: "not-determined" },
    );
  });

  it("normalizes malformed payloads to unavailable", () => {
    assert.deepEqual(parseNotifyOutcome({ status: "unavailable" }), {
      status: "unavailable",
      reason: "unknown",
    });
    assert.deepEqual(parseNotifyOutcome({ status: "bogus" }), {
      status: "unavailable",
      reason: "bad-outcome",
    });
    assert.deepEqual(parseNotifyOutcome(null), {
      status: "unavailable",
      reason: "bad-outcome",
    });
    assert.deepEqual(parseNotifyOutcome("delivered"), {
      status: "unavailable",
      reason: "bad-outcome",
    });
  });
});

describe("describeNotifyOutcome", () => {
  it("confirms delivery inline for the Test button, silently for finishes", () => {
    assert.deepEqual(
      describeNotifyOutcome({ status: "delivered" }, "test"),
      { ok: true, message: "System notification sent." },
    );
    assert.deepEqual(describeNotifyOutcome({ status: "delivered" }, "agent"), {
      ok: true,
      message: null,
    });
  });

  it("marks legacy attempts ok with a packaged-app pointer for Test", () => {
    const report = describeNotifyOutcome({ status: "attempted" }, "test");
    assert.equal(report.ok, true);
    assert.match(report.message ?? "", /packaged app/);
    assert.deepEqual(describeNotifyOutcome({ status: "attempted" }, "agent"), {
      ok: true,
      message: null,
    });
  });

  it("points denials at System Settings in both contexts", () => {
    for (const context of ["test", "agent"] as const) {
      const report = describeNotifyOutcome({ status: "denied" }, context);
      assert.equal(report.ok, false);
      assert.match(report.message ?? "", /System Settings/);
    }
  });

  it("routes undecided permission to the Test button", () => {
    const test = describeNotifyOutcome(
      { status: "unavailable", reason: "not-determined" },
      "test",
    );
    assert.equal(test.ok, false);
    assert.match(test.message ?? "", /press Test again/);
    const agent = describeNotifyOutcome(
      { status: "unavailable", reason: "not-determined" },
      "agent",
    );
    assert.equal(agent.ok, false);
    assert.match(agent.message ?? "", /Settings → Alerts/);
  });

  it("keeps transient failures inline for Test but silent for finishes", () => {
    const test = describeNotifyOutcome(
      { status: "unavailable", reason: "delivery-timed-out" },
      "test",
    );
    assert.equal(test.ok, false);
    assert.match(test.message ?? "", /delivery-timed-out/);
    assert.deepEqual(
      describeNotifyOutcome(
        { status: "unavailable", reason: "delivery-timed-out" },
        "agent",
      ),
      { ok: false, message: null },
    );
  });
});

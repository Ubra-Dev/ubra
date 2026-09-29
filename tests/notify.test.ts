import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  chimeStyleParam,
  parseChimeStyle,
  parseDelivery,
  parseToastPosition,
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

describe("chime styles", () => {
  it("parses persisted selections, falling back to default", () => {
    assert.equal(parseChimeStyle("bright"), "bright");
    assert.equal(parseChimeStyle("custom"), "custom");
    assert.equal(parseChimeStyle("nope"), "default");
    assert.equal(parseChimeStyle(null), "default");
    assert.equal(parseChimeStyle(undefined), "default");
  });

  it("maps selections to backend params without ever sending custom", () => {
    assert.equal(chimeStyleParam("soft"), "soft");
    assert.equal(chimeStyleParam("custom"), null);
    assert.equal(chimeStyleParam("nope"), "default");
  });
});

describe("testNotificationPayload", () => {
  it("marks itself as a test with a safe empty jump target", () => {
    const t = testNotificationPayload();
    assert.match(t.title, /\(test\)/);
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

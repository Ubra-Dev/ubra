import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  parseDelivery,
  parseToastPosition,
  routeNotification,
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

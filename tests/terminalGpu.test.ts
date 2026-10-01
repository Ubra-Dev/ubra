import { describe, it } from "node:test";
import assert from "node:assert/strict";
import type { WebglAddon } from "@xterm/addon-webgl";
import type { Terminal } from "@xterm/xterm";
import {
  defaultWebGl2Probe,
  disableGpuRenderer,
  enableGpuRenderer,
} from "../src/lib/terminalGpu.ts";

function fakeTerm(impl: { loadAddon?: (addon: unknown) => void } = {}): {
  term: Terminal;
  loaded: unknown[];
} {
  const loaded: unknown[] = [];
  return {
    loaded,
    term: {
      loadAddon: (addon: unknown) => {
        loaded.push(addon);
        impl.loadAddon?.(addon);
      },
    } as unknown as Terminal,
  };
}

function fakeAddon(): {
  addon: WebglAddon;
  disposed: () => number;
  fireContextLoss: () => void;
} {
  let disposes = 0;
  let handler: (() => void) | null = null;
  return {
    disposed: () => disposes,
    fireContextLoss: () => handler?.(),
    addon: {
      dispose: () => {
        disposes += 1;
      },
      onContextLoss: (listener: () => void) => {
        handler = listener;
      },
    } as unknown as WebglAddon,
  };
}

/** Failure paths log to console.error; keep test output clean. */
function silenceConsole<T>(fn: () => T): T {
  const original = console.error;
  console.error = () => {};
  try {
    return fn();
  } finally {
    console.error = original;
  }
}

describe("defaultWebGl2Probe", () => {
  it("returns false without a DOM", () => {
    assert.equal(typeof document, "undefined");
    assert.equal(defaultWebGl2Probe(), false);
  });
});

describe("enableGpuRenderer", () => {
  it("returns null without touching the terminal when the probe fails", () => {
    const { term, loaded } = fakeTerm();
    const result = enableGpuRenderer(term, {
      probe: () => false,
      createAddon: () => {
        throw new Error("must not construct");
      },
    });
    assert.equal(result, null);
    assert.deepEqual(loaded, []);
  });

  it("loads the addon and returns it when the probe passes", () => {
    const { term, loaded } = fakeTerm();
    const { addon } = fakeAddon();
    const result = enableGpuRenderer(term, {
      probe: () => true,
      createAddon: () => addon,
    });
    assert.equal(result, addon);
    assert.deepEqual(loaded, [addon]);
  });

  it("disposes the addon on context loss", () => {
    const { term } = fakeTerm();
    const { addon, disposed, fireContextLoss } = fakeAddon();
    enableGpuRenderer(term, {
      probe: () => true,
      createAddon: () => addon,
    });
    assert.equal(disposed(), 0);
    fireContextLoss();
    assert.equal(disposed(), 1);
  });

  it("returns null when construction throws", () => {
    const { term, loaded } = fakeTerm();
    const result = silenceConsole(() =>
      enableGpuRenderer(term, {
        probe: () => true,
        createAddon: () => {
          throw new Error("no WebGL2");
        },
      }),
    );
    assert.equal(result, null);
    assert.deepEqual(loaded, []);
  });

  it("returns null and disposes when loading throws", () => {
    const { term } = fakeTerm({
      loadAddon: () => {
        throw new Error("activation failed");
      },
    });
    const { addon, disposed } = fakeAddon();
    const result = silenceConsole(() =>
      enableGpuRenderer(term, {
        probe: () => true,
        createAddon: () => addon,
      }),
    );
    assert.equal(result, null);
    assert.equal(disposed(), 1);
  });
});

describe("disableGpuRenderer", () => {
  it("returns null for a null addon", () => {
    assert.equal(disableGpuRenderer(null), null);
  });

  it("disposes and returns null", () => {
    const { addon, disposed } = fakeAddon();
    assert.equal(disableGpuRenderer(addon), null);
    assert.equal(disposed(), 1);
  });

  it("returns null even when dispose throws", () => {
    const addon = {
      dispose: () => {
        throw new Error("already gone");
      },
    } as unknown as WebglAddon;
    assert.equal(silenceConsole(() => disableGpuRenderer(addon)), null);
  });
});

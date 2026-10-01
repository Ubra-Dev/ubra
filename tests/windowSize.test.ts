import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const config = JSON.parse(
  readFileSync(join(here, "..", "src-tauri", "tauri.conf.json"), "utf8"),
);
const main = config.app.windows.find(
  (w: { label: string }) => w.label === "main",
);

describe("main window default size", () => {
  it("exists", () => {
    assert.ok(main, "tauri.conf.json must define a main window");
  });

  it("opens at the fixed 1440x900 default", () => {
    assert.equal(main.width, 1440);
    assert.equal(main.height, 900);
  });

  it("stays resizable with a usable minimum and centered placement", () => {
    assert.equal(main.resizable, true);
    assert.equal(main.minWidth, 1024);
    assert.equal(main.minHeight, 640);
    assert.equal(main.center, true);
  });
});

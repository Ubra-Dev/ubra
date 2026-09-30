import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * The file explorer opens user-chosen files via `openPath`, which denies
 * every path unless `opener:allow-open-path` carries a path scope.
 * Guards against regressing to the bare (scopeless) permission string.
 */
describe("opener capability scope", () => {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const capability = JSON.parse(
    readFileSync(join(root, "src-tauri/capabilities/default.json"), "utf8"),
  ) as { permissions: unknown[] };

  it("grants open-path a non-empty path scope", () => {
    const entry = capability.permissions.find(
      (p): p is { identifier: string; allow: { path?: string }[] } =>
        typeof p === "object" &&
        p !== null &&
        (p as { identifier?: string }).identifier ===
          "opener:allow-open-path",
    );
    assert.ok(entry, "opener:allow-open-path permission missing");
    assert.ok(
      entry.allow?.some(
        (rule) => typeof rule.path === "string" && rule.path.length > 0,
      ),
      "opener:allow-open-path has no path scope",
    );
  });
});

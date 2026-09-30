import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { licenseFiles, noticeSection } from "../scripts/collect-licenses.mjs";
import { prepareRelease } from "../scripts/release-checksums.mjs";

function fixture(t) {
  const path = mkdtempSync(join(tmpdir(), "ubra-release-test-"));
  t.after(() => rmSync(path, { recursive: true, force: true }));
  return path;
}

test("collects nested copyright and AUTHORS notices without importing nested dependencies", t => {
  const path = fixture(t);
  mkdirSync(join(path, "upstream"));
  mkdirSync(join(path, "node_modules", "nested"), { recursive: true });
  writeFileSync(join(path, "AUTHORS"), "Copyright Upstream Authors");
  writeFileSync(join(path, "upstream", "LICENSE-MIT"), "Upstream MIT license");
  writeFileSync(join(path, "node_modules", "nested", "LICENSE"), "A different dependency");
  assert.deepEqual(licenseFiles(path), [join(path, "AUTHORS"), join(path, "upstream", "LICENSE-MIT")].sort());
});

test("missing or empty notices block generation; supplied copyright survives verbatim", () => {
  const component = { ecosystem: "npm", name: "example", version: "1.0", license: "MIT" };
  assert.throws(() => noticeSection(component, []), /Missing license/);
  assert.throws(() => noticeSection(component, [{ source: "LICENSE", text: " " }]), /Missing license/);
  const text = "Copyright (c) Example Authors\nPermission granted.\n";
  assert.ok(noticeSection(component, [{ source: "LICENSE", text }]).includes(text));
});

function releaseFixture(t) {
  const path = fixture(t);
  const input = join(path, "input");
  const output = join(path, "output");
  mkdirSync(join(input, "legal"), { recursive: true });
  mkdirSync(join(input, "macos", "Ubra.app"), { recursive: true });
  mkdirSync(join(input, "linux", "Ubra.AppDir", "legal"), { recursive: true });
  writeFileSync(join(input, "macos", "Ubra.dmg"), "installer bytes");
  writeFileSync(join(input, "macos", "Ubra.app", "private-file"), "not a download");
  writeFileSync(join(input, "linux", "Ubra.AppDir", "legal", "LICENSE"), "bundled duplicate notice");
  for (const name of ["LICENSE", "THIRD_PARTY_NOTICES.txt", "dependency-inventory.json", "THIRD_PARTY_SOURCES.tar.gz"]) {
    writeFileSync(join(input, "legal", name), `content of ${name}`);
  }
  return { input, output };
}

test("published checksums match copied bytes and include notice/source assets", t => {
  const { input, output } = releaseFixture(t);
  const lines = prepareRelease(input, output);
  assert.equal(lines.length, 5);
  for (const line of lines) {
    const [hash, filename] = line.split("  ");
    assert.equal(hash, createHash("sha256").update(readFileSync(join(output, filename))).digest("hex"));
  }
  assert.equal(readFileSync(join(output, "SHA256SUMS"), "utf8"), lines.join("\n") + "\n");
});

test("duplicate installer filenames and omitted legal assets block release preparation", t => {
  const { input, output } = releaseFixture(t);
  mkdirSync(join(input, "duplicate"));
  writeFileSync(join(input, "duplicate", "Ubra.dmg"), "different bytes");
  assert.throws(() => prepareRelease(input, output), /Duplicate release filename/);
  rmSync(join(input, "duplicate"), { recursive: true });
  rmSync(join(input, "legal", "THIRD_PARTY_SOURCES.tar.gz"));
  assert.throws(() => prepareRelease(input, output), /Missing release notice\/source asset/);
});

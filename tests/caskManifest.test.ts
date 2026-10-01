import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  assertArchPair,
  buildCaskRb,
  classifyDmgArch,
  type CaskInput,
} from "../src/lib/caskManifest.ts";

const here = dirname(fileURLToPath(import.meta.url));
const tauriConf = JSON.parse(
  readFileSync(join(here, "..", "src-tauri", "tauri.conf.json"), "utf8"),
);
const packageJson = JSON.parse(
  readFileSync(join(here, "..", "package.json"), "utf8"),
);

function input(): CaskInput {
  return {
    version: "0.1.0",
    tag: "v0.1.0",
    repo: "Ubra-Dev/ubra",
    productName: "Ubra",
    identifier: "com.ubra.app",
    description: "Agent runtime desktop app",
    homepage: "https://getubra.com/",
    launchAgentName: "Ubra",
    arm: { file: "Ubra_0.1.0_aarch64.dmg", sha256: "a".repeat(64) },
    intel: { file: "Ubra_0.1.0_x64.dmg", sha256: "b".repeat(64) },
  };
}

describe("classifyDmgArch", () => {
  it("maps Tauri arch tokens to cask arches", () => {
    assert.equal(classifyDmgArch("Ubra_0.1.0_aarch64.dmg"), "arm");
    assert.equal(classifyDmgArch("Ubra_0.1.0_arm64.dmg"), "arm");
    assert.equal(classifyDmgArch("Ubra_0.1.0_x86_64.dmg"), "intel");
    assert.equal(classifyDmgArch("Ubra_0.1.0_x64.dmg"), "intel");
  });

  it("throws on unrecognized names instead of guessing", () => {
    assert.throws(() => classifyDmgArch("Ubra_0.1.0.dmg"), /arch token/);
    assert.throws(() => classifyDmgArch("Ubra_0.1.0_universal.dmg"), /arch token/);
    assert.throws(() => classifyDmgArch("notes.txt"), /arch token/);
  });
});

describe("assertArchPair", () => {
  it("accepts exactly one arm + one intel DMG", () => {
    assertArchPair([
      { arch: "intel", file: "i.dmg", sha256: "x" },
      { arch: "arm", file: "a.dmg", sha256: "y" },
    ]);
  });

  it("rejects missing, duplicate, or extra DMGs", () => {
    assert.throws(() => assertArchPair([]), /one arm \+ one intel/);
    assert.throws(
      () => assertArchPair([{ arch: "arm", file: "a.dmg", sha256: "y" }]),
      /one arm \+ one intel/,
    );
    assert.throws(
      () =>
        assertArchPair([
          { arch: "arm", file: "a.dmg", sha256: "y" },
          { arch: "arm", file: "a2.dmg", sha256: "z" },
        ]),
      /one arm \+ one intel/,
    );
  });
});

describe("buildCaskRb", () => {
  it("renders per-arch urls with shas and app metadata", () => {
    const cask = buildCaskRb(input());
    assert.match(cask, /cask "ubra" do/);
    assert.match(cask, /version "0\.1\.0"/);
    assert.match(
      cask,
      new RegExp(
        `on_arm do\\n    sha256 "${"a".repeat(64)}"\\n    url "https://github.com/Ubra-Dev/ubra/releases/download/v0\\.1\\.0/Ubra_0\\.1\\.0_aarch64\\.dmg"`,
      ),
    );
    assert.match(
      cask,
      new RegExp(
        `on_intel do\\n    sha256 "${"b".repeat(64)}"\\n    url "https://github.com/Ubra-Dev/ubra/releases/download/v0\\.1\\.0/Ubra_0\\.1\\.0_x64\\.dmg"`,
      ),
    );
    assert.match(cask, /auto_updates true/);
    assert.match(cask, /depends_on macos: ">= :catalina"/);
    assert.match(cask, /app "Ubra\.app"/);
    assert.match(cask, /quit:      "com\.ubra\.app"/);
    assert.match(cask, /launchctl: "Ubra"/);
    assert.match(cask, /~\/Library\/LaunchAgents\/Ubra\.plist/);
  });

  it("tracks the repo's real product metadata (drift guard)", () => {
    const rendered = input();
    assert.equal(rendered.productName, tauriConf.productName);
    assert.equal(rendered.identifier, tauriConf.identifier);
    assert.equal(rendered.description, packageJson.description);
    assert.equal(rendered.homepage, packageJson.homepage + "/");
    assert.equal(tauriConf.bundle.macOS.minimumSystemVersion, "10.15");
  });
});

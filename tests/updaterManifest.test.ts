import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  assertCompletePlatforms,
  buildLatestJson,
  collectUpdaterBundles,
  REQUIRED_PLATFORMS,
} from "../src/lib/updaterManifest.ts";

const SIG = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ\n";

function reader(files: Record<string, string>): (name: string) => string {
  return (name) => {
    const content = files[name];
    if (content === undefined) throw new Error(`missing file: ${name}`);
    return content;
  };
}

describe("collectUpdaterBundles", () => {
  it("maps a universal macOS bundle to both darwin keys", () => {
    const bundles = collectUpdaterBundles(
      ["Ubra_0.1.0_universal.app.tar.gz", "Ubra_0.1.0_universal.app.tar.gz.sig"],
      reader({ "Ubra_0.1.0_universal.app.tar.gz.sig": SIG }),
      (name) => `https://example.com/${name}`,
    );
    assert.deepEqual(
      bundles.map((b) => b.platform).sort(),
      ["darwin-aarch64", "darwin-x86_64"],
    );
    assert.equal(
      bundles[0].url,
      "https://example.com/Ubra_0.1.0_universal.app.tar.gz",
    );
    assert.equal(bundles[0].signature, SIG.trim());
  });

  it("maps arch-specific macOS bundles individually", () => {
    const bundles = collectUpdaterBundles(
      ["Ubra_0.1.0_aarch64.app.tar.gz", "Ubra_0.1.0_aarch64.app.tar.gz.sig"],
      reader({ "Ubra_0.1.0_aarch64.app.tar.gz.sig": SIG }),
      (name) => `https://example.com/${name}`,
    );
    assert.deepEqual(
      bundles.map((b) => b.platform),
      ["darwin-aarch64"],
    );
  });

  it("maps linux and windows updater bundles", () => {
    const bundles = collectUpdaterBundles(
      [
        "Ubra_0.1.0_amd64.AppImage.tar.gz",
        "Ubra_0.1.0_amd64.AppImage.tar.gz.sig",
        "Ubra_0.1.0_x64-setup.nsis.zip",
        "Ubra_0.1.0_x64-setup.nsis.zip.sig",
      ],
      reader({
        "Ubra_0.1.0_amd64.AppImage.tar.gz.sig": SIG,
        "Ubra_0.1.0_x64-setup.nsis.zip.sig": SIG,
      }),
      (name) => `https://example.com/${name}`,
    );
    assert.deepEqual(
      bundles.map((b) => b.platform).sort(),
      ["linux-x86_64", "windows-x86_64"],
    );
  });

  it("prefers nsis over msi when both windows bundles exist", () => {
    const bundles = collectUpdaterBundles(
      [
        "Ubra_0.1.0_x64-setup.nsis.zip",
        "Ubra_0.1.0_x64-setup.nsis.zip.sig",
        "Ubra_0.1.0_x64_en-US.msi.zip",
        "Ubra_0.1.0_x64_en-US.msi.zip.sig",
      ],
      reader({
        "Ubra_0.1.0_x64-setup.nsis.zip.sig": SIG,
        "Ubra_0.1.0_x64_en-US.msi.zip.sig": SIG,
      }),
      (name) => `https://example.com/${name}`,
    );
    assert.equal(bundles.length, 1);
    assert.equal(bundles[0].platform, "windows-x86_64");
    assert.match(bundles[0].url, /\.nsis\.zip$/);
  });

  it("falls back to msi when nsis is absent", () => {
    const bundles = collectUpdaterBundles(
      ["Ubra_0.1.0_x64_en-US.msi.zip", "Ubra_0.1.0_x64_en-US.msi.zip.sig"],
      reader({ "Ubra_0.1.0_x64_en-US.msi.zip.sig": SIG }),
      (name) => `https://example.com/${name}`,
    );
    assert.equal(bundles.length, 1);
    assert.match(bundles[0].url, /\.msi\.zip$/);
  });

  it("ignores non-updater artifacts", () => {
    const bundles = collectUpdaterBundles(
      ["Ubra_0.1.0_universal.dmg", "Ubra_0.1.0_amd64.deb", "latest.json"],
      reader({}),
      (name) => `https://example.com/${name}`,
    );
    assert.deepEqual(bundles, []);
  });

  it("throws on duplicate updater bundle names", () => {
    assert.throws(
      () =>
        collectUpdaterBundles(
          [
            "Ubra_0.1.0_aarch64.app.tar.gz",
            "Ubra_0.1.0_aarch64.app.tar.gz.sig",
            "Ubra_0.1.0_aarch64.app.tar.gz",
          ],
          reader({ "Ubra_0.1.0_aarch64.app.tar.gz.sig": SIG }),
          (name) => `https://example.com/${name}`,
        ),
      /duplicate updater bundle name/,
    );
  });

  it("ignores duplicate unrelated files", () => {
    const bundles = collectUpdaterBundles(
      ["Info.plist", "Info.plist"],
      reader({}),
      (name) => `https://example.com/${name}`,
    );
    assert.deepEqual(bundles, []);
  });

  it("throws when a bundle has no signature", () => {
    assert.throws(
      () =>
        collectUpdaterBundles(
          ["Ubra_0.1.0_universal.app.tar.gz"],
          reader({}),
          (name) => `https://example.com/${name}`,
        ),
      /signature/,
    );
  });
});

describe("assertCompletePlatforms", () => {
  it("passes when every required platform is present", () => {
    assertCompletePlatforms(
      REQUIRED_PLATFORMS.map((platform) => ({
        platform,
        url: "https://example.com/bundle",
        signature: "sig",
      })),
    );
  });

  it("throws naming the missing platforms", () => {
    assert.throws(
      () =>
        assertCompletePlatforms([
          {
            platform: "darwin-aarch64",
            url: "https://example.com/bundle",
            signature: "sig",
          },
        ]),
      /darwin-x86_64/,
    );
  });
});

describe("buildLatestJson", () => {
  it("emits the static updater format", () => {
    const json = buildLatestJson({
      version: "0.1.0",
      pubDate: "2026-09-30T00:00:00.000Z",
      notes: "Bug fixes",
      bundles: [
        {
          platform: "darwin-aarch64",
          url: "https://example.com/Ubra.tar.gz",
          signature: "sig",
        },
      ],
    });
    assert.deepEqual(JSON.parse(json), {
      version: "0.1.0",
      pub_date: "2026-09-30T00:00:00.000Z",
      notes: "Bug fixes",
      platforms: {
        "darwin-aarch64": { url: "https://example.com/Ubra.tar.gz", signature: "sig" },
      },
    });
  });

  it("omits notes when not provided", () => {
    const json = buildLatestJson({
      version: "0.1.0",
      pubDate: "2026-09-30T00:00:00.000Z",
      bundles: [],
    });
    assert.ok(!("notes" in JSON.parse(json)));
  });
});

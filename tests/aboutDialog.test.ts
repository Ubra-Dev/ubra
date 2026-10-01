import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  ABOUT_BLURB,
  ABOUT_COPYRIGHT,
  ABOUT_LICENSE,
  ABOUT_WEBSITE,
  aboutMetadata,
} from "../src/lib/aboutDialog.ts";

describe("aboutMetadata", () => {
  it("fills the professional dialog fields from name and version", () => {
    assert.deepEqual(aboutMetadata("Ubra", "0.1.0"), {
      name: "Ubra",
      version: "0.1.0",
      copyright: ABOUT_COPYRIGHT,
      credits: ABOUT_BLURB,
      comments: ABOUT_BLURB,
      license: ABOUT_LICENSE,
      website: ABOUT_WEBSITE,
      websiteLabel: "GitHub",
    });
  });

  it("omits the icon key when no logo is available", () => {
    assert.ok(!("icon" in aboutMetadata("Ubra", "0.1.0", undefined)));
  });

  it("attaches the brand logo when provided", () => {
    const icon = { rid: 7, kind: "Image" };
    const meta = aboutMetadata(
      "Ubra",
      "0.1.0",
      icon as never,
    );
    assert.equal(meta.icon, icon);
  });
});

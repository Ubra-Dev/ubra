import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_UI_FONT_ID,
  UI_FONTS,
  UI_FONT_IDS,
  isUiFontId,
  matchUiFonts,
  uiFontStyle,
} from "../src/lib/uiFonts.ts";

describe("isUiFontId", () => {
  it("accepts catalog ids and rejects the rest", () => {
    assert.equal(isUiFontId("system"), true);
    assert.equal(isUiFontId("silkscreen"), true);
    assert.equal(isUiFontId("inter"), true);
    assert.equal(isUiFontId("Comic Sans"), false);
    assert.equal(isUiFontId(""), false);
    assert.equal(isUiFontId(null), false);
    assert.equal(isUiFontId(42), false);
  });
});

describe("uiFontStyle", () => {
  it("emits the system stack for the default", () => {
    assert.equal(
      uiFontStyle(DEFAULT_UI_FONT_ID),
      "--font-ui:system-ui, sans-serif",
    );
  });

  it("emits the bundled variable family first", () => {
    assert.equal(
      uiFontStyle("inter"),
      "--font-ui:'Inter Variable', system-ui, sans-serif",
    );
  });

  it("emits the static Silkscreen family first", () => {
    assert.equal(
      uiFontStyle("silkscreen"),
      "--font-ui:'Silkscreen', system-ui, sans-serif",
    );
  });
});

describe("matchUiFonts", () => {
  it("returns the whole catalog for a blank query", () => {
    assert.deepEqual(matchUiFonts("  "), [...UI_FONT_IDS]);
  });

  it("matches names case-insensitively", () => {
    assert.deepEqual(matchUiFonts("INTER"), ["inter"]);
    assert.deepEqual(matchUiFonts("sans"), [
      "dm-sans",
      "ibm-plex-sans",
      "open-sans",
      "plus-jakarta-sans",
      "public-sans",
      "source-sans-3",
      "work-sans",
    ]);
  });

  it("matches categories", () => {
    assert.deepEqual(matchUiFonts("pixel"), ["silkscreen"]);
    assert.deepEqual(matchUiFonts("rounded"), ["nunito"]);
    assert.deepEqual(matchUiFonts("humanist"), [
      "open-sans",
      "public-sans",
      "source-sans-3",
    ]);
  });

  it("returns an empty list when nothing matches", () => {
    assert.deepEqual(matchUiFonts("zzz-nope"), []);
  });
});

describe("catalog", () => {
  it("keeps system first as the default", () => {
    assert.equal(DEFAULT_UI_FONT_ID, "system");
    assert.equal(UI_FONT_IDS[0], "system");
  });

  it("lists Silkscreen right after the system default", () => {
    assert.equal(UI_FONT_IDS[1], "silkscreen");
    assert.equal(UI_FONTS.silkscreen.name, "Silkscreen");
  });

  it("falls every stack back to system fonts", () => {
    for (const id of UI_FONT_IDS) {
      assert.match(UI_FONTS[id].stack, /system-ui, sans-serif$/);
    }
  });
});

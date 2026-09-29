import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  COPY_TOAST_DISMISS_MS,
  SELECTION_DEBOUNCE_MS,
  copyTextToClipboard,
  shouldAutoCopy,
  truncatePreview,
} from "../src/lib/clipboard.ts";

describe("shouldAutoCopy", () => {
  it("copies non-blank selections", () => {
    assert.equal(shouldAutoCopy("hello", ""), true);
  });

  it("skips empty and blank selections", () => {
    assert.equal(shouldAutoCopy("", ""), false);
    assert.equal(shouldAutoCopy("   \n\t  ", ""), false);
  });

  it("skips repeats of the last copied text", () => {
    assert.equal(shouldAutoCopy("hello", "hello"), false);
    assert.equal(shouldAutoCopy("hello world", "hello"), true);
  });
});

describe("truncatePreview", () => {
  it("returns short selections unchanged", () => {
    assert.equal(truncatePreview("ls -la"), "ls -la");
  });

  it("flattens whitespace to a single line", () => {
    assert.equal(truncatePreview("foo\n  bar\tbaz"), "foo bar baz");
  });

  it("truncates long selections with an ellipsis", () => {
    const preview = truncatePreview("x".repeat(200), 80);
    assert.equal(preview.length, 80);
    assert.ok(preview.endsWith("…"));
  });
});

describe("copy timing", () => {
  it("uses a short toast and a sub-second debounce", () => {
    assert.ok(COPY_TOAST_DISMISS_MS <= 3000);
    assert.ok(SELECTION_DEBOUNCE_MS > 0 && SELECTION_DEBOUNCE_MS < 1000);
  });
});

describe("copyTextToClipboard", () => {
  it("resolves false for empty text without touching the clipboard", async () => {
    assert.equal(await copyTextToClipboard(""), false);
  });
});

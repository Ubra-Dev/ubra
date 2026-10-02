import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { formatNoteDate, newNoteName, slugify, titleFromContent } from "../src/lib/notes.ts";

describe("slugify", () => {
  it("lowercases and dashes titles", () => {
    assert.equal(slugify("My Great Idea!"), "my-great-idea");
    assert.equal(slugify("  spaced   out  "), "spaced-out");
    assert.equal(slugify("dots.and_underscores"), "dots-and-underscores");
  });

  it("falls back to untitled for empty or symbol-only titles", () => {
    assert.equal(slugify(""), "untitled");
    assert.equal(slugify("!!!"), "untitled");
    assert.equal(slugify("---"), "untitled");
  });

  it("caps length and always satisfies backend name rules", () => {
    const slug = slugify("a".repeat(200));
    assert.equal(slug.length, 40);
    for (const title of ["Hello World", "café ☕ notes", "../escape", ".hidden", "a/b"]) {
      const out = slugify(title);
      assert.match(out, /^[a-z0-9][a-z0-9-]*$/);
      assert.ok(!out.includes(".."));
    }
  });
});

describe("newNoteName", () => {
  it("suffixes the slug with a random hex id", () => {
    const name = newNoteName("Shopping", () => false);
    assert.match(name, /^shopping-[0-9a-f]{6}$/);
  });

  it("regenerates while the name is taken", () => {
    const taken = new Set<string>();
    const first = newNoteName("Todo", (n) => taken.has(n));
    taken.add(first);
    const second = newNoteName("Todo", (n) => taken.has(n));
    assert.notEqual(second, first);
    assert.match(second, /^todo-[0-9a-f]{6}$/);
  });
});

describe("titleFromContent", () => {
  it("prefers the first h1 heading", () => {
    assert.equal(titleFromContent("# Hello\nbody\n", "n"), "Hello");
    assert.equal(titleFromContent("## Sub\n# Top\n", "n"), "Top");
  });

  it("falls back to the first non-empty line, then the file name", () => {
    assert.equal(titleFromContent("## Sub\nbody\n", "n"), "Sub");
    assert.equal(titleFromContent("\n\nplain line\n", "n"), "plain line");
    assert.equal(titleFromContent("\n\n", "fallback"), "fallback");
    assert.equal(titleFromContent("", "fallback"), "fallback");
  });
});

describe("formatNoteDate", () => {
  // Fixed Monday noon local time so Yesterday/date branches are stable.
  const now = new Date(2026, 9, 5, 12, 0, 0).getTime();

  it("renders relative times under a day", () => {
    assert.equal(formatNoteDate(null, now), "");
    assert.equal(formatNoteDate(now, now), "just now");
    assert.equal(formatNoteDate(now + 1000, now), "just now");
    assert.equal(formatNoteDate(now - 30 * 1000, now), "just now");
    assert.equal(formatNoteDate(now - 5 * 60 * 1000, now), "5m ago");
    assert.equal(formatNoteDate(now - 3 * 60 * 60 * 1000, now), "3h ago");
  });

  it("renders yesterday and calendar dates beyond a day", () => {
    assert.equal(formatNoteDate(now - 26 * 60 * 60 * 1000, now), "Yesterday");
    assert.equal(formatNoteDate(new Date(2026, 9, 1, 9).getTime(), now), "1 Oct");
    assert.equal(formatNoteDate(new Date(2025, 4, 9, 9).getTime(), now), "9 May 2025");
  });
});

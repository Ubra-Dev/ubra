import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { highlightCode, languageForFile } from "../src/lib/highlight.ts";

describe("languageForFile", () => {
  it("maps common extensions", () => {
    assert.equal(languageForFile("main.rs"), "rust");
    assert.equal(languageForFile("app.tsx"), "typescript");
    assert.equal(languageForFile("bundle.min.js"), "javascript");
    assert.equal(languageForFile("data.json"), "json");
    assert.equal(languageForFile("run.py"), "python");
    assert.equal(languageForFile("page.html"), "xml");
    assert.equal(languageForFile("notes.md"), "markdown");
    assert.equal(languageForFile("config.yaml"), "yaml");
    assert.equal(languageForFile("run.sh"), "bash");
    assert.equal(languageForFile("schema.sql"), "sql");
    assert.equal(languageForFile("change.diff"), "diff");
  });

  it("maps non-core extensions to the closest grammar", () => {
    assert.equal(languageForFile("Cargo.toml"), "ini");
    assert.equal(languageForFile("App.svelte"), "xml");
    assert.equal(languageForFile("styles.less"), "scss");
  });

  it("maps exact file names and ignores directories and case", () => {
    assert.equal(languageForFile("Dockerfile"), "dockerfile");
    assert.equal(languageForFile("dockerfile"), "dockerfile");
    assert.equal(languageForFile("Makefile"), "makefile");
    assert.equal(languageForFile("src/lib/MAIN.RS"), "rust");
    assert.equal(languageForFile("nested/dir/Gemfile"), "ruby");
  });

  it("returns null for unknown and extensionless names", () => {
    assert.equal(languageForFile("LICENSE"), null);
    assert.equal(languageForFile("data.xyz"), null);
    assert.equal(languageForFile(".gitignore"), null);
    assert.equal(languageForFile(""), null);
  });
});

describe("highlightCode", () => {
  it("highlights with the mapped language", () => {
    const result = highlightCode("fn main() {\n  let x = 1;\n}\n", "main.rs");
    assert.equal(result.language, "rust");
    assert.match(result.html, /hljs-keyword/);
    assert.doesNotMatch(result.html, /<script/);
  });

  it("returns empty HTML for empty source", () => {
    assert.deepEqual(highlightCode("", "main.rs"), { html: "", language: "plaintext" });
  });

  it("never returns unescaped source", () => {
    const evil = "<script>alert('x')</script>";
    for (const name of ["a.rs", "notes.txt", "LICENSE", "data.xyz"]) {
      const result = highlightCode(evil, name);
      assert.doesNotMatch(result.html, /<script/);
    }
  });

  it("falls back to plaintext instead of throwing", () => {
    const result = highlightCode("\x00\x01binary?", "blob.bin");
    assert.equal(typeof result.html, "string");
    assert.doesNotMatch(result.html, /<script/);
  });
});

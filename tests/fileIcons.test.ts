import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { fileIconFor } from "../src/lib/fileIcons.ts";

describe("fileIconFor", () => {
  it("maps code files to the code glyph with per-language colors", () => {
    assert.deepEqual(fileIconFor("app.js", false), { name: "code", color: "#cbcb41" });
    assert.deepEqual(fileIconFor("app.ts", false), { name: "code", color: "#519aba" });
    assert.deepEqual(fileIconFor("App.SVELTE", false), { name: "code", color: "#ff3e00" });
    assert.deepEqual(fileIconFor("main.py", false), { name: "code", color: "#4b8bbe" });
    assert.deepEqual(fileIconFor("types.d.ts", false), { name: "code", color: "#519aba" });
  });

  it("matches exact filenames before extensions", () => {
    assert.deepEqual(fileIconFor("package.json", false), { name: "package", color: "#cc342d" });
    assert.deepEqual(fileIconFor("tsconfig.json", false), { name: "code", color: "#519aba" });
    assert.deepEqual(fileIconFor("data.json", false), { name: "code", color: "#cbcb41" });
    assert.deepEqual(fileIconFor("Dockerfile", false), { name: "package", color: "#519aba" });
    assert.deepEqual(fileIconFor("dockerfile.dev", false), { name: "package", color: "#519aba" });
    assert.deepEqual(fileIconFor("Makefile", false), { name: "tool", color: "#8b949e" });
    assert.deepEqual(fileIconFor("README.md", false), { name: "book", color: "#519aba" });
    assert.deepEqual(fileIconFor("LICENSE-MIT", false), { name: "book", color: "#cbcb41" });
    assert.deepEqual(fileIconFor("vite.config.ts", false), { name: "settings", color: "#8b949e" });
  });

  it("treats leading-dot names without another dot as extensionless", () => {
    assert.deepEqual(fileIconFor(".gitignore", false), { name: "file", color: "#e37933" });
    assert.deepEqual(fileIconFor(".env.local", false), { name: "key", color: "#cbcb41" });
    assert.deepEqual(fileIconFor(".nvmrc", false), { name: "file", color: "#8b949e" });
  });

  it("maps media, archives, and data files to distinct glyphs", () => {
    assert.deepEqual(fileIconFor("logo.svg", false), { name: "image", color: "#cbcb41" });
    assert.deepEqual(fileIconFor("photo.PNG", false), { name: "image", color: "#a074c4" });
    assert.deepEqual(fileIconFor("demo.mp4", false), { name: "video", color: "#a074c4" });
    assert.deepEqual(fileIconFor("track.mp3", false), { name: "music", color: "#a074c4" });
    assert.deepEqual(fileIconFor("backup.tar", false), { name: "archive", color: "#8b949e" });
    assert.deepEqual(fileIconFor("dump.sql", false), { name: "code", color: "#cbcb41" });
    assert.deepEqual(fileIconFor("app.db", false), { name: "database", color: "#8b949e" });
    assert.deepEqual(fileIconFor("run.sh", false), { name: "terminal", color: "#5c9e46" });
    assert.deepEqual(fileIconFor("yarn.lock", false), { name: "lock", color: "#8b949e" });
    assert.deepEqual(fileIconFor("ci.yml", false), { name: "file", color: "#cc342d" });
    assert.deepEqual(fileIconFor("manual.pdf", false), { name: "file-text", color: "#cc342d" });
    assert.deepEqual(fileIconFor("notes.md", false), { name: "file-text", color: "#519aba" });
    assert.deepEqual(fileIconFor("index.html", false), { name: "globe", color: "#e37933" });
  });

  it("falls back to the default file icon", () => {
    assert.deepEqual(fileIconFor("weird.unknownext", false), { name: "file", color: "#8b949e" });
    assert.deepEqual(fileIconFor("noextension", false), { name: "file", color: "#8b949e" });
    assert.deepEqual(fileIconFor("trailing.", false), { name: "file", color: "#8b949e" });
  });

  it("colors well-known folders and defaults the rest", () => {
    assert.deepEqual(fileIconFor("src", true), { name: "folder", color: "#519aba" });
    assert.deepEqual(fileIconFor("Tests", true), { name: "folder", color: "#5c9e46" });
    assert.deepEqual(fileIconFor("docs", true), { name: "folder", color: "#cbcb41" });
    assert.deepEqual(fileIconFor("public", true), { name: "folder", color: "#a074c4" });
    assert.deepEqual(fileIconFor("scripts", true), { name: "folder", color: "#e37933" });
    assert.deepEqual(fileIconFor("node_modules", true), { name: "folder", color: "#5c636e" });
    assert.deepEqual(fileIconFor("my-feature", true), { name: "folder", color: "#8b949e" });
  });

  it("ignores file rules for directories", () => {
    assert.deepEqual(fileIconFor("data.json", true), { name: "folder", color: "#8b949e" });
  });
});

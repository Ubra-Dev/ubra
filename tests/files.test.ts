import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { childRel, isHiddenName, joinFsPath } from "../src/lib/files.ts";

describe("childRel", () => {
  it("joins relative paths with forward slashes", () => {
    assert.equal(childRel("", "src"), "src");
    assert.equal(childRel("src", "lib"), "src/lib");
    assert.equal(childRel("a/b", "c"), "a/b/c");
  });
});

describe("isHiddenName", () => {
  it("matches dotfiles and dot-directories only", () => {
    assert.equal(isHiddenName(".gitignore"), true);
    assert.equal(isHiddenName(".config"), true);
    assert.equal(isHiddenName("visible.txt"), false);
    assert.equal(isHiddenName(""), false);
  });
});

describe("joinFsPath", () => {
  it("joins posix roots with forward slashes", () => {
    assert.equal(joinFsPath("/repo", "src/lib"), "/repo/src/lib");
    assert.equal(joinFsPath("/repo/", "src"), "/repo/src");
    assert.equal(joinFsPath("/repo", ""), "/repo");
  });

  it("joins windows roots with backslashes", () => {
    assert.equal(joinFsPath("C:\\repo", "src/lib"), "C:\\repo\\src\\lib");
    assert.equal(joinFsPath("C:\\repo\\", "src"), "C:\\repo\\src");
  });
});

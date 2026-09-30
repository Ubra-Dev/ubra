import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  changeCount,
  dirName,
  isClean,
  statusLabel,
  worktreeLabel,
  type GitStatus,
  type GitWorktree,
} from "../src/lib/git.ts";

function statusWith(overrides: Partial<GitStatus>): GitStatus {
  return {
    isRepo: true,
    branch: "main",
    upstream: null,
    ahead: 0,
    behind: 0,
    staged: [],
    unstaged: [],
    untracked: [],
    truncated: false,
    ...overrides,
  };
}

describe("changeCount", () => {
  it("sums staged, unstaged, and untracked entries", () => {
    const status = statusWith({
      staged: [{ path: "a.txt", status: "M", oldPath: null }],
      unstaged: [
        { path: "b.txt", status: "M", oldPath: null },
        { path: "c.txt", status: "D", oldPath: null },
      ],
      untracked: ["d.txt"],
    });
    assert.equal(changeCount(status), 4);
    assert.equal(changeCount(statusWith({})), 0);
  });
});

describe("isClean", () => {
  it("is true only without any changes", () => {
    assert.equal(isClean(statusWith({})), true);
    assert.equal(
      isClean(statusWith({ untracked: ["d.txt"] })),
      false,
    );
    assert.equal(
      isClean(statusWith({ staged: [{ path: "a.txt", status: "A", oldPath: null }] })),
      false,
    );
  });
});

describe("statusLabel", () => {
  it("expands porcelain letters and passes the rest through", () => {
    assert.equal(statusLabel("M"), "Modified");
    assert.equal(statusLabel("A"), "Added");
    assert.equal(statusLabel("D"), "Deleted");
    assert.equal(statusLabel("R"), "Renamed");
    assert.equal(statusLabel("U"), "Unmerged");
    assert.equal(statusLabel("?"), "Untracked");
    assert.equal(statusLabel("!"), "!");
  });
});

describe("dirName", () => {
  it("splits the parent directory from a repo-relative path", () => {
    assert.equal(dirName("src/lib/x.ts"), "src/lib");
    assert.equal(dirName("x.ts"), "");
    assert.equal(dirName("sub/"), "sub");
  });
});

describe("worktreeLabel", () => {
  function worktreeWith(overrides: Partial<GitWorktree>): GitWorktree {
    return {
      path: "/repo",
      head: "abc123",
      branch: null,
      detached: false,
      bare: false,
      locked: null,
      prunable: null,
      ...overrides,
    };
  }

  it("prefers the branch, then bare, then detached", () => {
    assert.equal(worktreeLabel(worktreeWith({ branch: "main" })), "main");
    assert.equal(worktreeLabel(worktreeWith({ bare: true, head: null })), "(bare)");
    assert.equal(
      worktreeLabel(worktreeWith({ detached: true })),
      "(detached HEAD)",
    );
  });
});

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  changeCount,
  dirName,
  gitSummaryTitle,
  gitSyncLabel,
  isClean,
  shortWorktreePath,
  statusLabel,
  summarizeStatus,
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

describe("summarizeStatus", () => {
  it("reduces a status to sidebar counts", () => {
    const summary = summarizeStatus(
      statusWith({
        staged: [{ path: "a.txt", status: "A", oldPath: null }],
        unstaged: [{ path: "b.txt", status: "M", oldPath: null }],
        untracked: ["c.txt", "d.txt"],
        ahead: 2,
        behind: 1,
        truncated: true,
      }),
    );
    assert.deepEqual(summary, {
      changed: 4,
      staged: 1,
      unstaged: 1,
      untracked: 2,
      ahead: 2,
      behind: 1,
      truncated: true,
    });
  });
});

describe("gitSummaryTitle", () => {
  it("says clean without changes", () => {
    assert.equal(gitSummaryTitle(summarizeStatus(statusWith({}))), "Clean");
  });

  it("breaks down dirty files with singular grammar", () => {
    assert.equal(
      gitSummaryTitle(
        summarizeStatus(
          statusWith({ unstaged: [{ path: "b.txt", status: "M", oldPath: null }] }),
        ),
      ),
      "1 changed file (1 unstaged)",
    );
    assert.equal(
      gitSummaryTitle(
        summarizeStatus(
          statusWith({
            staged: [{ path: "a.txt", status: "A", oldPath: null }],
            unstaged: [{ path: "b.txt", status: "M", oldPath: null }],
            untracked: ["c.txt"],
          }),
        ),
      ),
      "3 changed files (1 staged · 1 unstaged · 1 untracked)",
    );
  });

  it("marks truncated counts as lower bounds", () => {
    assert.equal(
      gitSummaryTitle(
        summarizeStatus(statusWith({ unstaged: [{ path: "b.txt", status: "M", oldPath: null }], truncated: true })),
      ),
      "1+ changed file (1 unstaged)",
    );
  });

  it("appends ahead and behind", () => {
    assert.equal(
      gitSummaryTitle(summarizeStatus(statusWith({ ahead: 2, behind: 1 }))),
      "Clean · ↑2 ahead · ↓1 behind",
    );
  });
});

describe("gitSyncLabel", () => {
  it("is empty when synced", () => {
    assert.equal(gitSyncLabel(summarizeStatus(statusWith({}))), "");
  });

  it("shows ahead and behind arrows", () => {
    assert.equal(
      gitSyncLabel(summarizeStatus(statusWith({ ahead: 2 }))),
      "↑2",
    );
    assert.equal(
      gitSyncLabel(summarizeStatus(statusWith({ behind: 1 }))),
      "↓1",
    );
    assert.equal(
      gitSyncLabel(summarizeStatus(statusWith({ ahead: 2, behind: 1 }))),
      "↑2 ↓1",
    );
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

describe("shortWorktreePath", () => {
  it("keeps the last two segments of long paths", () => {
    assert.equal(
      shortWorktreePath("/Users/me/repo/.worktrees/foo"),
      "…/.worktrees/foo",
    );
    assert.equal(shortWorktreePath("/a/b/c"), "…/b/c");
  });

  it("passes short paths through unchanged", () => {
    assert.equal(shortWorktreePath("/repo"), "/repo");
    assert.equal(shortWorktreePath("/a/b"), "/a/b");
    assert.equal(shortWorktreePath(""), "");
    assert.equal(shortWorktreePath("/"), "/");
  });

  it("handles trailing slashes and windows separators", () => {
    assert.equal(shortWorktreePath("/a/b/"), "/a/b");
    assert.equal(shortWorktreePath("C:\\repo\\.worktrees\\foo"), "…/.worktrees/foo");
    assert.equal(shortWorktreePath("C:\\repo"), "C:\\repo");
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

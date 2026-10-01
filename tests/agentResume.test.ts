import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  isValidResumeArgv,
  joinArgvPosix,
  restoreCommandFor,
  resumeArgvFor,
} from "../src/lib/agentResume.ts";

describe("agent resume commands", () => {
  it("resumes claude without a session and codex with one", () => {
    assert.deepEqual(resumeArgvFor("claude", undefined), ["claude", "--continue"]);
    assert.deepEqual(resumeArgvFor("Claude --model opus", undefined), [
      "claude",
      "--continue",
    ]);
    assert.deepEqual(
      resumeArgvFor("codex", { cli: "codex", value: "019dd790-abc" }),
      ["codex", "resume", "019dd790-abc"],
    );
    // A session from another CLI never applies.
    assert.equal(
      resumeArgvFor("codex", { cli: "claude", value: "019dd790-abc" }),
      null,
    );
    // Unverified CLIs and missing sessions fall back to the original.
    assert.equal(resumeArgvFor("codex", undefined), null);
    assert.equal(resumeArgvFor("droid", { cli: "droid", value: "s1" }), null);
    assert.equal(resumeArgvFor("/usr/bin/codex", { cli: "codex", value: "s1" }), null);
    assert.equal(resumeArgvFor("", undefined), null);
  });

  it("restores the resume line or the original command", () => {
    assert.equal(
      restoreCommandFor("codex", { cli: "codex", value: "019dd790-abc" }),
      "codex resume 019dd790-abc",
    );
    assert.equal(restoreCommandFor("claude", undefined), "claude --continue");
    assert.equal(restoreCommandFor("droid --auto", undefined), "droid --auto");
    assert.equal(restoreCommandFor("codex", undefined), "codex");
  });

  it("rejects hostile resume input from edited layouts", () => {
    assert.equal(isValidResumeArgv([]), false);
    assert.equal(isValidResumeArgv([""]), false);
    assert.equal(isValidResumeArgv(["/bin/codex", "resume", "x"]), false);
    assert.equal(isValidResumeArgv(["-codex", "resume", "x"]), false);
    assert.equal(isValidResumeArgv(["codex resume", "x"]), false);
    assert.equal(isValidResumeArgv(["codex", "resume", "bad\nid"]), false);
    assert.equal(isValidResumeArgv(["codex", "resume", "it's"]), false);
    assert.equal(isValidResumeArgv(["codex", "resume", "x".repeat(8192)]), false);
    assert.equal(
      isValidResumeArgv(["codex", ...new Array(64).fill("x")]),
      false,
    );
    assert.equal(isValidResumeArgv(["claude", "--continue"]), true);
    assert.equal(isValidResumeArgv(["codex", "resume", "019dd790-abc"]), true);
    // Invalid refs fall back instead of resuming.
    assert.equal(
      restoreCommandFor("codex", { cli: "codex", value: "bad\nid" }),
      "codex",
    );
  });

  it("quotes shell-joined arguments", () => {
    assert.equal(joinArgvPosix(["codex", "resume", "abc-123"]), "codex resume abc-123");
    assert.equal(joinArgvPosix(["agent", "two words"]), "agent 'two words'");
    assert.equal(joinArgvPosix(["agent", "a;b"]), "agent 'a;b'");
  });
});

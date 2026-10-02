import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const project = process.env.UBRA_SMOKE_PROJECT;
const remote = process.env.UBRA_SMOKE_REMOTE;

async function waitForFile(path, timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (existsSync(path)) return;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Timed out waiting for PTY marker: ${path}`);
}

describe("packaged release smoke", () => {
  it("launches, executes a PTY command, and renders the workspace explorer", async () => {
    assert.ok(process.env.APP_BINARY, "APP_BINARY must point to the installed packaged app");
    assert.equal(await browser.getTitle(), "Ubra — Agent runtime");
    await waitForFile(join(project, "marker.txt"));
    assert.equal((await browser.$("body").getText()).includes("Native Smoke"), true);

    await browser.$('button[role="tab"][title="Explorer"]').click();
    const explorer = await browser.$('[role="tree"][aria-label="Workspace files"]');
    await explorer.waitForExist();
    await browser.$('button[aria-label="Refresh"]').click();
    await browser.$('[role="treeitem"][title$="marker.txt"]').waitForExist();
  });

  it("stages, commits, and pushes a real change from Source Control", async () => {
    await browser.$('button[role="tab"][title="Source Control"]').click();
    const marker = "marker.txt";
    const stage = await browser.$(`[aria-label="Stage ${marker}"]`);
    await stage.waitForExist();
    await stage.click();
    await browser.$(`[aria-label="Unstage ${marker}"]`).waitForExist();

    const message = await browser.$('textarea[aria-label="Commit message"]');
    await message.setValue("Native smoke commit");
    const commitButton = await browser.$("button=Commit");
    await commitButton.waitForEnabled();
    await commitButton.click();
    await browser.waitUntil(() => {
      try {
        return execFileSync("git", ["-C", project, "log", "-1", "--pretty=%s"], { encoding: "utf8" }).trim() === "Native smoke commit";
      } catch {
        return false;
      }
    }, { timeout: 30_000, timeoutMsg: "Source Control did not create the local commit" });

    const push = await browser.$('button[aria-label="Push"]');
    await push.waitForEnabled();
    await push.click();
    await browser.waitUntil(() => {
      try {
        return execFileSync("git", ["--git-dir", remote, "rev-parse", "refs/heads/main"], { encoding: "utf8" }).trim().length > 0;
      } catch {
        return false;
      }
    }, { timeout: 30_000, timeoutMsg: "Source Control did not push the commit to the local bare remote" });

    const commit = execFileSync("git", ["--git-dir", remote, "log", "-1", "--pretty=%s", "refs/heads/main"], { encoding: "utf8" }).trim();
    assert.equal(commit, "Native smoke commit");
  });

  it("creates and switches to another tab", async () => {
    const tabs = await browser.$$(".tabbar .tab");
    const before = tabs.length;
    await browser.$('button[title^="New tab "]').click();
    await browser.waitUntil(async () => (await browser.$$(".tabbar .tab")).length === before + 1);
  });
});

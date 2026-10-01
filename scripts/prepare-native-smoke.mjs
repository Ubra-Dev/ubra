import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const root = resolve(".native-smoke");
const dataDir = join(root, "data");
const project = join(root, "project");
const remote = join(root, "remote.git");
mkdirSync(dataDir, { recursive: true });
mkdirSync(project, { recursive: true });
mkdirSync(root, { recursive: true });

function git(cwd, ...args) {
  execFileSync("git", args, { cwd, stdio: "inherit" });
}

git(root, "init", "--bare", "--initial-branch=main", remote);
git(project, "init", "--initial-branch=main");
git(project, "config", "user.name", "Ubra Native Smoke");
git(project, "config", "user.email", "ubra-smoke@example.invalid");
writeFileSync(join(project, "README.md"), "Native release smoke project\n");
git(project, "add", "README.md");
git(project, "commit", "-m", "Initial smoke fixture");
git(project, "remote", "add", "origin", remote);
git(project, "push", "--set-upstream", "origin", "main");

const windows = process.platform === "win32";
const command = windows
  ? ["powershell.exe", "-NoProfile", "-Command", "Set-Content -NoNewline -Path marker.txt -Value 'ubra-native-smoke-ok'; Start-Sleep -Seconds 600"]
  : ["/bin/sh", "-lc", "printf 'ubra-native-smoke-ok\\n' > marker.txt; sleep 600"];
const layout = {
  version: 2,
  activeWorkspaceId: "smoke-workspace",
  workspaces: [{
    id: "smoke-workspace",
    name: "Native Smoke",
    root: project,
    tabs: [{
      id: "smoke-tab",
      name: "Tab 1",
      root: { kind: "pane", id: "smoke-pane", cwd: project, cmd: command, cmdOnRestore: true },
    }],
    activeTabId: "smoke-tab",
  }],
};
writeFileSync(join(dataDir, "layout.json"), JSON.stringify(layout, null, 2));

const envFile = process.env.GITHUB_ENV;
if (envFile) {
  appendFileSync(envFile, `UBRA_DATA_DIR=${dataDir}\nUBRA_SMOKE_PROJECT=${project}\nUBRA_SMOKE_REMOTE=${remote}\n`);
} else {
  console.log(`UBRA_DATA_DIR=${dataDir}`);
}
console.log(`Prepared isolated app data and git fixture under ${root}`);

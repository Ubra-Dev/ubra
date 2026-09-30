import { execFileSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";

if (process.platform === "darwin") {
  const directory = "src-tauri/target/release/bundle/macos";
  if (!existsSync(directory)) throw new Error("Missing macOS bundle directory");
  const apps = readdirSync(directory).filter(name => name.endsWith(".app"));
  if (apps.length === 0) throw new Error("No macOS application bundle to verify");
  for (const app of apps) {
    const path = join(directory, app);
    execFileSync("codesign", ["--verify", "--deep", "--strict", "--verbose=2", path], { stdio: "inherit" });
    execFileSync("xcrun", ["stapler", "validate", path], { stdio: "inherit" });
    execFileSync("spctl", ["--assess", "--type", "execute", "--verbose=2", path], { stdio: "inherit" });
  }
} else if (process.platform === "win32") {
  execFileSync("pwsh", ["-NoProfile", "-File", "scripts/verify-signing.ps1"], { stdio: "inherit" });
}

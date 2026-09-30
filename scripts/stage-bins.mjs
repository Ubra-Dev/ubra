// Stage the daemon + CLI sidecars for `tauri build` (Tauri v2 externalBin).
//
// Builds the release bins and copies them to src-tauri/binaries/ with the
// host target-triple suffix Tauri expects
// (e.g. ubra-daemon-aarch64-apple-darwin). The bundler strips the suffix
// and places the sidecars next to the app binary on every platform.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const srcTauri = join(root, "..", "src-tauri");
const profile = process.env.STAGE_PROFILE ?? "release";
const outDir = process.env.CARGO_TARGET_DIR
  ? join(process.env.CARGO_TARGET_DIR, profile)
  : join(srcTauri, "target", profile);

execFileSync("cargo", ["build", ...(profile === "release" ? ["--release"] : []), "--bins"], {
  cwd: srcTauri,
  stdio: "inherit",
});

const host = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((line) => line.startsWith("host:"))
  ?.split("host:")[1]
  .trim();
if (!host) throw new Error("could not determine rustc host triple");

const exe = process.platform === "win32" ? ".exe" : "";
mkdirSync(join(srcTauri, "binaries"), { recursive: true });
for (const name of ["ubra-daemon", "ubra-cli"]) {
  const from = join(outDir, `${name}${exe}`);
  if (!existsSync(from)) throw new Error(`missing built binary: ${from}`);
  const to = join(srcTauri, "binaries", `${name}-${host}${exe}`);
  copyFileSync(from, to);
  console.log(`staged ${to}`);
}

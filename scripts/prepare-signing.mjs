// Release runners only. Secrets are passed via the environment, never logged.
import { execFileSync } from "node:child_process";
import { chmodSync, mkdtempSync, rmSync, writeFileSync, appendFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error(`Release signing requires ${name}; configure it in repository Actions secrets.`);
  return value;
}

if (process.platform === "darwin") {
  const certificate = required("APPLE_CERTIFICATE");
  const certificatePassword = required("APPLE_CERTIFICATE_PASSWORD");
  required("APPLE_SIGNING_IDENTITY");
  required("APPLE_ID");
  required("APPLE_PASSWORD");
  required("APPLE_TEAM_ID");
  const directory = mkdtempSync(join(tmpdir(), "ubra-signing-"));
  const p12 = join(directory, "certificate.p12");
  const keychain = join(directory, "release.keychain-db");
  const password = required("KEYCHAIN_PASSWORD");
  writeFileSync(p12, Buffer.from(certificate, "base64"), { mode: 0o600 });
  chmodSync(p12, 0o600);
  appendFileSync(required("GITHUB_ENV"), `UBRA_SIGNING_KEYCHAIN=${keychain}\n`);
  function security(args) {
    try { return execFileSync("security", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }); }
    catch { throw new Error(`macOS signing setup failed at ${args[0]}; verify your certificate and credentials.`); }
  }
  try {
    security(["create-keychain", "-p", password, keychain]);
    security(["set-keychain-settings", "-lut", "21600", keychain]);
    security(["unlock-keychain", "-p", password, keychain]);
    security(["import", p12, "-k", keychain, "-P", certificatePassword, "-T", "/usr/bin/codesign", "-T", "/usr/bin/productbuild"]);
    security(["set-key-partition-list", "-S", "apple-tool:,apple:,codesign:", "-s", "-k", password, keychain]);
    const existing = security(["list-keychains", "-d", "user"]).match(/"([^"]+)"/g)?.map(s => s.slice(1, -1)) ?? [];
    security(["list-keychains", "-d", "user", "-s", keychain, ...existing]);
  } finally {
    rmSync(p12, { force: true });
  }
  console.log("Imported macOS signing certificate into an ephemeral release keychain.");
} else if (process.platform === "win32") {
  required("WINDOWS_CERTIFICATE");
  required("WINDOWS_CERTIFICATE_PASSWORD");
  throw new Error("Use scripts/prepare-signing.ps1 on Windows.");
}

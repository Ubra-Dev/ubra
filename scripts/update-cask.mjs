#!/usr/bin/env node
// Render Casks/ubra.rb for Ubra-Dev/homebrew-tap from release DMGs.
// Usage: node scripts/update-cask.mjs --tag v0.1.0 --repo owner/name
//   --dir dmg --out Casks/ubra.rb
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  assertArchPair,
  buildCaskRb,
  classifyDmgArch,
} from "../src/lib/caskManifest.ts";

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  if (i < 0 || i + 1 >= process.argv.length) {
    console.error(`missing required --${name}`);
    process.exit(1);
  }
  return process.argv[i + 1];
}

function walk(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

const tag = arg("tag");
const repo = arg("repo");
const dir = arg("dir");
const out = arg("out");

const dmgs = [];
for (const full of walk(dir)) {
  const file = full.split("/").pop();
  if (!file.endsWith(".dmg")) continue;
  const sha256 = createHash("sha256").update(readFileSync(full)).digest("hex");
  dmgs.push({ arch: classifyDmgArch(file), file, sha256 });
}
assertArchPair(dmgs);

// Fixed display fields mirror src-tauri/tauri.conf.json (productName,
// identifier), package.json (description, homepage), and the bundle's
// 10.15 minimum, rendered by buildCaskRb; tests/caskManifest.test.ts
// guards them against config drift.
const arm = dmgs.find((dmg) => dmg.arch === "arm");
const intel = dmgs.find((dmg) => dmg.arch === "intel");
writeFileSync(
  out,
  buildCaskRb({
    version: tag.replace(/^v/, ""),
    tag,
    repo,
    productName: "Ubra",
    identifier: "com.ubra.app",
    description: "Agent runtime desktop app",
    homepage: "https://getubra.com/",
    launchAgentName: "Ubra",
    arm,
    intel,
  }),
);
console.log(`wrote ${out} (arm ${arm.file}, intel ${intel.file})`);

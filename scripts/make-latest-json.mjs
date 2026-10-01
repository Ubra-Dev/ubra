#!/usr/bin/env node
// Build the static updater manifest (latest.json) from release bundles.
// Usage: node scripts/make-latest-json.mjs --tag v0.1.0 --repo owner/name
//   --dir bundles --out bundles/latest.json [--notes "..."]
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  assertCompletePlatforms,
  buildLatestJson,
  collectUpdaterBundles,
} from "../src/lib/updaterManifest.ts";

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  if (i < 0 || i + 1 >= process.argv.length) {
    console.error(`missing required --${name}`);
    process.exit(1);
  }
  return process.argv[i + 1];
}

function optional(name) {
  const i = process.argv.indexOf(`--${name}`);
  return i < 0 ? undefined : process.argv[i + 1];
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
const notes = optional("notes");

const byName = new Map();
const names = [];
for (const full of walk(dir)) {
  const name = full.split("/").pop();
  names.push(name);
  // First wins for content; collectUpdaterBundles throws on duplicate
  // updater-bundle names, while unrelated files may repeat freely.
  if (!byName.has(name)) byName.set(name, full);
}

const bundles = collectUpdaterBundles(
  names,
  (sigName) => readFileSync(byName.get(sigName), "utf8"),
  (bundleName) =>
    `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(bundleName)}`,
);
assertCompletePlatforms(bundles);

writeFileSync(
  out,
  buildLatestJson({
    version: tag.replace(/^v/, ""),
    pubDate: new Date().toISOString(),
    ...(notes ? { notes } : {}),
    bundles,
  }),
);
console.log(`wrote ${out} (${bundles.length} platform entries)`);

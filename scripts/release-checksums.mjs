import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

export function prepareRelease(input, output) {
  mkdirSync(output, { recursive: true });
  const files = [];
  function visit(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      // An .app is a directory; its signed downloadable DMG is published instead.
      if (entry.name.endsWith(".app") || entry.name.endsWith(".AppDir")) continue;
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile() && /(?:\.(?:dmg|msi|exe|deb|rpm|AppImage|zip|tar\.gz)|^(?:LICENSE|THIRD_PARTY_NOTICES\.txt|dependency-inventory\.json))$/.test(entry.name)) files.push(path);
    }
  }
  visit(input);
  if (!files.some(path => /\.(?:dmg|msi|exe|deb|rpm|AppImage)$/.test(path))) throw new Error("No release installers found");
  const names = new Set();
  const lines = [];
  for (const path of files.sort()) {
    const name = basename(path);
    if (names.has(name)) throw new Error(`Duplicate release filename: ${name}`);
    names.add(name);
    copyFileSync(path, join(output, name));
    lines.push(`${createHash("sha256").update(readFileSync(path)).digest("hex")}  ${name}`);
  }
  for (const name of ["LICENSE", "THIRD_PARTY_NOTICES.txt", "dependency-inventory.json", "THIRD_PARTY_SOURCES.tar.gz"]) {
    if (!names.has(name)) throw new Error(`Missing release notice/source asset: ${name}`);
  }
  writeFileSync(join(output, "SHA256SUMS"), lines.join("\n") + "\n");
  return lines;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  prepareRelease(resolve(process.argv[2] ?? "bundles"), resolve(process.argv[3] ?? "release-assets"));
}

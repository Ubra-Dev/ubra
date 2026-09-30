// Preserve dependency notices without rewriting their upstream license terms.
import { execFileSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const noticeName = /^(license|licence|copying|notice|copyright|authors)(?:$|[._-])/i;

export function licenseFiles(directory) {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === "node_modules" || entry.name === ".git" || entry.name === "target") continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...licenseFiles(path));
    else if (entry.isFile() && noticeName.test(entry.name)) files.push(path);
  }
  return files.sort();
}

export function noticeSection(component, texts) {
  if (!component.license || texts.length === 0 || texts.some(t => !t.text.trim())) {
    throw new Error(`Missing license declaration or text: ${component.name}@${component.version}`);
  }
  return [
    "=".repeat(80),
    `${component.ecosystem}: ${component.name}@${component.version}`,
    `Declared license: ${component.license}`,
    ...(component.repository ? [`Upstream: ${component.repository}`] : []),
    ...texts.map(({ source, text }) => `\n--- ${source} ---\n${text.trimEnd()}\n`),
  ].join("\n");
}

export function collectLicenses() {
  const output = join(root, ".tooling", "legal");
  mkdirSync(output, { recursive: true });
  const overrides = JSON.parse(readFileSync(join(root, "third_party", "license-overrides.json"), "utf8"));
  const lock = JSON.parse(readFileSync(join(root, "package-lock.json"), "utf8"));
  const components = [];
  for (const [path, locked] of Object.entries(lock.packages)) {
    if (!path || locked.dev) continue;
    const directory = join(root, path);
    if (!existsSync(directory)) {
      if (locked.optional) continue;
      throw new Error(`Missing installed runtime dependency: ${path}. Run npm ci first.`);
    }
    const pkg = JSON.parse(readFileSync(join(directory, "package.json"), "utf8"));
    if (pkg.version !== locked.version) throw new Error(`Lock/install version mismatch: ${path}`);
    components.push({
      ecosystem: "npm", name: pkg.name, version: pkg.version, license: pkg.license,
      repository: typeof pkg.repository === "string" ? pkg.repository : pkg.repository?.url,
      directory,
    });
  }

  const metadata = JSON.parse(execFileSync("cargo", [
    "metadata", "--locked", "--format-version", "1", "--manifest-path", "src-tauri/Cargo.toml",
  ], { cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024 }));
  for (const pkg of metadata.packages) {
    if (!pkg.source) continue;
    components.push({
      ecosystem: "cargo", name: pkg.name, version: pkg.version, license: pkg.license,
      repository: pkg.repository, directory: dirname(pkg.manifest_path), licenseFile: pkg.license_file,
    });
  }
  components.sort((a, b) => `${a.ecosystem}:${a.name}@${a.version}`.localeCompare(`${b.ecosystem}:${b.name}@${b.version}`, "en"));
  const sections = [
    readFileSync(join(root, "THIRD_PARTY_NOTICES.md"), "utf8"),
    noticeSection({ ecosystem: "asset", name: "Feather icons", version: "2013–2023", license: "MIT" }, [
      { source: "https://github.com/feathericons/feather/blob/main/LICENSE", text: readFileSync(join(root, "third_party/licenses/Feather-MIT.txt"), "utf8") },
    ]),
  ];
  const sources = join(output, "sources");
  rmSync(sources, { recursive: true, force: true });
  mkdirSync(sources, { recursive: true });
  const inventory = [];
  for (const component of components) {
    const files = licenseFiles(component.directory);
    if (component.licenseFile) {
      const file = resolve(component.directory, component.licenseFile);
      if (!files.includes(file)) files.push(file);
    }
    const texts = files.map(path => ({
      source: relative(component.directory, path).split("\\").join("/"),
      text: readFileSync(path, "utf8"),
    }));
    const supplements = overrides[`${component.ecosystem}:${component.name}@${component.version}`] ?? [];
    for (const supplement of supplements) {
      texts.push({ source: supplement.source, text: readFileSync(join(root, supplement.file), "utf8") });
    }
    sections.push(noticeSection(component, texts));
    const { directory, licenseFile, ...publicInfo } = component;
    inventory.push({ ...publicInfo, notices: texts.map(t => t.source) });
    if (component.ecosystem === "cargo" && /\bMPL-2\.0\b/.test(component.license)) {
      const destination = join(sources, `${component.name}-${component.version}`);
      cpSync(component.directory, destination, { recursive: true });
      for (const supplement of supplements) {
        const to = join(destination, "upstream-licenses", supplement.file.split("/").at(-1));
        mkdirSync(dirname(to), { recursive: true });
        copyFileSync(join(root, supplement.file), to);
      }
    }
  }
  writeFileSync(join(output, "THIRD_PARTY_NOTICES.txt"), sections.join("\n\n") + "\n");
  writeFileSync(join(output, "dependency-inventory.json"), JSON.stringify(inventory, null, 2) + "\n");
  copyFileSync(join(root, "LICENSE"), join(output, "LICENSE"));
  writeFileSync(join(sources, "README.txt"),
    "Corresponding unmodified source of MPL-2.0 Rust components from Ubra's locked dependencies.\n" +
    "See THIRD_PARTY_NOTICES.txt and dependency-inventory.json distributed with this release.\n");
  execFileSync("tar", ["-czf", join(output, "THIRD_PARTY_SOURCES.tar.gz"), "-C", output, "sources"], { cwd: root });
  console.log(`Prepared notices for ${components.length} dependencies plus Feather icons in .tooling/legal/`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) collectLicenses();

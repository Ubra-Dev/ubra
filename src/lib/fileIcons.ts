import type { IconName } from "./Icon.svelte";

export interface FileIcon {
  name: IconName;
  color: string;
}

/**
 * VSCode Seti-inspired file colors. Fixed hex (like Seti) so icons read the
 * same on every theme; all values stay legible on dark and light surfaces.
 */
const CODE_JS = "#cbcb41";
const CODE_TS = "#519aba";
const CODE_PY = "#4b8bbe";
const CODE_RED = "#cc342d";
const CODE_ORANGE = "#e37933";
const CODE_GREEN = "#5c9e46";
const CODE_PURPLE = "#a074c4";
const CODE_PINK = "#e535ab";
const CODE_TEAL = "#4e8ca2";
const MUTED = "#8b949e";
const DIMMED = "#5c636e";

const DEFAULT_FILE: FileIcon = { name: "file", color: MUTED };
const DEFAULT_FOLDER: FileIcon = { name: "folder", color: MUTED };

const CODE: Record<string, string> = {
  js: CODE_JS, jsx: CODE_JS, mjs: CODE_JS, cjs: CODE_JS,
  ts: CODE_TS, mts: CODE_TS, cts: CODE_TS, tsx: CODE_TS,
  vue: "#41b883", svelte: "#ff3e00", astro: "#e535ab",
  py: CODE_PY, pyw: CODE_PY, r: "#2266aa",
  rb: CODE_RED, java: CODE_ORANGE, scala: CODE_RED, kt: "#7f52ff", kts: "#7f52ff",
  go: "#2fa8c5", rs: "#dea584",
  c: CODE_TS, h: CODE_PURPLE, cpp: CODE_TS, hpp: CODE_PURPLE, cc: CODE_TS, cxx: CODE_TS,
  cs: "#5966c5", swift: CODE_ORANGE, dart: CODE_TEAL, php: "#777bb4",
  lua: CODE_TS, hs: CODE_PURPLE, ex: CODE_PURPLE, exs: CODE_PURPLE,
  clj: CODE_GREEN, cljs: CODE_GREEN, elm: CODE_TEAL,
  jl: CODE_PURPLE, nim: CODE_ORANGE, zig: CODE_ORANGE,
  json: CODE_JS, jsonc: CODE_JS, json5: CODE_JS, webmanifest: CODE_JS,
  css: CODE_TS, scss: "#cd6799", sass: "#cd6799", less: CODE_TS,
  xml: CODE_ORANGE, xsl: CODE_ORANGE, xsd: CODE_ORANGE,
  graphql: CODE_PINK, gql: CODE_PINK,
  sql: CODE_JS,
  csv: CODE_GREEN, tsv: CODE_GREEN,
};

const GLYPH_BY_EXT: Record<string, IconName> = {
  html: "globe", htm: "globe",
  md: "file-text", mdx: "file-text", markdown: "file-text", rst: "file-text",
  txt: "file-text", text: "file-text", log: "file-text",
  pdf: "file-text", tex: "file-text", ipynb: "file-text",
  png: "image", jpg: "image", jpeg: "image", gif: "image", webp: "image",
  ico: "image", avif: "image", bmp: "image", tif: "image", tiff: "image",
  svg: "image",
  mp3: "music", wav: "music", ogg: "music", flac: "music", m4a: "music", mid: "music",
  mp4: "video", mov: "video", webm: "video", mkv: "video", avi: "video",
  zip: "archive", tar: "archive", gz: "archive", bz2: "archive", xz: "archive",
  "7z": "archive", rar: "archive", tgz: "archive",
  db: "database", sqlite: "database", sqlite3: "database",
  sh: "terminal", bash: "terminal", zsh: "terminal", fish: "terminal",
  ps1: "terminal", bat: "terminal", cmd: "terminal", ksh: "terminal",
  yml: "file", yaml: "file",
  lock: "lock",
  pem: "key", key: "key", crt: "key", cert: "key", p12: "key", pfx: "key",
  ttf: "type", otf: "type", woff: "type", woff2: "type", eot: "type",
  exe: "package", msi: "package", dmg: "package", pkg: "package",
  deb: "package", rpm: "package", appimage: "package",
  toml: "file", ini: "file", cfg: "file", conf: "file", properties: "file",
};

const COLOR_BY_EXT: Record<string, string> = {
  html: CODE_ORANGE, htm: CODE_ORANGE,
  md: CODE_TS, mdx: CODE_TS, markdown: CODE_TS, rst: CODE_TS,
  txt: MUTED, text: MUTED, log: MUTED,
  pdf: CODE_RED, tex: CODE_TS, ipynb: CODE_ORANGE,
  png: CODE_PURPLE, jpg: CODE_PURPLE, jpeg: CODE_PURPLE, gif: CODE_PURPLE,
  webp: CODE_PURPLE, ico: CODE_PURPLE, avif: CODE_PURPLE, bmp: CODE_PURPLE,
  tif: CODE_PURPLE, tiff: CODE_PURPLE,
  svg: CODE_JS,
  mp3: CODE_PURPLE, wav: CODE_PURPLE, ogg: CODE_PURPLE, flac: CODE_PURPLE,
  m4a: CODE_PURPLE, mid: CODE_PURPLE,
  mp4: CODE_PURPLE, mov: CODE_PURPLE, webm: CODE_PURPLE, mkv: CODE_PURPLE,
  avi: CODE_PURPLE,
  zip: MUTED, tar: MUTED, gz: MUTED, bz2: MUTED, xz: MUTED,
  "7z": MUTED, rar: MUTED, tgz: MUTED,
  db: MUTED, sqlite: MUTED, sqlite3: MUTED,
  sh: CODE_GREEN, bash: CODE_GREEN, zsh: CODE_GREEN, fish: CODE_GREEN,
  ps1: CODE_TS, bat: CODE_GREEN, cmd: CODE_GREEN, ksh: CODE_GREEN,
  yml: CODE_RED, yaml: CODE_RED,
  lock: MUTED,
  pem: CODE_JS, key: CODE_JS, crt: CODE_JS, cert: CODE_JS, p12: CODE_JS, pfx: CODE_JS,
  ttf: CODE_TS, otf: CODE_TS, woff: CODE_TS, woff2: CODE_TS, eot: CODE_TS,
  exe: MUTED, msi: MUTED, dmg: MUTED, pkg: MUTED,
  deb: MUTED, rpm: MUTED, appimage: MUTED,
  toml: "#9c4223", ini: MUTED, cfg: MUTED, conf: MUTED, properties: MUTED,
};

/** Exact lowercase basenames with their own icon. */
const BY_NAME: Record<string, FileIcon> = {
  "package.json": { name: "package", color: CODE_RED },
  "package-lock.json": { name: "package", color: CODE_RED },
  "tsconfig.json": { name: "code", color: CODE_TS },
  "jsconfig.json": { name: "code", color: CODE_TS },
  "docker-compose.yml": { name: "package", color: CODE_TS },
  "docker-compose.yaml": { name: "package", color: CODE_TS },
  "cmakelists.txt": { name: "tool", color: MUTED },
  makefile: { name: "tool", color: MUTED },
  gnumakefile: { name: "tool", color: MUTED },
  ".gitignore": { name: "file", color: CODE_ORANGE },
  ".gitattributes": { name: "file", color: CODE_ORANGE },
  ".gitmodules": { name: "file", color: CODE_ORANGE },
  ".editorconfig": { name: "settings", color: MUTED },
  ".prettierrc": { name: "settings", color: MUTED },
  ".eslintrc": { name: "settings", color: MUTED },
};

const CONFIG_SUFFIXES = [".config.js", ".config.mjs", ".config.cjs", ".config.ts", ".config.mts", ".config.cts"];

/** Lowercase basename prefixes with their own icon. */
const BY_PREFIX: Array<[string, FileIcon]> = [
  ["dockerfile", { name: "package", color: CODE_TS }],
  ["readme", { name: "book", color: CODE_TS }],
  ["license", { name: "book", color: CODE_JS }],
  ["licence", { name: "book", color: CODE_JS }],
  ["copying", { name: "book", color: CODE_JS }],
  ["changelog", { name: "file-text", color: CODE_TS }],
  [".env", { name: "key", color: CODE_JS }],
];

const FOLDER_BLUE = new Set([
  "src", "source", "sources", "lib", "libs", "app", "apps", "packages",
  "components", "ui", "core", "client", "frontend", "web", "www", "site", "styles", "style",
]);
const FOLDER_GREEN = new Set([
  "test", "tests", "__tests__", "spec", "specs", "e2e", "fixtures", "mocks", "__mocks__",
]);
const FOLDER_YELLOW = new Set(["docs", "doc", "documentation", "wiki", "manual", "manuals"]);
const FOLDER_PURPLE = new Set([
  "assets", "asset", "static", "public", "images", "img", "media", "fonts", "icons",
]);
const FOLDER_ORANGE = new Set(["scripts", "script", "tools", "tooling", "bin", "tasks"]);
const FOLDER_DIM = new Set([
  "node_modules", ".git", "dist", "build", "out", "target", "vendor", ".next",
  ".nuxt", "coverage", ".coverage", "tmp", "temp", ".cache", "__pycache__", ".venv", "venv",
]);

/**
 * Resolve the explorer icon for an entry. Matching is case-insensitive:
 * exact filename, then prefix, then extension, then the default.
 */
export function fileIconFor(name: string, isDir: boolean): FileIcon {
  const lower = name.toLowerCase();
  if (isDir) {
    if (FOLDER_BLUE.has(lower)) return { name: "folder", color: CODE_TS };
    if (FOLDER_GREEN.has(lower)) return { name: "folder", color: CODE_GREEN };
    if (FOLDER_YELLOW.has(lower)) return { name: "folder", color: CODE_JS };
    if (FOLDER_PURPLE.has(lower)) return { name: "folder", color: CODE_PURPLE };
    if (FOLDER_ORANGE.has(lower)) return { name: "folder", color: CODE_ORANGE };
    if (FOLDER_DIM.has(lower)) return { name: "folder", color: DIMMED };
    return DEFAULT_FOLDER;
  }
  const exact = BY_NAME[lower];
  if (exact) return exact;
  for (const [prefix, icon] of BY_PREFIX) {
    if (lower === prefix || lower.startsWith(prefix + ".") || lower.startsWith(prefix + "-") || lower.startsWith(prefix + "_")) {
      return icon;
    }
  }
  if (lower.endsWith(".d.ts") || lower.endsWith(".d.mts") || lower.endsWith(".d.cts")) {
    return { name: "code", color: CODE_TS };
  }
  for (const suffix of CONFIG_SUFFIXES) {
    if (lower.endsWith(suffix)) return { name: "settings", color: MUTED };
  }
  const ext = extensionOf(lower);
  if (ext) {
    const code = CODE[ext];
    if (code) return { name: "code", color: code };
    const glyph = GLYPH_BY_EXT[ext];
    if (glyph) return { name: glyph, color: COLOR_BY_EXT[ext] ?? MUTED };
  }
  return DEFAULT_FILE;
}

/** Extension without the dot; dotfiles like ".gitignore" report no extension. */
function extensionOf(lower: string): string {
  const dot = lower.lastIndexOf(".");
  if (dot <= 0 || dot === lower.length - 1) return "";
  return lower.slice(dot + 1);
}

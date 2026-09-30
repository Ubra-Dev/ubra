import hljs from "highlight.js/lib/core";
import bash from "highlight.js/lib/languages/bash";
import c from "highlight.js/lib/languages/c";
import cmake from "highlight.js/lib/languages/cmake";
import cpp from "highlight.js/lib/languages/cpp";
import csharp from "highlight.js/lib/languages/csharp";
import css from "highlight.js/lib/languages/css";
import dart from "highlight.js/lib/languages/dart";
import diff from "highlight.js/lib/languages/diff";
import dockerfile from "highlight.js/lib/languages/dockerfile";
import go from "highlight.js/lib/languages/go";
import graphql from "highlight.js/lib/languages/graphql";
import ini from "highlight.js/lib/languages/ini";
import java from "highlight.js/lib/languages/java";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import kotlin from "highlight.js/lib/languages/kotlin";
import lua from "highlight.js/lib/languages/lua";
import makefile from "highlight.js/lib/languages/makefile";
import markdown from "highlight.js/lib/languages/markdown";
import perl from "highlight.js/lib/languages/perl";
import php from "highlight.js/lib/languages/php";
import powershell from "highlight.js/lib/languages/powershell";
import protobuf from "highlight.js/lib/languages/protobuf";
import python from "highlight.js/lib/languages/python";
import r from "highlight.js/lib/languages/r";
import ruby from "highlight.js/lib/languages/ruby";
import rust from "highlight.js/lib/languages/rust";
import scss from "highlight.js/lib/languages/scss";
import sql from "highlight.js/lib/languages/sql";
import swift from "highlight.js/lib/languages/swift";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

hljs.registerLanguage("bash", bash);
hljs.registerLanguage("c", c);
hljs.registerLanguage("cmake", cmake);
hljs.registerLanguage("cpp", cpp);
hljs.registerLanguage("csharp", csharp);
hljs.registerLanguage("css", css);
hljs.registerLanguage("dart", dart);
hljs.registerLanguage("diff", diff);
hljs.registerLanguage("dockerfile", dockerfile);
hljs.registerLanguage("go", go);
hljs.registerLanguage("graphql", graphql);
hljs.registerLanguage("ini", ini);
hljs.registerLanguage("java", java);
hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("json", json);
hljs.registerLanguage("kotlin", kotlin);
hljs.registerLanguage("lua", lua);
hljs.registerLanguage("makefile", makefile);
hljs.registerLanguage("markdown", markdown);
hljs.registerLanguage("perl", perl);
hljs.registerLanguage("php", php);
hljs.registerLanguage("powershell", powershell);
hljs.registerLanguage("protobuf", protobuf);
hljs.registerLanguage("python", python);
hljs.registerLanguage("r", r);
hljs.registerLanguage("ruby", ruby);
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("scss", scss);
hljs.registerLanguage("sql", sql);
hljs.registerLanguage("swift", swift);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("yaml", yaml);

/** Extension (lowercase, no dot) to registered highlight.js language. */
const EXTENSIONS: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  jsonc: "json",
  webmanifest: "json",
  rs: "rust",
  py: "python",
  pyi: "python",
  pyw: "python",
  html: "xml",
  htm: "xml",
  xhtml: "xml",
  svg: "xml",
  xaml: "xml",
  xml: "xml",
  plist: "xml",
  svelte: "xml",
  vue: "xml",
  astro: "xml",
  css: "css",
  scss: "scss",
  less: "scss",
  md: "markdown",
  markdown: "markdown",
  mdown: "markdown",
  yml: "yaml",
  yaml: "yaml",
  toml: "ini",
  ini: "ini",
  cfg: "ini",
  conf: "ini",
  editorconfig: "ini",
  gitconfig: "ini",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  ps1: "powershell",
  psm1: "powershell",
  diff: "diff",
  patch: "diff",
  sql: "sql",
  c: "c",
  h: "c",
  cpp: "cpp",
  cc: "cpp",
  cxx: "cpp",
  hpp: "cpp",
  hh: "cpp",
  cs: "csharp",
  java: "java",
  go: "go",
  swift: "swift",
  kt: "kotlin",
  kts: "kotlin",
  rb: "ruby",
  php: "php",
  lua: "lua",
  dart: "dart",
  graphql: "graphql",
  gql: "graphql",
  proto: "protobuf",
  pl: "perl",
  pm: "perl",
  r: "r",
  cmake: "cmake",
};

/** Exact file names (lowercase) to language, for extensionless files. */
const EXACT_NAMES: Record<string, string> = {
  dockerfile: "dockerfile",
  makefile: "makefile",
  gnumakefile: "makefile",
  "cmakelists.txt": "cmake",
  gemfile: "ruby",
  rakefile: "ruby",
  podfile: "ruby",
  bashrc: "bash",
  zshrc: "bash",
  profile: "bash",
};

/** Leading bytes used for auto-detection, bounding its cost on huge files. */
const AUTO_SAMPLE_CHARS = 32768;

/**
 * Highlight.js language for a file, or null when nothing maps and the
 * caller should auto-detect or render plaintext. Matching is case-insensitive
 * and ignores directory components.
 */
export function languageForFile(fileName: string): string | null {
  const base = fileName.split("/").pop() ?? fileName;
  const lower = base.toLowerCase();
  const exact = EXACT_NAMES[lower];
  if (exact) return exact;
  const dot = lower.lastIndexOf(".");
  if (dot < 0) return null;
  return EXTENSIONS[lower.slice(dot + 1)] ?? null;
}

export interface HighlightedCode {
  /** Safe HTML: highlighted spans or escaped plaintext, never raw source. */
  html: string;
  language: string;
}

function escapeHtml(source: string): string {
  return source
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/**
 * Highlight source for display. Prefers the mapped language for the file
 * name, falls back to auto-detection on a bounded sample, and finally to
 * escaped plaintext. Never throws and never returns unescaped source.
 */
export function highlightCode(source: string, fileName: string): HighlightedCode {
  if (source === "") return { html: "", language: "plaintext" };
  try {
    const mapped = languageForFile(fileName);
    if (mapped && hljs.getLanguage(mapped)) {
      return { html: hljs.highlight(source, { language: mapped }).value, language: mapped };
    }
    const auto = hljs.highlightAuto(source.slice(0, AUTO_SAMPLE_CHARS));
    if (auto.language && hljs.getLanguage(auto.language)) {
      return {
        html: hljs.highlight(source, { language: auto.language }).value,
        language: auto.language,
      };
    }
  } catch {
    // Fall through to plaintext below.
  }
  return { html: escapeHtml(source), language: "plaintext" };
}

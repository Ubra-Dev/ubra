import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { cliBrand, type CliBrand } from "../src/lib/agentCliIcons.ts";

type Logo = Extract<CliBrand, { kind: "logo" }>;
type Mono = Extract<CliBrand, { kind: "mono" }>;

function expectLogo(cli: string): Logo {
  const brand = cliBrand(cli);
  assert.equal(brand.kind, "logo");
  if (brand.kind !== "logo") throw new Error(`expected a logo for ${cli}`);
  return brand;
}

function expectMono(cli: string, label?: string): Mono {
  const brand = cliBrand(cli, label);
  assert.equal(brand.kind, "mono");
  if (brand.kind !== "mono") throw new Error(`expected a monogram for ${cli}`);
  return brand;
}

/** Mirrors AGENT_TABLE stems in src-tauri/src/agent_watch.rs. */
const KNOWN_STEMS = [
  "claude", "codex", "cursor-agent", "opencode", "gemini", "copilot", "amp",
  "aider", "goose", "crush", "droid", "qwen", "grok", "kiro", "cline",
  "devin", "muse", "pi", "omp", "kimi", "hermes", "qoder", "qodercli",
  "letta", "kilo", "mastracode", "antigravity", "antigravity-cli", "maki",
  "mimo", "agy",
];

describe("cliBrand", () => {
  it("resolves vendored logos for mapped CLIs", () => {
    for (const [cli, hex] of [
      ["claude", "191919"],
      ["gemini", "8E75B2"],
      ["copilot", "000000"],
      ["cursor-agent", "000000"],
      ["cline", "18181B"],
      ["qwen", "6950EF"],
      ["kimi", "000000"],
    ]) {
      const brand = expectLogo(cli);
      assert.equal(brand.hex, hex);
      assert.match(brand.path, /^[0-9A-Za-z.,\- ]+$/);
      assert.ok(brand.path.length > 100);
    }
  });

  it("matches CLI ids case-insensitively", () => {
    assert.equal(cliBrand("Claude").kind, "logo");
    assert.equal(cliBrand("  GEMINI ").kind, "logo");
  });

  it("falls back to a monogram tile for unmapped CLIs", () => {
    const brand = expectMono("codex", "Codex");
    assert.equal(brand.letter, "C");
    assert.equal(brand.bg, "#10A37F");
  });

  it("derives the monogram letter from the label, then the id", () => {
    assert.equal(expectMono("maki", "Maki").letter, "M");
    assert.equal(expectMono("maki").letter, "M");
    assert.equal(expectMono("9lives").letter, "9");
    assert.equal(expectMono("-aider", "-aider").letter, "A");
  });

  it("never crashes on blank input", () => {
    for (const cli of ["", "   "]) {
      const brand = expectMono(cli);
      assert.equal(brand.letter, "?");
      assert.match(brand.bg, /^#[0-9A-F]{6}$/);
    }
  });

  it("assigns deterministic palette colors to unmapped CLIs", () => {
    const first = expectMono("goose");
    const second = expectMono("GOOSE", "Goose");
    assert.equal(first.bg, second.bg);
    assert.match(first.bg, /^#[0-9A-F]{6}$/);
  });

  it("resolves every known agent stem without crashing", () => {
    for (const stem of KNOWN_STEMS) {
      const brand = cliBrand(stem);
      assert.ok(brand.kind === "logo" || brand.kind === "mono");
    }
  });
});

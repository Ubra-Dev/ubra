import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_THEME_ID,
  THEMES,
  THEME_IDS,
  isThemeId,
  onAccentFor,
  themeStyle,
  withAlpha,
} from "../src/lib/themes.ts";

function luminance(hex: string): number {
  const value = Number.parseInt(hex.slice(1), 16);
  const channel = (shift: number) => {
    const component = ((value >> shift) & 0xff) / 255;
    return component <= 0.03928
      ? component / 12.92
      : Math.pow((component + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
}

function contrastRatio(first: string, second: string): number {
  const [lighter, darker] = [luminance(first), luminance(second)].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

const builtInThemeIds = [
  "catppuccin",
  "catppuccin-latte",
  "tokyo-night",
  "tokyo-night-day",
  "dracula",
  "nord",
  "gruvbox",
  "gruvbox-light",
  "one-dark",
  "one-light",
  "solarized",
  "solarized-light",
  "kanagawa",
  "kanagawa-lotus",
  "rose-pine",
  "rose-pine-dawn",
  "vesper",
] as const;

describe("theme catalog", () => {
  it("includes every built-in theme and keeps VS Code Dark+ as default", () => {
    for (const id of builtInThemeIds) assert.ok(isThemeId(id), `${id} is missing`);
    assert.equal(DEFAULT_THEME_ID, "vscode-dark");
    assert.equal(THEMES[DEFAULT_THEME_ID].name, "VS Code Dark+");
  });

  it("provides complete UI and xterm colors for every theme", () => {
    const hex = /^#[\da-f]{3}(?:[\da-f]{3})?$/i;
    for (const id of THEME_IDS) {
      const theme = THEMES[id];
      for (const color of Object.values(theme.terminal)) {
        assert.match(color, hex, `${id} has an invalid terminal color: ${color}`);
      }
      assert.ok(theme.ui.appBg);
      assert.ok(theme.ui.separator);
      assert.ok(theme.ui.accent);
      assert.ok(themeStyle(theme).includes(`--terminal-background:${theme.terminal.background}`));
    }
  });

  it("picks the higher-contrast on-accent foreground", () => {
    assert.equal(onAccentFor("#0e639c"), "#ffffff");
    assert.equal(onAccentFor("#bd93f9"), "#1e1e1e");
    assert.equal(onAccentFor("#88c0d0"), "#1e1e1e");
    assert.equal(onAccentFor("#0969da"), "#ffffff");
  });

  it("keeps accent-filled button text readable on every theme", () => {
    for (const id of THEME_IDS) {
      const theme = THEMES[id];
      const ratio = contrastRatio(theme.ui.accent, theme.ui.onAccent);
      assert.ok(
        ratio >= 4,
        `${id} on-accent contrast too low: ${ratio.toFixed(2)}`,
      );
      assert.ok(
        themeStyle(theme).includes(`--on-accent:${theme.ui.onAccent}`),
        `${id} is missing --on-accent`,
      );
    }
  });
});

describe("withAlpha", () => {
  it("adds an alpha channel to 6-digit hex", () => {
    assert.equal(withAlpha("#1e1e1e", 0.8), "rgba(30, 30, 30, 0.8)");
    assert.equal(withAlpha("#FF0000", 0.5), "rgba(255, 0, 0, 0.5)");
  });

  it("clamps and rounds the alpha", () => {
    assert.equal(withAlpha("#1e1e1e", 2), "#1e1e1e");
    assert.equal(withAlpha("#1e1e1e", 1), "#1e1e1e");
    assert.equal(withAlpha("#1e1e1e", 0.333), "rgba(30, 30, 30, 0.33)");
    assert.equal(withAlpha("#1e1e1e", -1), "rgba(30, 30, 30, 0)");
  });

  it("passes anything unparseable through untouched", () => {
    assert.equal(withAlpha("red", 0.5), "red");
    assert.equal(withAlpha("#abc", 0.5), "#abc");
    assert.equal(withAlpha("", 0.5), "");
  });

  it("themeStyle applies alpha only to the background stack", () => {
    const theme = THEMES[DEFAULT_THEME_ID];
    const styled = themeStyle(theme, 0.8);
    assert.ok(styled.includes(`--app-bg:rgba(30, 30, 30, 0.8)`));
    assert.ok(styled.includes(`--pane-bg:rgba(30, 30, 30, 0.8)`));
    assert.ok(
      styled.includes(`--terminal-background:rgba(30, 30, 30, 0.8)`),
    );
    assert.ok(styled.includes(`--sidebar-bg:${theme.ui.sidebarBg}`));
    assert.equal(themeStyle(theme), themeStyle(theme, 1));
  });
});

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_THEME_ID,
  THEMES,
  THEME_IDS,
  isThemeId,
  themeStyle,
  withAlpha,
} from "../src/lib/themes.ts";

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

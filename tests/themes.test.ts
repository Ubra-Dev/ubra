import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_THEME_ID,
  THEMES,
  THEME_IDS,
  isThemeId,
  themeStyle,
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

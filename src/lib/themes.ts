export interface UiColors {
  colorScheme: "dark" | "light";
  appBg: string;
  sidebarBg: string;
  tabbarBg: string;
  paneBg: string;
  paneHeaderBg: string;
  surfaceBg: string;
  surfaceHover: string;
  surfaceActive: string;
  border: string;
  separator: string;
  text: string;
  textStrong: string;
  textMuted: string;
  textSubtle: string;
  accent: string;
  accentHover: string;
  inputBg: string;
  inputBorder: string;
  agentText: string;
  agentBg: string;
  success: string;
  attention: string;
  attentionBg: string;
  attentionHoverBg: string;
  errorBg: string;
  errorHoverBg: string;
  errorText: string;
  scrollbarThumb: string;
  shadowColor: string;
}

export interface TerminalColors {
  background: string;
  foreground: string;
  cursor: string;
  cursorAccent: string;
  selectionBackground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

export interface ThemeDefinition {
  name: string;
  ui: UiColors;
  terminal: TerminalColors;
}

interface HerdrPalette {
  scheme: "dark" | "light";
  background: string;
  sidebar: string;
  active: string;
  selection: string;
  surface: string;
  surfaceHover: string;
  separator: string;
  text: string;
  subtext: string;
  muted: string;
  subtle: string;
  accent: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  peach: string;
}

function adjustHexColor(color: string, amount: number): string {
  const value = Number.parseInt(color.slice(1), 16);
  const adjust = (shift: number) =>
    Math.max(0, Math.min(255, Math.round(((value >> shift) & 0xff) + amount * 255)));
  return `#${[16, 8, 0]
    .map((shift) => adjust(shift).toString(16).padStart(2, "0"))
    .join("")}`;
}

function mixHexColors(first: string, second: string, amount: number): string {
  const a = Number.parseInt(first.slice(1), 16);
  const b = Number.parseInt(second.slice(1), 16);
  const mix = (shift: number) =>
    Math.round(
      ((a >> shift) & 0xff) * (1 - amount) + ((b >> shift) & 0xff) * amount,
    );
  return `#${[16, 8, 0]
    .map((shift) => mix(shift).toString(16).padStart(2, "0"))
    .join("")}`;
}

function makeHerdrTheme(name: string, palette: HerdrPalette): ThemeDefinition {
  const { scheme, background, sidebar, active, selection, surface, surfaceHover,
    separator, text, subtext, muted, subtle, accent, red, green, yellow, blue,
    magenta, cyan, peach } = palette;
  const brighten = (color: string) => adjustHexColor(color, scheme === "dark" ? 0.12 : -0.12);
  const ansi: TerminalColors = {
    background,
    foreground: text,
    cursor: accent,
    cursorAccent: background,
    selectionBackground: selection,
    black: scheme === "dark" ? adjustHexColor(background, -0.08) : text,
    red,
    green,
    yellow,
    blue,
    magenta,
    cyan,
    white: text,
    brightBlack: muted,
    brightRed: brighten(red),
    brightGreen: brighten(green),
    brightYellow: brighten(yellow),
    brightBlue: brighten(blue),
    brightMagenta: brighten(magenta),
    brightCyan: brighten(cyan),
    brightWhite: scheme === "dark" ? "#ffffff" : background,
  };

  return {
    name,
    ui: {
      colorScheme: scheme,
      appBg: background,
      sidebarBg: sidebar,
      tabbarBg: sidebar,
      paneBg: background,
      paneHeaderBg: surface,
      surfaceBg: surface,
      surfaceHover,
      surfaceActive: active,
      border: surfaceHover,
      separator,
      text,
      textStrong: text,
      textMuted: subtext,
      textSubtle: subtle,
      accent,
      accentHover: brighten(accent),
      inputBg: scheme === "dark" ? sidebar : brighten(background),
      inputBorder: accent,
      agentText: cyan,
      agentBg: surface,
      success: green,
      attention: yellow,
      attentionBg: active,
      attentionHoverBg: surfaceHover,
      errorBg: mixHexColors(background, red, scheme === "dark" ? 0.24 : 0.12),
      errorHoverBg: red,
      errorText: scheme === "dark" ? brighten(red) : red,
      scrollbarThumb: muted,
      shadowColor: scheme === "dark"
        ? "rgba(0, 0, 0, 0.45)"
        : "rgba(35, 40, 50, 0.18)",
    },
    terminal: ansi,
  };
}

export const THEMES = {
  "vscode-dark": {
    name: "VS Code Dark+",
    ui: {
      colorScheme: "dark",
      appBg: "#1e1e1e",
      sidebarBg: "#252526",
      tabbarBg: "#252526",
      paneBg: "#1e1e1e",
      paneHeaderBg: "#2d2d2d",
      surfaceBg: "#2d2d2d",
      surfaceHover: "#3e3e3e",
      surfaceActive: "#3e3e3e",
      border: "#454545",
      separator: "#494949",
      text: "#bbb",
      textStrong: "#fff",
      textMuted: "#888",
      textSubtle: "#666",
      accent: "#0e639c",
      accentHover: "#1177bb",
      inputBg: "#1e1e1e",
      inputBorder: "#0e639c",
      agentText: "#9cdcfe",
      agentBg: "#14324a",
      success: "#89d185",
      attention: "#e5c07b",
      attentionBg: "#5a4a1d",
      attentionHoverBg: "#6b5a24",
      errorBg: "#5a1d1d",
      errorHoverBg: "#a1260d",
      errorText: "#f0b0b0",
      scrollbarThumb: "#555",
      shadowColor: "rgba(0, 0, 0, 0.5)",
    },
    terminal: {
      background: "#1e1e1e",
      foreground: "#d4d4d4",
      cursor: "#aeafad",
      cursorAccent: "#1e1e1e",
      selectionBackground: "#264f78",
      black: "#000000",
      red: "#cd3131",
      green: "#0dbc79",
      yellow: "#e5e510",
      blue: "#2472c8",
      magenta: "#bc3fbc",
      cyan: "#11a8cd",
      white: "#e5e5e5",
      brightBlack: "#666666",
      brightRed: "#f14c4c",
      brightGreen: "#23d18b",
      brightYellow: "#f5f543",
      brightBlue: "#3b8eea",
      brightMagenta: "#d670d6",
      brightCyan: "#29b8db",
      brightWhite: "#e5e5e5",
    },
  },
  dracula: {
    name: "Dracula",
    ui: {
      colorScheme: "dark",
      appBg: "#282a36",
      sidebarBg: "#21222c",
      tabbarBg: "#21222c",
      paneBg: "#282a36",
      paneHeaderBg: "#343746",
      surfaceBg: "#343746",
      surfaceHover: "#44475a",
      surfaceActive: "#44475a",
      border: "#44475a",
      separator: "#505365",
      text: "#e3e4eb",
      textStrong: "#f8f8f2",
      textMuted: "#bdc2d6",
      textSubtle: "#8b92ad",
      accent: "#bd93f9",
      accentHover: "#d6acff",
      inputBg: "#21222c",
      inputBorder: "#bd93f9",
      agentText: "#8be9fd",
      agentBg: "#343746",
      success: "#50fa7b",
      attention: "#f1fa8c",
      attentionBg: "#494632",
      attentionHoverBg: "#5b573b",
      errorBg: "#512e3b",
      errorHoverBg: "#b13e53",
      errorText: "#ffb8c6",
      scrollbarThumb: "#55586b",
      shadowColor: "rgba(0, 0, 0, 0.45)",
    },
    terminal: {
      background: "#282a36",
      foreground: "#f8f8f2",
      cursor: "#f8f8f0",
      cursorAccent: "#282a36",
      selectionBackground: "#44475a",
      black: "#21222c",
      red: "#ff5555",
      green: "#50fa7b",
      yellow: "#f1fa8c",
      blue: "#bd93f9",
      magenta: "#ff79c6",
      cyan: "#8be9fd",
      white: "#f8f8f2",
      brightBlack: "#6272a4",
      brightRed: "#ff6e6e",
      brightGreen: "#69ff94",
      brightYellow: "#ffffa5",
      brightBlue: "#d6acff",
      brightMagenta: "#ff92df",
      brightCyan: "#a4ffff",
      brightWhite: "#ffffff",
    },
  },
  nord: {
    name: "Nord",
    ui: {
      colorScheme: "dark",
      appBg: "#2e3440",
      sidebarBg: "#272d38",
      tabbarBg: "#272d38",
      paneBg: "#2e3440",
      paneHeaderBg: "#3b4252",
      surfaceBg: "#3b4252",
      surfaceHover: "#434c5e",
      surfaceActive: "#434c5e",
      border: "#4c566a",
      separator: "#596477",
      text: "#d8dee9",
      textStrong: "#eceff4",
      textMuted: "#b8c1d1",
      textSubtle: "#8792a3",
      accent: "#88c0d0",
      accentHover: "#8fbcbb",
      inputBg: "#272d38",
      inputBorder: "#88c0d0",
      agentText: "#8fbcbb",
      agentBg: "#3b4252",
      success: "#a3be8c",
      attention: "#ebcb8b",
      attentionBg: "#4d493d",
      attentionHoverBg: "#5c5646",
      errorBg: "#513d42",
      errorHoverBg: "#bf616a",
      errorText: "#efb8b8",
      scrollbarThumb: "#596477",
      shadowColor: "rgba(0, 0, 0, 0.4)",
    },
    terminal: {
      background: "#2e3440",
      foreground: "#d8dee9",
      cursor: "#d8dee9",
      cursorAccent: "#2e3440",
      selectionBackground: "#434c5e",
      black: "#3b4252",
      red: "#bf616a",
      green: "#a3be8c",
      yellow: "#ebcb8b",
      blue: "#81a1c1",
      magenta: "#b48ead",
      cyan: "#88c0d0",
      white: "#e5e9f0",
      brightBlack: "#4c566a",
      brightRed: "#bf616a",
      brightGreen: "#a3be8c",
      brightYellow: "#ebcb8b",
      brightBlue: "#81a1c1",
      brightMagenta: "#b48ead",
      brightCyan: "#8fbcbb",
      brightWhite: "#eceff4",
    },
  },
  solarized: makeHerdrTheme("Solarized", {
    scheme: "dark", background: "#002b36", sidebar: "#073642",
    active: "#164b57", selection: "#083e55", surface: "#073642",
    surfaceHover: "#586e75", separator: "#002b36", text: "#93a1a1",
    subtext: "#839496", muted: "#586e75", subtle: "#657b83",
    accent: "#268bd2", red: "#dc322f", green: "#859900",
    yellow: "#b58900", blue: "#268bd2", magenta: "#d33682",
    cyan: "#2aa198", peach: "#cb4b16",
  }),
  light: {
    name: "Light",
    ui: {
      colorScheme: "light",
      appBg: "#ffffff",
      sidebarBg: "#f6f8fa",
      tabbarBg: "#f6f8fa",
      paneBg: "#ffffff",
      paneHeaderBg: "#f6f8fa",
      surfaceBg: "#f6f8fa",
      surfaceHover: "#eaeef2",
      surfaceActive: "#eaeef2",
      border: "#d0d7de",
      separator: "#d8dee4",
      text: "#24292f",
      textStrong: "#1f2328",
      textMuted: "#57606a",
      textSubtle: "#6e7781",
      accent: "#0969da",
      accentHover: "#0550ae",
      inputBg: "#ffffff",
      inputBorder: "#0969da",
      agentText: "#0550ae",
      agentBg: "#ddf4ff",
      success: "#1a7f37",
      attention: "#9a6700",
      attentionBg: "#fff8c5",
      attentionHoverBg: "#ffef9a",
      errorBg: "#ffebe9",
      errorHoverBg: "#cf222e",
      errorText: "#cf222e",
      scrollbarThumb: "#afb8c1",
      shadowColor: "rgba(31, 35, 40, 0.18)",
    },
    terminal: {
      background: "#ffffff",
      foreground: "#24292f",
      cursor: "#24292f",
      cursorAccent: "#ffffff",
      selectionBackground: "#add6ff",
      black: "#24292f",
      red: "#cf222e",
      green: "#1a7f37",
      yellow: "#9a6700",
      blue: "#0969da",
      magenta: "#8250df",
      cyan: "#1b7c83",
      white: "#6e7781",
      brightBlack: "#57606a",
      brightRed: "#a40e26",
      brightGreen: "#116329",
      brightYellow: "#7d4e00",
      brightBlue: "#0550ae",
      brightMagenta: "#6639ba",
      brightCyan: "#055160",
      brightWhite: "#ffffff",
    },
  },
  catppuccin: makeHerdrTheme("Catppuccin", {
    scheme: "dark", background: "#181825", sidebar: "#181825",
    active: "#1e1e2e", selection: "#313244", surface: "#313244",
    surfaceHover: "#45475a", separator: "#1e1e2e", text: "#cdd6f4",
    subtext: "#a6adc8", muted: "#6c7086", subtle: "#7f849c",
    accent: "#89b4fa", red: "#f38ba8", green: "#a6e3a1",
    yellow: "#f9e2af", blue: "#89b4fa", magenta: "#cba6f7",
    cyan: "#94e2d5", peach: "#fab387",
  }),
  "catppuccin-latte": makeHerdrTheme("Catppuccin Latte", {
    scheme: "light", background: "#eff1f5", sidebar: "#eff1f5",
    active: "#e6e9ef", selection: "#bdd0f5", surface: "#ccd0da",
    surfaceHover: "#bcc0cc", separator: "#e6e9ef", text: "#4c4f69",
    subtext: "#6c6f85", muted: "#9ca0b0", subtle: "#8c8fa1",
    accent: "#1e66f5", red: "#d20f39", green: "#40a02b",
    yellow: "#df8e1d", blue: "#1e66f5", magenta: "#8839ef",
    cyan: "#179299", peach: "#fe640b",
  }),
  "tokyo-night": makeHerdrTheme("Tokyo Night", {
    scheme: "dark", background: "#1a1b26", sidebar: "#1a1b26",
    active: "#232636", selection: "#2d3650", surface: "#24283b",
    surfaceHover: "#414868", separator: "#1a1b26", text: "#c0caf5",
    subtext: "#a9b1d6", muted: "#565f89", subtle: "#697196",
    accent: "#7aa2f7", red: "#f7768e", green: "#9ece6a",
    yellow: "#e0af68", blue: "#7aa2f7", magenta: "#bb9af7",
    cyan: "#7dcfff", peach: "#ff9e64",
  }),
  "tokyo-night-day": makeHerdrTheme("Tokyo Night Day", {
    scheme: "light", background: "#e1e2e7", sidebar: "#e1e2e7",
    active: "#d2d3da", selection: "#b6cae7", surface: "#c4c8da",
    surfaceHover: "#a8aecb", separator: "#d2d3da", text: "#3760bf",
    subtext: "#6172b0", muted: "#8990b3", subtle: "#68709a",
    accent: "#2e7de9", red: "#f52a65", green: "#587539",
    yellow: "#8c6c3e", blue: "#2e7de9", magenta: "#7847bd",
    cyan: "#118c74", peach: "#b15c00",
  }),
  gruvbox: makeHerdrTheme("Gruvbox", {
    scheme: "dark", background: "#282828", sidebar: "#282828",
    active: "#323130", selection: "#4b3f27", surface: "#3c3836",
    surfaceHover: "#504945", separator: "#282828", text: "#ebdbb2",
    subtext: "#d5c4a1", muted: "#928374", subtle: "#a89984",
    accent: "#d79921", red: "#fb4934", green: "#b8bb26",
    yellow: "#fabd2f", blue: "#83a598", magenta: "#d3869b",
    cyan: "#8ec07c", peach: "#fe8019",
  }),
  "gruvbox-light": makeHerdrTheme("Gruvbox Light", {
    scheme: "light", background: "#fbf1c7", sidebar: "#fbf1c7",
    active: "#f2e5bc", selection: "#ebdbb2", surface: "#ebdbb2",
    surfaceHover: "#d5c4a1", separator: "#f2e5bc", text: "#3c3836",
    subtext: "#504945", muted: "#928374", subtle: "#7c6f64",
    accent: "#076678", red: "#9d0006", green: "#79740e",
    yellow: "#b57614", blue: "#076678", magenta: "#8f3f71",
    cyan: "#427b58", peach: "#af3a03",
  }),
  "one-dark": makeHerdrTheme("One Dark", {
    scheme: "dark", background: "#282c34", sidebar: "#282c34",
    active: "#313640", selection: "#334659", surface: "#2c313a",
    surfaceHover: "#3e4451", separator: "#282c34", text: "#abb2bf",
    subtext: "#969ca8", muted: "#5c6370", subtle: "#737a87",
    accent: "#61afef", red: "#e06c75", green: "#98c379",
    yellow: "#e5c07b", blue: "#61afef", magenta: "#c678dd",
    cyan: "#56b6c2", peach: "#d19a66",
  }),
  "one-light": makeHerdrTheme("One Light", {
    scheme: "light", background: "#fafafa", sidebar: "#f5f5f6",
    active: "#d8dbe2", selection: "#cddbf8",
    surface: "#f0f0f1", surfaceHover: "#e5e5e6", separator: "#f5f5f6",
    text: "#383a42", subtext: "#686b77", muted: "#a0a1a7",
    subtle: "#686b77", accent: "#4078f2", red: "#e45649",
    green: "#50a14f", yellow: "#c18401", blue: "#4078f2",
    magenta: "#a626a4", cyan: "#0184bc", peach: "#986801",
  }),
  "solarized-light": makeHerdrTheme("Solarized Light", {
    scheme: "light", background: "#fdf6e3", sidebar: "#fdf6e3",
    active: "#eee8d5", selection: "#c9dcdf", surface: "#eee8d5",
    surfaceHover: "#93a1a1", separator: "#eee8d5", text: "#657b83",
    subtext: "#839496", muted: "#93a1a1", subtle: "#586e75",
    accent: "#268bd2", red: "#dc322f", green: "#859900",
    yellow: "#b58900", blue: "#268bd2", magenta: "#d33682",
    cyan: "#2aa198", peach: "#cb4b16",
  }),
  kanagawa: makeHerdrTheme("Kanagawa", {
    scheme: "dark", background: "#1f1f28", sidebar: "#1f1f28",
    active: "#363646", selection: "#32384b", surface: "#2a2a37",
    surfaceHover: "#363646", separator: "#1f1f28", text: "#dcd7ba",
    subtext: "#c8c3aa", muted: "#727169", subtle: "#87867d",
    accent: "#7e9cd8", red: "#c34043", green: "#76946a",
    yellow: "#c0a36e", blue: "#7e9cd8", magenta: "#957fb8",
    cyan: "#7fb4ca", peach: "#ffa066",
  }),
  "kanagawa-lotus": makeHerdrTheme("Kanagawa Lotus", {
    scheme: "light", background: "#f2ecbc", sidebar: "#f2ecbc",
    active: "#d5cea3", selection: "#dcd5ac", surface: "#dcd5ac",
    surfaceHover: "#c9cbd1", separator: "#d5cea3", text: "#545464",
    subtext: "#43436c", muted: "#a09cac", subtle: "#8a8980",
    accent: "#4d699b", red: "#c84053", green: "#6f894e",
    yellow: "#77713f", blue: "#4d699b", magenta: "#624c83",
    cyan: "#4e8ca2", peach: "#cc6d00",
  }),
  "rose-pine": makeHerdrTheme("Rosé Pine", {
    scheme: "dark", background: "#191724", sidebar: "#191724",
    active: "#26233a", selection: "#3b344b", surface: "#1f1d2e",
    surfaceHover: "#26233a", separator: "#26233a", text: "#e0def4",
    subtext: "#c8c5dc", muted: "#6e6a86", subtle: "#908caa",
    accent: "#c4a7e7", red: "#eb6f92", green: "#31748f",
    yellow: "#f6c177", blue: "#31748f", magenta: "#c4a7e7",
    cyan: "#9ccfd8", peach: "#ea9a97",
  }),
  "rose-pine-dawn": makeHerdrTheme("Rosé Pine Dawn", {
    scheme: "light", background: "#faf4ed", sidebar: "#faf4ed",
    active: "#e3d9cf", selection: "#f2e9e1", surface: "#f2e9e1",
    surfaceHover: "#fffaf3", separator: "#f2e9e1", text: "#464261",
    subtext: "#797593", muted: "#9893a5", subtle: "#797593",
    accent: "#907aa9", red: "#b4637a", green: "#286983",
    yellow: "#ea9d34", blue: "#286983", magenta: "#907aa9",
    cyan: "#56949f", peach: "#d7827e",
  }),
  vesper: makeHerdrTheme("Vesper", {
    scheme: "dark", background: "#1a1a1a", sidebar: "#1a1a1a",
    active: "#101010", selection: "#232323", surface: "#232323",
    surfaceHover: "#282828", separator: "#101010", text: "#ffffff",
    subtext: "#a0a0a0", muted: "#5c5c5c", subtle: "#7e7e7e",
    accent: "#ffc799", red: "#ff8080", green: "#99ffe4",
    yellow: "#ffc799", blue: "#b0b0b0", magenta: "#ffd1a8",
    cyan: "#66ddcc", peach: "#ffc799",
  }),
} satisfies Record<string, ThemeDefinition>;

export type ThemeId = keyof typeof THEMES;
export type AppTheme = (typeof THEMES)[ThemeId];
export const DEFAULT_THEME_ID: ThemeId = "vscode-dark";

export const THEME_IDS = Object.keys(THEMES) as ThemeId[];

export function isThemeId(value: unknown): value is ThemeId {
  return typeof value === "string" && Object.hasOwn(THEMES, value);
}

export function themeStyle(theme: AppTheme): string {
  const appVariables = Object.entries(theme.ui).map(
    ([key, value]) => `--${key.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`)}:${value}`,
  );
  appVariables.push(
    `--terminal-background:${theme.terminal.background}`,
    `--terminal-foreground:${theme.terminal.foreground}`,
  );
  return appVariables.join(";");
}

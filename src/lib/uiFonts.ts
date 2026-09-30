/**
 * Curated Google Fonts (bundled via Fontsource) available for the interface.
 *
 * This module is deliberately pure: it carries the catalog and its helpers but
 * no font CSS imports, so unit tests can import it under plain node. The
 * actual Fontsource CSS is loaded once via `./uiFontFaces`.
 */
export interface UiFontDefinition {
  /** Display name shown in settings. */
  name: string;
  /** Short style category shown next to the name and matched by search. */
  category: string;
  /**
   * Full font stack: the bundled variable family first, system fonts last so
   * text still renders if the webfont fails to load.
   */
  stack: string;
}

export const UI_FONTS = {
  system: {
    name: "System default",
    category: "System",
    stack: "system-ui, sans-serif",
  },
  silkscreen: {
    name: "Silkscreen",
    category: "Pixel",
    stack: "'Silkscreen', system-ui, sans-serif",
  },
  "dm-sans": {
    name: "DM Sans",
    category: "Geometric",
    stack: "'DM Sans Variable', system-ui, sans-serif",
  },
  figtree: {
    name: "Figtree",
    category: "Geometric",
    stack: "'Figtree Variable', system-ui, sans-serif",
  },
  "ibm-plex-sans": {
    name: "IBM Plex Sans",
    category: "Grotesque",
    stack: "'IBM Plex Sans Variable', system-ui, sans-serif",
  },
  inter: {
    name: "Inter",
    category: "Grotesque",
    stack: "'Inter Variable', system-ui, sans-serif",
  },
  manrope: {
    name: "Manrope",
    category: "Geometric",
    stack: "'Manrope Variable', system-ui, sans-serif",
  },
  montserrat: {
    name: "Montserrat",
    category: "Geometric",
    stack: "'Montserrat Variable', system-ui, sans-serif",
  },
  nunito: {
    name: "Nunito",
    category: "Rounded",
    stack: "'Nunito Variable', system-ui, sans-serif",
  },
  "open-sans": {
    name: "Open Sans",
    category: "Humanist",
    stack: "'Open Sans Variable', system-ui, sans-serif",
  },
  outfit: {
    name: "Outfit",
    category: "Geometric",
    stack: "'Outfit Variable', system-ui, sans-serif",
  },
  "plus-jakarta-sans": {
    name: "Plus Jakarta Sans",
    category: "Geometric",
    stack: "'Plus Jakarta Sans Variable', system-ui, sans-serif",
  },
  "public-sans": {
    name: "Public Sans",
    category: "Humanist",
    stack: "'Public Sans Variable', system-ui, sans-serif",
  },
  raleway: {
    name: "Raleway",
    category: "Geometric",
    stack: "'Raleway Variable', system-ui, sans-serif",
  },
  "roboto-flex": {
    name: "Roboto Flex",
    category: "Grotesque",
    stack: "'Roboto Flex Variable', system-ui, sans-serif",
  },
  "source-sans-3": {
    name: "Source Sans 3",
    category: "Humanist",
    stack: "'Source Sans 3 Variable', system-ui, sans-serif",
  },
  urbanist: {
    name: "Urbanist",
    category: "Geometric",
    stack: "'Urbanist Variable', system-ui, sans-serif",
  },
  "work-sans": {
    name: "Work Sans",
    category: "Grotesque",
    stack: "'Work Sans Variable', system-ui, sans-serif",
  },
} satisfies Record<string, UiFontDefinition>;

export type UiFontId = keyof typeof UI_FONTS;

export const DEFAULT_UI_FONT_ID: UiFontId = "system";

export const UI_FONT_IDS = Object.keys(UI_FONTS) as UiFontId[];

export function isUiFontId(value: unknown): value is UiFontId {
  return typeof value === "string" && Object.hasOwn(UI_FONTS, value);
}

/** Inline-style declaration consumed by the app root (`--font-ui`). */
export function uiFontStyle(id: UiFontId): string {
  return `--font-ui:${UI_FONTS[id].stack}`;
}

/**
 * Catalog ids matching a settings search query (case-insensitive substring
 * over name and category). A blank query returns the whole catalog.
 */
export function matchUiFonts(query: string): UiFontId[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...UI_FONT_IDS];
  return UI_FONT_IDS.filter((id) => {
    const font = UI_FONTS[id];
    return (
      font.name.toLowerCase().includes(needle) ||
      font.category.toLowerCase().includes(needle)
    );
  });
}

/**
 * Brand gradient for the loading animation, sampled left-to-right from
 * `static/logo.png`: cyan → blue → purple → magenta → pink → orange → yellow.
 *
 * This module is deliberately pure: it carries the stops and their helpers
 * but no CSS imports, so unit tests can import it under plain node. The
 * animated rendering lives in `./LoadingText.svelte`.
 */

/** Hex stops of the Ubra logo gradient, left to right. */
export const BRAND_GRADIENT_STOPS = [
  "#01eafc",
  "#0178fc",
  "#7717fc",
  "#ff00ff",
  "#fc3672",
  "#fb7c36",
  "#fcce05",
] as const;

/** Default text for the loading animation when the caller passes none. */
export const DEFAULT_LOADING_TEXT = "Loading...";

/**
 * `background-image` value for the brand gradient at the given angle.
 * Evenly spaced stops; the animation oversizes the background and slides
 * its position, so the gradient itself stays a plain linear fill.
 */
export function brandGradientImage(angleDeg = 90): string {
  return `linear-gradient(${angleDeg}deg, ${[...BRAND_GRADIENT_STOPS].join(", ")})`;
}

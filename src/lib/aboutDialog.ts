import type { JsImage } from "@tauri-apps/api/image";
import type { AboutMetadata } from "@tauri-apps/api/menu";
import { SITE_URL } from "./site.ts";

export const ABOUT_COPYRIGHT = "© 2026 Ubra";
export const ABOUT_BLURB = "Agent runtime desktop app";
export const ABOUT_LICENSE = "MIT";
export const ABOUT_WEBSITE = SITE_URL;
export const ABOUT_WEBSITE_LABEL = "Website";

/**
 * Full metadata for the native About dialog. macOS shows
 * name/version/copyright/credits/icon; the rest serves Windows/Linux.
 */
export function aboutMetadata(
  name: string,
  version: string,
  icon?: JsImage,
): AboutMetadata {
  return {
    name,
    version,
    copyright: ABOUT_COPYRIGHT,
    credits: ABOUT_BLURB,
    comments: ABOUT_BLURB,
    license: ABOUT_LICENSE,
    website: ABOUT_WEBSITE,
    websiteLabel: ABOUT_WEBSITE_LABEL,
    ...(icon === undefined ? {} : { icon }),
  };
}

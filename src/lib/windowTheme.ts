import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Mirror the app theme's scheme on the native titlebar. Best-effort: a
 * failure only logs, so a denied capability never breaks theming.
 */
export async function syncWindowTheme(scheme: "dark" | "light"): Promise<void> {
  try {
    await getCurrentWindow().setTheme(scheme);
  } catch (e) {
    console.error("ubra: window theme sync failed", e);
  }
}

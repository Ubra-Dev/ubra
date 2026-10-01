import { WebglAddon } from "@xterm/addon-webgl";
import type { Terminal } from "@xterm/xterm";
import { frameBudgetQueue, type FrameBudgetQueue } from "./schedule.ts";

/**
 * Shared GPU-upgrade queue: revealing a multi-pane workspace re-acquires one
 * WebGL context per pane, so upgrades drain at most two per frame instead of
 * all landing in the reveal frame. Keyed by pane session key (latest wins).
 */
export const gpuUpgradeQueue: FrameBudgetQueue = frameBudgetQueue(2);

/**
 * Probe for WebGL2 support. Injectable so unit tests can run without a DOM;
 * production passes nothing and uses {@link defaultWebGl2Probe}.
 */
export type WebGl2Probe = () => boolean;

export interface GpuRendererDeps {
  probe?: WebGl2Probe;
  createAddon?: () => WebglAddon;
}

/** True when this webview can create a WebGL2 context. Never throws. */
export function defaultWebGl2Probe(): boolean {
  if (typeof document === "undefined") return false;
  try {
    return document.createElement("canvas").getContext("webgl2") !== null;
  } catch {
    return false;
  }
}

/**
 * Attempt GPU terminal rendering on an already-opened terminal. Returns the
 * addon, or null when WebGL2 is unavailable or activation fails — the caller
 * then stays on xterm's built-in renderer. Wires context loss to dispose, so
 * a reclaimed context also falls back instead of going blank.
 */
export function enableGpuRenderer(
  term: Terminal,
  deps: GpuRendererDeps = {},
): WebglAddon | null {
  const probe = deps.probe ?? defaultWebGl2Probe;
  if (!probe()) return null;
  let addon: WebglAddon | null = null;
  try {
    addon = deps.createAddon ? deps.createAddon() : new WebglAddon();
    term.loadAddon(addon);
  } catch (error) {
    console.error("ubra: GPU terminal renderer unavailable, using canvas", error);
    if (addon) {
      try {
        addon.dispose();
      } catch {
        // Best-effort cleanup of a half-activated addon.
      }
    }
    return null;
  }
  const active: WebglAddon = addon;
  active.onContextLoss(() => active.dispose());
  return active;
}

/** Dispose a GPU renderer if present. Always returns null for easy reset. */
export function disableGpuRenderer(addon: WebglAddon | null): null {
  if (addon) {
    try {
      addon.dispose();
    } catch (error) {
      console.error("ubra: failed to dispose GPU terminal renderer", error);
    }
  }
  return null;
}

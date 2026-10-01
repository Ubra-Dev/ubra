import { countPanes, type Layout } from "./layout.ts";

/** Remembered Quit choice. `ask` shows the confirmation dialog. */
export type QuitAction = "ask" | "keep" | "stop";

export const QUIT_ACTION_KEY = "ubra.quitAction";

export function parseQuitAction(raw: string | null): QuitAction {
  return raw === "keep" || raw === "stop" ? raw : "ask";
}

export function countLayoutPanes(layout: Layout | null): number {
  if (!layout) return 0;
  return layout.workspaces.reduce(
    (total, ws) =>
      total + ws.tabs.reduce((sum, tab) => sum + countPanes(tab.root), 0),
    0,
  );
}

/** How a Quit request resolves: show the `dialog`, or quit at once. */
export type QuitResolution = "dialog" | "keep" | "stop";

export function resolveQuitRequest(
  action: QuitAction,
  paneCount: number,
): QuitResolution {
  // Nothing survives, so nothing needs a warning: plain quit.
  if (paneCount <= 0) return "keep";
  if (action === "keep") return "keep";
  if (action === "stop") return "stop";
  return "dialog";
}

export interface QuitDialogCopy {
  title: string;
  detail: string;
}

export function quitDialogCopy(panes: number, agents: number): QuitDialogCopy {
  const paneWord = panes === 1 ? "pane" : "panes";
  const agentWord = agents === 1 ? "agent" : "agents";
  const detail =
    agents > 0
      ? `${panes} terminal ${paneWord} (${agents} ${agentWord}) will keep running in the background. Reopen Ubra to reattach, or stop every agent first.`
      : `${panes} terminal ${paneWord} will keep running in the background. Reopen Ubra to reattach, or stop every process first.`;
  return { title: "Quit Ubra?", detail };
}

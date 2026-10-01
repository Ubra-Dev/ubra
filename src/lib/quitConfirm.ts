import { countPanes, type Layout } from "./layout.ts";

/** Remembered Quit choice. `ask` shows the confirmation dialog. */
export type QuitAction = "ask" | "quit";

export const QUIT_ACTION_KEY = "ubra.quitAction";

export function parseQuitAction(raw: string | null): QuitAction {
  // Legacy "stop" skipped the dialog and stopped every pane, exactly what
  // "quit" does now. Legacy "keep" promised survival, which no longer
  // exists, so it falls through to "ask".
  if (raw === "quit" || raw === "stop") return "quit";
  return "ask";
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
export type QuitResolution = "dialog" | "quit";

export function resolveQuitRequest(
  action: QuitAction,
  paneCount: number,
): QuitResolution {
  // Nothing open, so nothing needs a warning: plain quit.
  if (paneCount <= 0) return "quit";
  if (action === "quit") return "quit";
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
      ? `${panes} terminal ${paneWord} (${agents} ${agentWord}) will stop. Quitting ends every process.`
      : `${panes} terminal ${paneWord} will stop. Quitting ends every process.`;
  return { title: "Quit Ubra?", detail };
}

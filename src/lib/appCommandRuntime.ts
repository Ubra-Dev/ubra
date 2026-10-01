import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { canDispatch, type CommandContext, type CommandRequest } from "./appCommands";
import { agent } from "./agent.svelte";
import { isEditableTarget } from "./shortcuts";
import { findPane } from "./layout";
import { store } from "./store.svelte";
import { ISSUES_URL, REPO_URL } from "./site.ts";
import { terminalCommands } from "./terminalCommands";
import { toasts } from "./toasts.svelte.ts";

function focusedElement(): HTMLElement | null {
  const el = document.activeElement;
  return el instanceof HTMLElement && !el.closest("[inert]") ? el : null;
}

function selectedText(el: HTMLElement | null): string {
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    return el.value.slice(el.selectionStart ?? 0, el.selectionEnd ?? 0);
  }
  return el?.isContentEditable ? document.getSelection()?.toString() ?? "" : "";
}

export function commandContext(): CommandContext {
  const el = focusedElement();
  const terminalFocus = !!el?.closest(".xterm");
  const textFocus = !terminalFocus && isEditableTarget(el);
  const pane = store.currentPane();
  const controls = terminalCommands.get(pane?.paneId);
  const blocked = store.settingsOpen || !!store.pendingClose || !!store.pendingQuit || store.firstRun ||
    store.recoveryRequired || store.recoveryBusy ||
    !!document.querySelector("[data-keyboard-overlay]");
  const writable = (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)
    ? !el.readOnly && !el.disabled : !!el?.isContentEditable;
  return {
    ready: store.loaded && !!store.layout && !store.recoveryRequired,
    blocked,
    settingsOpen: store.settingsOpen && !store.pendingClose,
    textFocus,
    writable,
    textSelection: selectedText(el).length > 0,
    terminalFocus,
    terminalAvailable: !!controls,
    terminalSelection: !!controls?.selection(),
    terminalRunning: !!controls?.running(),
    hasPane: !!pane,
    hasWorkspace: !!store.workspace(),
    tabCount: store.workspace()?.tabs.length ?? 0,
    workspaceIds: store.layout?.workspaces.map((ws) => ws.id) ?? [],
    restartable: terminalCommands.canRestart(pane?.paneId),
    savedCommand: !!pane && !!findSavedCommand(pane.paneId),
    zoomed: !!store.tab()?.zoomedPaneId,
  };
}

function findSavedCommand(id: string): boolean {
  const tab = store.tab();
  const pane = tab ? findPane(tab.root, id) : null;
  return !!pane?.cmd?.length && pane.cmdOnRestore === false;
}

function activeAgentCount(): number {
  return agent
    .activeAgents()
    .reduce((sum, group) => sum + group.agents.length, 0);
}

async function edit(action: string): Promise<void> {
  const target = focusedElement();
  const paneId = store.currentPane()?.paneId;
  const controls = terminalCommands.get(paneId);
  if (target?.closest(".xterm") && controls) {
    if (action === "copy") await writeText(controls.selection());
    else if (action === "select-all") controls.selectAll();
    else if (action === "paste") {
      const text = await readText();
      // A clipboard read may finish after its pane was closed or replaced.
      if (paneId && terminalCommands.get(paneId) === controls &&
          store.currentPane()?.paneId === paneId && canDispatch({ action: "paste" }, commandContext())) {
        controls.paste(text);
      }
    }
    return;
  }
  if (!target || !isEditableTarget(target)) return;
  target.focus({ preventScroll: true });
  if (action === "paste") {
    const text = await readText();
    if (focusedElement() !== target || !target.isConnected || !canDispatch({ action: "paste" }, commandContext())) return;
    if (!document.execCommand("insertText", false, text)) throw new Error("This text field could not accept the paste.");
  } else {
    document.execCommand(action === "select-all" ? "selectAll" : action);
  }
}

/** One guarded execution path for menu actions and keyboard shortcuts. */
export async function dispatchCommand(request: CommandRequest): Promise<boolean> {
  if (!canDispatch(request, commandContext())) return false;
  const cur = store.currentPane();
  try {
    switch (request.action) {
      case "open-settings": store.settingsOpenSection = "app"; store.settingsOpen = true; break;
      case "keyboard-shortcuts": store.settingsOpenSection = "shortcuts"; store.settingsOpen = true; break;
      case "new-workspace": case "open-project": store.addWorkspace(); break;
      case "new-empty-workspace": store.addBlankWorkspace(); break;
      case "new-tab": store.addTab(); break;
      case "close-pane-or-tab": store.requestClosePaneOrTab(); break;
      case "close-workspace": store.requestCloseWorkspace(store.workspace()!.id); break;
      case "split-right": store.splitPane(cur!.paneId, "row"); break;
      case "split-down": store.splitPane(cur!.paneId, "col"); break;
      case "toggle-zoom": store.toggleZoomPane(cur!.paneId); break;
      case "prev-tab": store.cycleTab(-1); break;
      case "next-tab": store.cycleTab(1); break;
      case "jump-tab": store.jumpTab(request.index!); break;
      case "prev-workspace": store.cycleWorkspace(-1); break;
      case "next-workspace": store.cycleWorkspace(1); break;
      case "switch-workspace": store.switchWorkspace(request.workspaceId!); break;
      case "font-bigger": store.bumpTermFontSize(1); store.bumpUiScale(1); break;
      case "font-smaller": store.bumpTermFontSize(-1); store.bumpUiScale(-1); break;
      case "font-reset": store.resetTermFontSize(); store.resetUiScale(); break;
      case "rename-pane": store.requestPaneRename(); break;
      case "find-in-pane": store.requestPaneFind(); break;
      case "focus-neighbor": if (request.dir) store.focusNeighbor(request.dir); break;
      case "swap-neighbor": if (request.dir) store.swapWithNeighbor(request.dir); break;
      case "resize-pane": if (request.dir) store.resizeFocused(request.dir); break;
      case "move-pane-to-new-tab": store.movePaneToNewTab(cur!.paneId); break;
      case "move-pane-to-new-workspace": store.movePaneToNewWorkspace(cur!.paneId); break;
      case "restart-terminal": terminalCommands.restart(cur!.paneId); break;
      case "undo": case "redo": case "cut": case "copy": case "paste": case "select-all": await edit(request.action); break;
      case "fullscreen": {
        const win = getCurrentWindow();
        await win.setFullscreen(!(await win.isFullscreen()));
        break;
      }
      case "minimize": await getCurrentWindow().minimize(); break;
      case "documentation": await openUrl(`${REPO_URL}#readme`); break;
      case "report-issue": await openUrl(ISSUES_URL); break;
      case "quit": store.requestQuit(activeAgentCount()); break;
      case "quit-stop-agents": await invoke("quit_app_and_stop_agents"); break;
    }
    return true;
  } catch (error) {
    console.error("ubra: command failed", request.action, error);
    toasts.push("Action failed", String(error), "", { kind: "copy" });
    return false;
  }
}

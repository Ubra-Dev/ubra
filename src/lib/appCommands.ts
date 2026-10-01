import { SHORTCUTS, type KeyShape, type ShortcutAction, type ShortcutMatch } from "./shortcuts.ts";

export type AppCommand = ShortcutAction | "open-project" | "new-empty-workspace" |
  "close-workspace" | "restart-terminal" |
  "undo" | "redo" | "cut" | "copy" | "paste" | "select-all" |
  "fullscreen" | "minimize" | "keyboard-shortcuts" | "documentation" | "report-issue" |
  "check-for-updates" | "quit" | "quit-stop-agents";

export type CommandRequest = Omit<ShortcutMatch, "action"> & {
  action: AppCommand | "switch-workspace";
  workspaceId?: string;
};

export interface CommandContext {
  ready: boolean;
  blocked: boolean;
  settingsOpen: boolean;
  textFocus: boolean;
  writable: boolean;
  textSelection: boolean;
  terminalFocus: boolean;
  terminalAvailable: boolean;
  terminalSelection: boolean;
  terminalRunning: boolean;
  hasPane: boolean;
  hasWorkspace: boolean;
  tabCount: number;
  workspaceIds: string[];
  restartable: boolean;
  savedCommand: boolean;
  zoomed: boolean;
}

export function canDispatch(request: CommandRequest, ctx: CommandContext): boolean {
  const action = request.action;
  if (["quit", "quit-stop-agents", "minimize", "fullscreen", "documentation", "report-issue",
    "check-for-updates"].includes(action)) return true;
  if (action === "open-settings" || action === "keyboard-shortcuts") return !ctx.blocked || ctx.settingsOpen;
  if (["undo", "redo", "cut", "copy", "paste", "select-all"].includes(action)) {
    if (ctx.textFocus) {
      if (action === "copy") return ctx.textSelection;
      if (action === "select-all") return true;
      return ctx.writable && (action !== "cut" || ctx.textSelection);
    }
    if (ctx.blocked || !ctx.terminalFocus || !ctx.terminalAvailable) return false;
    if (action === "copy") return ctx.terminalSelection;
    if (action === "paste") return ctx.terminalRunning;
    return action === "select-all";
  }
  if (!ctx.ready || ctx.blocked || ctx.textFocus) return false;
  if (["new-workspace", "open-project", "new-empty-workspace",
       "font-bigger", "font-smaller", "font-reset"].includes(action)) return true;
  if (action === "switch-workspace") return !!request.workspaceId && ctx.workspaceIds.includes(request.workspaceId);
  if (action === "prev-workspace" || action === "next-workspace") return ctx.workspaceIds.length > 1;
  if (action === "prev-tab" || action === "next-tab") return ctx.tabCount > 1;
  if (action === "jump-tab") return request.index !== undefined && request.index >= 0 && request.index < ctx.tabCount;
  if (action === "new-tab" || action === "close-workspace") return ctx.hasWorkspace;
  if (action === "restart-terminal") return ctx.hasPane && ctx.restartable;
  return ctx.hasPane;
}

export function acceleratorFor(action: AppCommand, isMac: boolean): string | null {
  if (["copy", "paste", "select-all"].includes(action)) {
    const key = action === "copy" ? "C" : action === "paste" ? "V" : "A";
    // Stable accelerators cannot intercept a shell's Ctrl+C/V/A during an
    // asynchronous native-menu update after focus changes.
    return `${isMac ? "Cmd" : "Ctrl+Shift"}+${key}`;
  }
  if (!isMac && ["undo", "redo", "cut"].includes(action)) return null;
  if (action === "undo") return `${isMac ? "Cmd" : "Ctrl"}+Z`;
  if (action === "redo") return isMac ? "Cmd+Shift+Z" : "Ctrl+Y";
  if (action === "cut") return `${isMac ? "Cmd" : "Ctrl"}+X`;
  if (action === "open-project") return "CmdOrCtrl+O";
  if (action === "quit") return "CmdOrCtrl+Q";
  const binding = SHORTCUTS.find((binding) => binding.action === action && !binding.dir);
  if (!binding) return null;
  return [binding.mod ? "CmdOrCtrl" : null, binding.alt ? "Alt" : null,
    binding.shift ? "Shift" : null, binding.key.toUpperCase()].filter(Boolean).join("+");
}

export interface MenuEntry {
  id: string;
  label: string;
  action?: AppCommand;
  system?: "About" | "Services" | "Hide" | "HideOthers" | "ShowAll" | "BringAllToFront";
  checked?: boolean;
}
export interface MenuGroup { label: string; items: Array<MenuEntry | null> }
const entry = (action: AppCommand, label: string, checked?: boolean): MenuEntry => ({ id: action, action, label, checked });

export function menuGroups(isMac: boolean): MenuGroup[] {
  const settings = entry("open-settings", "Settings…");
  const quit = entry("quit", "Quit Ubra");
  const quitStop = entry("quit-stop-agents", "Stop Agents and Quit");
  const checkUpdates = entry("check-for-updates", "Check for Updates…");
  const about: MenuEntry = { id: "about", label: "About Ubra", system: "About" as const };
  return [
    ...(isMac ? [{ label: "Ubra", items: [about, checkUpdates, settings, null,
      { id: "services", label: "Services", system: "Services" as const }, null,
      { id: "hide", label: "Hide Ubra", system: "Hide" as const },
      { id: "hide-others", label: "Hide Others", system: "HideOthers" as const },
      { id: "show-all", label: "Show All", system: "ShowAll" as const }, null, quit, quitStop] }] : []),
    { label: "File", items: [entry("open-project", "Open Project Folder…"),
      entry("new-empty-workspace", "New Empty Workspace"), entry("new-tab", "New Tab"), null,
      entry("close-pane-or-tab", "Close Pane/Tab"), entry("close-workspace", "Close Workspace"),
      ...(!isMac ? [null, settings, quit, quitStop] : [])] },
    { label: "Edit", items: [entry("undo", "Undo"), entry("redo", "Redo"), null,
      entry("cut", "Cut"), entry("copy", "Copy"), entry("paste", "Paste"), entry("select-all", "Select All"),
      null, entry("find-in-pane", "Find in Terminal…")] },
    { label: "Terminal", items: [entry("split-right", "Split Right"), entry("split-down", "Split Down"), null,
      entry("rename-pane", "Rename Pane…"), entry("restart-terminal", "Restart Terminal"), null,
      entry("move-pane-to-new-tab", "Move Pane to New Tab"), entry("move-pane-to-new-workspace", "Move Pane to New Workspace")] },
    { label: "View", items: [entry("toggle-zoom", "Zoom Pane"), null,
      entry("font-bigger", "Increase Text Size"), entry("font-smaller", "Decrease Text Size"), entry("font-reset", "Reset Text Size"),
      null, entry("fullscreen", "Full Screen", false)] },
    { label: "Window", items: [entry("minimize", "Minimize"),
      ...(isMac ? [{ id: "bring-front", label: "Bring All to Front", system: "BringAllToFront" as const }] : []), null,
      entry("prev-tab", "Previous Tab"), entry("next-tab", "Next Tab"),
      entry("prev-workspace", "Previous Workspace"), entry("next-workspace", "Next Workspace"), null] },
    { label: "Help", items: [entry("keyboard-shortcuts", "Keyboard Shortcuts"), entry("documentation", "Documentation"),
      entry("report-issue", "Report an Issue"), ...(!isMac ? [checkUpdates, null, about] : [])] },
  ];
}

/** Native accelerators own their commands; unmatched bindings retain DOM dispatch. */
export function shouldDispatchDom(action: AppCommand, nativeOwnsBinding: boolean, shiftedPlus = false): boolean {
  return !nativeOwnsBinding || (action === "font-bigger" && shiftedPlus);
}

export function matchEditingShortcut(shape: KeyShape, isMac: boolean, terminalFocus: boolean): AppCommand | null {
  if (!shape.mod || shape.alt) return null;
  const key = shape.key.toLowerCase();
  const clipboardShift = !isMac && terminalFocus;
  if (shape.shift === clipboardShift) {
    if (key === "c") return "copy";
    if (key === "v") return "paste";
    if (key === "a") return "select-all";
  }
  if (terminalFocus) return null;
  if (!shape.shift && key === "x") return "cut";
  if (!shape.shift && key === "z") return "undo";
  if ((isMac && shape.shift && key === "z") || (!isMac && !shape.shift && key === "y")) return "redo";
  return null;
}

import { invoke } from "@tauri-apps/api/core";
import {
  activeTab,
  activeWorkspace,
  baseName,
  clearStaleZoom,
  closePaneInTab,
  collectPaneIds,
  countPanes,
  defaultLayout,
  defaultTab,
  defaultWorkspace,
  extractPaneToNewTab,
  extractPaneToNewWorkspace,
  findNeighbor,
  findPane,
  findTabByPane,
  gridTab,
  moveWorkspace,
  resizePaneInTab,
  sanitizeLayout,
  setZoomedPane,
  splitPaneInTab,
  swapPanesInTab,
  type Direction,
  type Layout,
  type Tab,
  type Workspace,
} from "./layout";
import {
  DEFAULT_CHIME_STYLE,
  DEFAULT_DELIVERY,
  DEFAULT_TOAST_POSITION,
  parseChimeStyle,
  parseDelivery,
  parseToastPosition,
  type NotifyDelivery,
  type ToastPosition,
} from "./notify";
import {
  DEFAULT_THEME_ID,
  THEMES,
  isThemeId,
  type ThemeId,
} from "./themes";

export type CloseKind = "workspace" | "tab" | "pane";

export interface PendingClose {
  kind: CloseKind;
  /** Workspace id, tab id, or pane node id. */
  id: string;
  /** Display name shown in the dialog. */
  name: string;
  tabs: number;
  panes: number;
}

export const DEFAULT_TERM_FONT_SIZE = 13;
export const MIN_TERM_FONT_SIZE = 9;
export const MAX_TERM_FONT_SIZE = 24;
export const DEFAULT_TERM_SCROLLBACK = 1000;
export const SCROLLBACK_OPTIONS = [100, 1000, 5000, 10000];
export const DEFAULT_TERM_OPACITY = 100;
export const MIN_TERM_OPACITY = 10;
export const MAX_TERM_OPACITY = 100;

class AppStore {
  layout = $state<Layout | null>(null);
  loaded = $state(false);
  firstRun = $state(false);
  onboardingOpen = $state(false);
  loadError = $state<string | null>(null);
  saveError = $state<string | null>(null);
  recoveryRequired = $state(false);
  recoveryBusy = $state(false);
  recoveryError = $state<string | null>(null);
  recoveryBackupPath = $state<string | null>(null);
  themeId = $state<ThemeId>(DEFAULT_THEME_ID);
  termFontSize = $state<number>(DEFAULT_TERM_FONT_SIZE);
  termScrollback = $state<number>(DEFAULT_TERM_SCROLLBACK);
  termOpacity = $state<number>(DEFAULT_TERM_OPACITY);
  notifyDelivery = $state<NotifyDelivery>(DEFAULT_DELIVERY);
  toastPosition = $state<ToastPosition>(DEFAULT_TOAST_POSITION);
  soundEnabled = $state<boolean>(true);
  /** Lowercase agent clis muted for sounds (Herdr mutes droid by default). */
  mutedAgents = $state<string[]>(["droid"]);
  /** Custom notification sound file (blank = synthesized default chime). */
  soundFile = $state<string>("");
  /** Selected chime: a built-in id or "custom" (uses soundFile). */
  soundStyle = $state<string>(DEFAULT_CHIME_STYLE);
  settingsOpen = $state(false);
  /** Last-focused pane node id (session-only, for shortcuts). */
  focusedPaneId = $state<string | null>(null);
  /** Pane node id that should enter rename editing (F2); cleared on take. */
  paneRenameTarget = $state<string | null>(null);
  /** Close awaiting confirmation in the alert dialog; null when idle. */
  pendingClose = $state<PendingClose | null>(null);
  /** Pane node id whose terminal should take keyboard focus; cleared on take. */
  paneFocusTarget = $state<string | null>(null);
  /** Pane node id that should open terminal find; cleared on take. */
  paneFindTarget = $state<string | null>(null);
  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private pendingTerminalCommand: { paneId: string; command: string } | null =
    null;

  get theme() {
    return THEMES[this.themeId];
  }

  workspace(): Workspace | null {
    if (!this.layout) return null;
    return activeWorkspace(this.layout);
  }

  tab(): Tab | null {
    const ws = this.workspace();
    return ws ? activeTab(ws) : null;
  }

  async boot(): Promise<void> {
    try {
      const savedTheme = window.localStorage.getItem("ubra.theme");
      if (isThemeId(savedTheme)) this.themeId = savedTheme;
      const savedFont = window.localStorage.getItem("ubra.termFontSize");
      if (savedFont !== null) this.termFontSize = this.clampFontSize(Number(savedFont));
      const savedScrollback = window.localStorage.getItem("ubra.termScrollback");
      if (
        savedScrollback !== null &&
        SCROLLBACK_OPTIONS.includes(Number(savedScrollback))
      ) {
        this.termScrollback = Number(savedScrollback);
      }
      const savedOpacity = window.localStorage.getItem("ubra.termOpacity");
      if (savedOpacity !== null) {
        this.termOpacity = this.clampOpacity(Number(savedOpacity));
      }
      const savedDelivery = window.localStorage.getItem("ubra.notifyDelivery");
      if (savedDelivery !== null) this.notifyDelivery = parseDelivery(savedDelivery);
      const savedPosition = window.localStorage.getItem("ubra.toastPosition");
      if (savedPosition !== null) this.toastPosition = parseToastPosition(savedPosition);
      const savedSound = window.localStorage.getItem("ubra.soundEnabled");
      if (savedSound !== null) this.soundEnabled = savedSound !== "false";
      try {
        const savedMuted = window.localStorage.getItem("ubra.mutedAgents");
        if (savedMuted !== null) {
          const parsed: unknown = JSON.parse(savedMuted);
          if (Array.isArray(parsed)) {
            this.mutedAgents = parsed.filter(
              (v): v is string => typeof v === "string",
            );
          }
        }
      } catch {
        // Keep the default mute list when the saved value is corrupt.
      }
      const savedFile = window.localStorage.getItem("ubra.soundFile");
      if (savedFile !== null) this.soundFile = savedFile;
      const savedStyle = window.localStorage.getItem("ubra.soundStyle");
      if (savedStyle !== null) this.soundStyle = parseChimeStyle(savedStyle);
    } catch {
      // The app can still start with its defaults if storage is unavailable.
    }

    await this.retryLayout();
  }

  /** A failed load never installs a renderable/default layout or enables autosave. */
  async retryLayout(): Promise<void> {
    if (this.recoveryBusy) return;
    this.recoveryBusy = true;
    this.recoveryRequired = true;
    this.layout = null;
    this.firstRun = false;
    this.recoveryError = null;
    clearTimeout(this.saveTimer ?? undefined);
    this.saveTimer = null;
    try {
      const raw = await invoke<unknown>("load_layout");
      this.layout = raw == null ? defaultLayout() : sanitizeLayout(raw);
      this.firstRun = raw == null;
      this.recoveryRequired = false;
      this.loadError = null;
      this.saveError = null;
    } catch (e) {
      this.loadError = e instanceof Error ? e.message : String(e);
    } finally {
      this.recoveryBusy = false;
      this.loaded = true;
    }
  }

  /** Export keeps exact original bytes, including corrupt/unsupported documents. */
  async exportRecoveryLayout(): Promise<void> {
    if (!this.recoveryRequired || this.recoveryBusy) return;
    this.recoveryBusy = true;
    this.recoveryError = null;
    try {
      this.recoveryBackupPath = await invoke<string>("export_layout");
    } catch (e) {
      this.recoveryError = e instanceof Error ? e.message : String(e);
    } finally {
      this.recoveryBusy = false;
    }
  }

  /** Call only from the explicit recovery reset action, never automatically. */
  async resetRecoveryLayout(): Promise<void> {
    if (!this.recoveryRequired || this.recoveryBusy) return;
    this.recoveryBusy = true;
    this.recoveryError = null;
    const fresh = defaultLayout();
    try {
      const backup = await invoke<string | null>("reset_layout", { layout: fresh });
      if (backup) this.recoveryBackupPath = backup;
      this.layout = fresh;
      this.firstRun = false;
      this.recoveryRequired = false;
      this.loadError = null;
      this.saveError = null;
    } catch (e) {
      this.recoveryError = e instanceof Error ? e.message : String(e);
    } finally {
      this.recoveryBusy = false;
    }
  }

  setTheme(themeId: ThemeId): void {
    this.themeId = themeId;
    try {
      window.localStorage.setItem("ubra.theme", themeId);
    } catch (e) {
      console.error("ubra: failed to save theme", e);
    }
  }

  private clampFontSize(px: number): number {
    if (!Number.isFinite(px)) return DEFAULT_TERM_FONT_SIZE;
    return Math.min(
      MAX_TERM_FONT_SIZE,
      Math.max(MIN_TERM_FONT_SIZE, Math.round(px)),
    );
  }

  setTermFontSize(px: number): void {
    const clamped = this.clampFontSize(px);
    if (clamped === this.termFontSize) return;
    this.termFontSize = clamped;
    try {
      window.localStorage.setItem("ubra.termFontSize", String(clamped));
    } catch (e) {
      console.error("ubra: failed to save terminal font size", e);
    }
  }

  bumpTermFontSize(delta: number): void {
    this.setTermFontSize(this.termFontSize + delta);
  }

  resetTermFontSize(): void {
    this.setTermFontSize(DEFAULT_TERM_FONT_SIZE);
  }

  setTermScrollback(lines: number): void {
    if (!SCROLLBACK_OPTIONS.includes(lines) || lines === this.termScrollback)
      return;
    this.termScrollback = lines;
    try {
      window.localStorage.setItem("ubra.termScrollback", String(lines));
    } catch (e) {
      console.error("ubra: failed to save terminal scrollback", e);
    }
  }

  private clampOpacity(pct: number): number {
    if (!Number.isFinite(pct)) return DEFAULT_TERM_OPACITY;
    return Math.min(
      MAX_TERM_OPACITY,
      Math.max(MIN_TERM_OPACITY, Math.round(pct)),
    );
  }

  setTermOpacity(pct: number): void {
    const clamped = this.clampOpacity(pct);
    if (clamped === this.termOpacity) return;
    this.termOpacity = clamped;
    try {
      window.localStorage.setItem("ubra.termOpacity", String(clamped));
    } catch (e) {
      console.error("ubra: failed to save terminal opacity", e);
    }
  }

  private savePref(key: string, value: string): void {
    try {
      window.localStorage.setItem(key, value);
    } catch (e) {
      console.error(`ubra: failed to save ${key}`, e);
    }
  }

  setNotifyDelivery(delivery: NotifyDelivery): void {
    this.notifyDelivery = delivery;
    this.savePref("ubra.notifyDelivery", delivery);
  }

  setToastPosition(position: ToastPosition): void {
    this.toastPosition = position;
    this.savePref("ubra.toastPosition", position);
  }

  setSoundEnabled(enabled: boolean): void {
    this.soundEnabled = enabled;
    this.savePref("ubra.soundEnabled", String(enabled));
  }

  setSoundFile(path: string): void {
    this.soundFile = path;
    this.savePref("ubra.soundFile", path);
  }

  setSoundStyle(style: string): void {
    this.soundStyle = parseChimeStyle(style);
    this.savePref("ubra.soundStyle", this.soundStyle);
  }

  setAgentMuted(cli: string, muted: boolean): void {
    const lower = cli.toLowerCase();
    this.mutedAgents = muted
      ? [...new Set([...this.mutedAgents, lower])]
      : this.mutedAgents.filter((m) => m.toLowerCase() !== lower);
    this.savePref("ubra.mutedAgents", JSON.stringify(this.mutedAgents));
  }

  isAgentMuted(cli: string): boolean {
    return this.mutedAgents.some((m) => m.toLowerCase() === cli.toLowerCase());
  }

  saveSoon(immediate = false): void {
    clearTimeout(this.saveTimer ?? undefined);
    this.saveTimer = null;
    if (!this.loaded || this.recoveryRequired || this.recoveryBusy || !this.layout) return;
    if (immediate) {
      void this.flush();
      return;
    }
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null;
      void this.flush();
    }, 300);
  }

  private async flush(): Promise<void> {
    if (!this.layout || !this.loaded || this.recoveryRequired || this.recoveryBusy) return;
    try {
      const layout = sanitizeLayout($state.snapshot(this.layout));
      await invoke("save_layout", { layout });
      this.saveError = null;
    } catch (e) {
      this.saveError = e instanceof Error ? e.message : String(e);
    }
  }

  addWorkspace(withGrid = false): void {
    if (!this.layout) return;
    const ws = defaultWorkspace(`Workspace ${this.layout.workspaces.length + 1}`);
    if (withGrid) {
      const tab = gridTab();
      ws.tabs = [tab];
      ws.activeTabId = tab.id;
      this.paneFocusTarget = collectPaneIds(tab.root)[0];
    }
    this.layout.workspaces.push(ws);
    this.layout.activeWorkspaceId = ws.id;
    this.saveSoon();
  }

  switchWorkspace(id: string): void {
    if (!this.layout) return;
    if (this.layout.workspaces.some((w) => w.id === id)) {
      this.layout.activeWorkspaceId = id;
      this.saveSoon();
    }
  }

  renameWorkspace(id: string, name: string): void {
    const ws = this.layout?.workspaces.find((w) => w.id === id);
    if (ws && name.trim()) {
      ws.name = name.trim();
      this.saveSoon();
    }
  }

  moveWorkspace(
    sourceId: string,
    targetId: string,
    position: "before" | "after" = "before",
  ): void {
    if (!this.layout) return;
    if (moveWorkspace(this.layout, sourceId, targetId, position)) {
      this.saveSoon();
    }
  }

  requestCloseWorkspace(id: string): void {
    const ws = this.layout?.workspaces.find((w) => w.id === id);
    if (!ws) return;
    this.pendingClose = {
      kind: "workspace",
      id,
      name: ws.name,
      tabs: ws.tabs.length,
      panes: ws.tabs.reduce((n, t) => n + countPanes(t.root), 0),
    };
  }

  private doCloseWorkspace(id: string): void {
    if (!this.layout) return;
    if (this.layout.workspaces.length <= 1) {
      const ws = defaultWorkspace();
      this.layout.workspaces = [ws];
      this.layout.activeWorkspaceId = ws.id;
    } else {
      this.layout.workspaces = this.layout.workspaces.filter((w) => w.id !== id);
      if (this.layout.activeWorkspaceId === id) {
        this.layout.activeWorkspaceId =
          this.layout.workspaces[this.layout.workspaces.length - 1].id;
      }
    }
    this.saveSoon(true);
  }

  addTab(): void {
    const ws = this.workspace();
    if (!ws) return;
    const tab = defaultTab(`Tab ${ws.tabs.length + 1}`);
    ws.tabs.push(tab);
    ws.activeTabId = tab.id;
    this.saveSoon();
  }

  requestCloseTab(id: string): void {
    const tab = this.workspace()?.tabs.find((t) => t.id === id);
    if (!tab) return;
    this.pendingClose = {
      kind: "tab",
      id,
      name: tab.name,
      tabs: 1,
      panes: countPanes(tab.root),
    };
  }

  private doCloseTab(id: string): void {
    const ws = this.workspace();
    if (!ws) return;
    if (ws.tabs.length <= 1) {
      ws.tabs = [defaultTab()];
      ws.activeTabId = ws.tabs[0].id;
    } else {
      ws.tabs = ws.tabs.filter((t) => t.id !== id);
      if (ws.activeTabId === id) ws.activeTabId = ws.tabs[ws.tabs.length - 1].id;
    }
    this.saveSoon(true);
  }

  switchTab(id: string): void {
    const ws = this.workspace();
    if (ws && ws.tabs.some((t) => t.id === id)) {
      ws.activeTabId = id;
      this.saveSoon();
    }
  }

  renameTab(id: string, name: string): void {
    const ws = this.workspace();
    const tab = ws?.tabs.find((t) => t.id === id);
    if (tab && name.trim()) {
      tab.name = name.trim();
      this.saveSoon();
    }
  }

  splitPane(paneId: string, dir: "row" | "col", ratio = 0.5): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, paneId);
    if (!found) return;
    const sibling = splitPaneInTab(found.tab, paneId, dir, ratio);
    if (sibling) {
      // A split made while zoomed would hide the new sibling; show both.
      setZoomedPane(found.tab, null);
      // The new pane takes focus (outline + keyboard).
      this.focusedPaneId = sibling.id;
      this.paneFocusTarget = sibling.id;
      this.saveSoon();
    }
  }

  requestClosePane(paneId: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, paneId);
    const node = found ? findPane(found.tab.root, paneId) : null;
    if (!node) return;
    this.pendingClose = {
      kind: "pane",
      id: paneId,
      name: node.title?.trim() || "Pane",
      tabs: 0,
      panes: 1,
    };
  }

  private doClosePane(paneId: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, paneId);
    if (found && closePaneInTab(found.tab, paneId)) {
      clearStaleZoom(found.tab);
      this.saveSoon(true);
    }
  }

  confirmPendingClose(): void {
    const pending = this.pendingClose;
    this.pendingClose = null;
    if (!pending || !this.layout) return;
    // Re-resolve: never close a target that no longer exists.
    if (pending.kind === "workspace") {
      if (this.layout.workspaces.some((w) => w.id === pending.id)) {
        this.doCloseWorkspace(pending.id);
      }
    } else if (pending.kind === "tab") {
      if (this.workspace()?.tabs.some((t) => t.id === pending.id)) {
        this.doCloseTab(pending.id);
      }
    } else if (findTabByPane(this.layout, pending.id)) {
      this.doClosePane(pending.id);
    }
  }

  cancelPendingClose(): void {
    this.pendingClose = null;
  }

  renamePane(paneId: string, name: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, paneId);
    const node = found ? findPane(found.tab.root, paneId) : null;
    if (node && name.trim()) {
      node.title = name.trim();
      this.saveSoon();
    }
  }

  toggleZoomPane(paneId: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, paneId);
    if (!found) return;
    setZoomedPane(
      found.tab,
      found.tab.zoomedPaneId === paneId ? null : paneId,
    );
    this.saveSoon();
  }

  /** Switch workspace+tab so the given pane node is visible. */
  revealPane(nodeId: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, nodeId);
    if (!found) return;
    this.layout.activeWorkspaceId = found.ws.id;
    found.ws.activeTabId = found.tab.id;
    this.saveSoon();
  }

  focusPane(nodeId: string): void {
    this.focusedPaneId = nodeId;
  }

  /** Shortcut target: focused pane when it is in the active tab, else its first pane. */
  currentPane(): { tab: Tab; paneId: string } | null {
    const ws = this.workspace();
    if (!ws) return null;
    const tab = activeTab(ws);
    if (this.focusedPaneId && findPane(tab.root, this.focusedPaneId)) {
      return { tab, paneId: this.focusedPaneId };
    }
    const first = collectPaneIds(tab.root)[0];
    return first ? { tab, paneId: first } : null;
  }

  openOnboarding(): void {
    if (!this.layout || this.firstRun) return;
    this.settingsOpen = false;
    this.onboardingOpen = true;
  }

  completeOnboarding(
    projectDirectory: string | null,
    command: string | null,
  ): string | null {
    if (!this.layout || (!this.firstRun && !this.onboardingOpen)) return null;

    let pane: ReturnType<typeof findPane> = null;
    if (this.firstRun) {
      const current = this.currentPane();
      if (!current) return null;
      pane = findPane(current.tab.root, current.paneId);
      if (!pane) return null;
      if (projectDirectory) {
        pane.cwd = projectDirectory;
        const workspace = this.layout.workspaces.find((ws) =>
          ws.tabs.some((tab) => tab.id === current.tab.id),
        );
        if (workspace) workspace.name = baseName(projectDirectory) || "Project";
      }
    } else {
      if (!projectDirectory) return null;
      const workspace = defaultWorkspace(baseName(projectDirectory) || "Project");
      this.layout.workspaces.push(workspace);
      this.layout.activeWorkspaceId = workspace.id;
      pane = findPane(workspace.tabs[0].root, workspace.tabs[0].root.id);
      if (!pane) return null;
      pane.cwd = projectDirectory;
    }

    const trimmedCommand = command?.trim() ?? "";
    this.pendingTerminalCommand = trimmedCommand
      ? { paneId: pane.id, command: trimmedCommand }
      : null;
    this.firstRun = false;
    this.onboardingOpen = false;
    this.saveSoon(true);
    return pane.id;
  }

  skipOnboarding(): void {
    if (this.firstRun) {
      this.firstRun = false;
      this.saveSoon(true);
    }
    this.onboardingOpen = false;
  }

  takePendingTerminalCommand(paneId: string): string | null {
    if (this.pendingTerminalCommand?.paneId !== paneId) return null;
    const command = this.pendingTerminalCommand.command;
    this.pendingTerminalCommand = null;
    return command;
  }

  cycleTab(dir: 1 | -1): void {
    const ws = this.workspace();
    if (!ws || ws.tabs.length < 2) return;
    const at = ws.tabs.findIndex((t) => t.id === ws.activeTabId);
    const cur = at < 0 ? 0 : at;
    this.switchTab(ws.tabs[(cur + dir + ws.tabs.length) % ws.tabs.length].id);
  }

  cycleWorkspace(dir: 1 | -1): void {
    if (!this.layout || this.layout.workspaces.length < 2) return;
    const all = this.layout.workspaces;
    const at = all.findIndex((w) => w.id === this.layout!.activeWorkspaceId);
    const cur = at < 0 ? 0 : at;
    this.switchWorkspace(all[(cur + dir + all.length) % all.length].id);
  }

  jumpTab(index: number): void {
    const ws = this.workspace();
    const tab = ws?.tabs[index];
    if (tab) this.switchTab(tab.id);
  }

  /** Close the shortcut-target pane; when it is the tab's last pane, close the tab. */
  requestClosePaneOrTab(): void {
    const cur = this.currentPane();
    if (!cur) return;
    if (countPanes(cur.tab.root) > 1) this.requestClosePane(cur.paneId);
    else this.requestCloseTab(cur.tab.id);
  }

  requestPaneRename(): void {
    const cur = this.currentPane();
    if (cur) this.paneRenameTarget = cur.paneId;
  }

  requestPaneFind(): void {
    const cur = this.currentPane();
    if (cur) this.paneFindTarget = cur.paneId;
  }

  /** Move keyboard focus to the neighbor pane in `dir`, if any. */
  focusNeighbor(dir: Direction): void {
    const cur = this.currentPane();
    if (!cur) return;
    const next = findNeighbor(cur.tab.root, cur.paneId, dir);
    if (next) {
      this.focusedPaneId = next.id;
      this.paneFocusTarget = next.id;
    }
  }

  /** Exchange the focused pane's position with its neighbor in `dir`. */
  swapWithNeighbor(dir: Direction): void {
    const cur = this.currentPane();
    if (!cur) return;
    const next = findNeighbor(cur.tab.root, cur.paneId, dir);
    if (next && swapPanesInTab(cur.tab, cur.paneId, next.id)) {
      this.saveSoon();
    }
  }

  /** Exchange two panes' positions (titlebar drag-drop). Same-tab only. */
  swapPanes(aId: string, bId: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, aId);
    if (found && swapPanesInTab(found.tab, aId, bId)) {
      this.saveSoon();
    }
  }

  /** Grow the focused pane toward `dir` by one step. */
  resizeFocused(dir: Direction): void {
    const cur = this.currentPane();
    if (cur && resizePaneInTab(cur.tab, cur.paneId, dir, 0.05)) {
      this.paneSizesChanged();
    }
  }

  /** Move a pane into a new tab of its workspace and reveal it. */
  movePaneToNewTab(paneId: string): void {
    if (!this.layout) return;
    if (extractPaneToNewTab(this.layout, paneId)) {
      this.revealPane(paneId);
      this.saveSoon(true);
    }
  }

  /** Move a pane into a new workspace and reveal it. */
  movePaneToNewWorkspace(paneId: string): void {
    if (!this.layout) return;
    if (extractPaneToNewWorkspace(this.layout, paneId)) {
      this.revealPane(paneId);
      this.saveSoon(true);
    }
  }

  paneSizesChanged(): void {
    this.saveSoon();
  }
}

export const store = new AppStore();

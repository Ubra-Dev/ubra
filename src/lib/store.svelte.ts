import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { agentClis } from "./agentClis.svelte";
import { DeferredSwitch } from "./deferredSwitch";
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
  findWorkspaceByRoot,
  moveWorkspace,
  preferredAgentCli as pickPreferredAgentCli,
  presetTab,
  resizePaneInTab,
  resolveWorkspaceRoot,
  sanitizeLayout,
  setZoomedPane,
  splitPaneInTab,
  stampPaneAgentCli,
  stampPaneAgentSession,
  swapPanesInTab,
  type Direction,
  type Layout,
  type PaneNode,
  type Tab,
  type Workspace,
  type WorkspaceLayoutPreset,
} from "./layout";
import { implicitLaunchCommand } from "./agentLaunch";
import { restoreCommandFor } from "./agentResume";
import {
  countLayoutPanes,
  parseQuitAction,
  QUIT_ACTION_KEY,
  resolveQuitRequest,
  type QuitAction,
} from "./quitConfirm";
import { PendingCommands } from "./pendingCommands";
import { type FleetLaunchPlan } from "./fleet";
import type { PtySessionInfo } from "./terminalLifecycle";
import { toasts } from "./toasts.svelte.ts";
import { StartupCommands } from "./startupCommands";
import {
  DEFAULT_DELIVERY,
  DEFAULT_TOAST_POSITION,
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
import {
  DEFAULT_UI_FONT_ID,
  UI_FONTS,
  isUiFontId,
  type UiFontId,
} from "./uiFonts";
import { DEFAULT_UI_SCALE, UI_SCALE_STEP, clampUiScale } from "./uiScale";
import {
  DEFAULT_RIGHT_PANEL_WIDTH,
  DEFAULT_SIDEBAR_WIDTH,
  DEFAULT_SPLIT_RATIO,
  clampSidebarWidth,
  clampSplitRatio,
} from "./sidebarResize";

export type CloseKind = "workspace" | "tab" | "pane";
export type RightPanelView = "explorer" | "source-control";

export interface PendingClose {
  kind: CloseKind;
  /** Workspace id, tab id, or pane node id. */
  id: string;
  /** Display name shown in the dialog. */
  name: string;
  tabs: number;
  panes: number;
}

export interface PendingQuit {
  panes: number;
  agents: number;
}

export const DEFAULT_TERM_FONT_SIZE = 13;
export const MIN_TERM_FONT_SIZE = 9;
export const MAX_TERM_FONT_SIZE = 24;
export const DEFAULT_TERM_SCROLLBACK = 1000;
export const SCROLLBACK_OPTIONS = [100, 1000, 5000, 10000];
export const DEFAULT_TERM_OPACITY = 100;
export const MIN_TERM_OPACITY = 10;
export const MAX_TERM_OPACITY = 100;
export const DEFAULT_TERM_GPU = true;

class AppStore {
  layout = $state<Layout | null>(null);
  loaded = $state(false);
  firstRun = $state(false);
  loadError = $state<string | null>(null);
  saveError = $state<string | null>(null);
  /** True while a layout save is scheduled or in flight. */
  saving = $state(false);
  rightPanelOpen = $state(false);
  rightPanelView = $state<RightPanelView>("explorer");
  rightPanelWidth = $state<number>(DEFAULT_RIGHT_PANEL_WIDTH);
  leftPanelOpen = $state(true);
  recoveryRequired = $state(false);
  recoveryBusy = $state(false);
  recoveryError = $state<string | null>(null);
  recoveryBackupPath = $state<string | null>(null);
  themeId = $state<ThemeId>(DEFAULT_THEME_ID);
  termFontSize = $state<number>(DEFAULT_TERM_FONT_SIZE);
  uiScale = $state<number>(DEFAULT_UI_SCALE);
  uiFontId = $state<UiFontId>(DEFAULT_UI_FONT_ID);
  /** Font id currently being activated, or null when idle. */
  uiFontApplying = $state<UiFontId | null>(null);
  /** Last agent CLI stored as a workspace default (onboarding prefill). */
  lastUsedAgentCli = $state("");
  sidebarWidth = $state<number>(DEFAULT_SIDEBAR_WIDTH);
  /** Fraction of sidebar split height given to workspaces (rest to agents). */
  sidebarSplit = $state<number>(DEFAULT_SPLIT_RATIO);
  termScrollback = $state<number>(DEFAULT_TERM_SCROLLBACK);
  termOpacity = $state<number>(DEFAULT_TERM_OPACITY);
  /** Prefer the GPU terminal renderer; canvas is the automatic fallback. */
  termGpu = $state<boolean>(DEFAULT_TERM_GPU);
  notifyDelivery = $state<NotifyDelivery>(DEFAULT_DELIVERY);
  toastPosition = $state<ToastPosition>(DEFAULT_TOAST_POSITION);
  soundEnabled = $state<boolean>(true);
  /** Global auto-launch for implicit agent starts; explicit picks bypass it. */
  autoLaunchAgent = $state<boolean>(false);
  /** Menu-bar agent count next to the tray icon (macOS/Linux; no-op on Windows). */
  trayTitleEnabled = $state<boolean>(true);
  /** Live agent list in the tray menu; when off the menu stays static. */
  trayMenuListEnabled = $state<boolean>(true);
  /** Silent background update check on boot + interval; installs stay manual. */
  autoCheckUpdates = $state<boolean>(true);
  /** Lowercase agent clis muted for sounds (Herdr mutes droid by default). */
  mutedAgents = $state<string[]>(["droid"]);
  settingsOpen = $state(false);
  /** Section the Settings modal should open on; consumed on mount, then cleared. */
  settingsOpenSection = $state<string | null>(null);
  /** Revisit-mode onboarding opened from Settings (independent of firstRun). */
  onboardingOpen = $state(false);
  /** Last-focused pane node id (session-only, for shortcuts). */
  focusedPaneId = $state<string | null>(null);
  /** Pane node id that should enter rename editing (F2); cleared on take. */
  paneRenameTarget = $state<string | null>(null);
  /** Close awaiting confirmation in the alert dialog; null when idle. */
  pendingClose = $state<PendingClose | null>(null);
  /** Quit awaiting confirmation; null when idle. */
  pendingQuit = $state<PendingQuit | null>(null);
  /** Remembered Quit choice; `ask` shows the confirmation dialog. */
  quitAction = $state<QuitAction>("ask");
  /** Pane node id whose terminal should take keyboard focus; cleared on take. */
  paneFocusTarget = $state<string | null>(null);
  /** Pane node id that should open terminal find; cleared on take. */
  paneFindTarget = $state<string | null>(null);
  /**
   * Workspace id highlighted in the sidebar while a deferred switch is still
   * scheduled; null when idle. Lets the highlight paint on click, a frame
   * before the canvas reveal commits.
   */
  pendingWorkspaceId = $state<string | null>(null);
  /** Bumped whenever a different workspace becomes visible (reveal animation). */
  workspaceSwitchToken = $state(0);
  /** Bumped whenever a different tab becomes visible (reveal animation). */
  tabSwitchToken = $state(0);
  private deferredSwitch = new DeferredSwitch({
    onPendingChange: (id) => {
      this.pendingWorkspaceId = id;
    },
  });
  private startup = new StartupCommands();
  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private pendingTerminalCommands = new PendingCommands();

  get theme() {
    return THEMES[this.themeId];
  }

  workspace(): Workspace | null {
    if (!this.layout) return null;
    return activeWorkspace(this.layout) ?? null;
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
      const savedUiScale = window.localStorage.getItem("ubra.uiScale");
      if (savedUiScale !== null) this.uiScale = clampUiScale(Number(savedUiScale));
      const savedUiFont = window.localStorage.getItem("ubra.uiFont");
      if (isUiFontId(savedUiFont)) this.uiFontId = savedUiFont;
      const savedSidebarWidth = window.localStorage.getItem("ubra.sidebarWidth");
      if (savedSidebarWidth !== null)
        this.sidebarWidth = clampSidebarWidth(Number(savedSidebarWidth));
      const savedSidebarSplit = window.localStorage.getItem("ubra.sidebarSplit");
      if (savedSidebarSplit !== null)
        this.sidebarSplit = clampSplitRatio(Number(savedSidebarSplit));
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
      const savedGpu = window.localStorage.getItem("ubra.termGpu");
      if (savedGpu !== null) this.termGpu = savedGpu !== "false";
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
      // The chime is fixed; drop any selection persisted by older versions.
      window.localStorage.removeItem("ubra.soundFile");
      window.localStorage.removeItem("ubra.soundStyle");
      const savedPanelOpen = window.localStorage.getItem("ubra.rightPanelOpen");
      if (savedPanelOpen !== null) this.rightPanelOpen = savedPanelOpen !== "false";
      const savedPanelWidth = window.localStorage.getItem("ubra.rightPanelWidth");
      if (savedPanelWidth !== null) {
        this.rightPanelWidth = clampSidebarWidth(
          Number(savedPanelWidth),
          DEFAULT_RIGHT_PANEL_WIDTH,
        );
      }
      const savedLeftOpen = window.localStorage.getItem("ubra.leftPanelOpen");
      if (savedLeftOpen !== null) this.leftPanelOpen = savedLeftOpen !== "false";
      const savedPanelView = window.localStorage.getItem("ubra.rightPanelView");
      if (savedPanelView === "explorer" || savedPanelView === "source-control") {
        this.rightPanelView = savedPanelView;
      }
      const savedAgentCli = window.localStorage.getItem("ubra.lastAgentCli");
      if (savedAgentCli?.trim()) this.lastUsedAgentCli = savedAgentCli.trim();
      const savedAutoLaunch = window.localStorage.getItem("ubra.autoLaunchAgent");
      if (savedAutoLaunch !== null) this.autoLaunchAgent = savedAutoLaunch === "true";
      const savedTrayTitle = window.localStorage.getItem("ubra.trayTitle");
      if (savedTrayTitle !== null) this.trayTitleEnabled = savedTrayTitle === "true";
      const savedTrayMenuList = window.localStorage.getItem("ubra.trayMenuList");
      if (savedTrayMenuList !== null) this.trayMenuListEnabled = savedTrayMenuList === "true";
      const savedAutoCheck = window.localStorage.getItem("ubra.autoCheckUpdates");
      if (savedAutoCheck !== null) this.autoCheckUpdates = savedAutoCheck !== "false";
      this.quitAction = parseQuitAction(window.localStorage.getItem(QUIT_ACTION_KEY));
    } catch {
      // The app can still start with its defaults if storage is unavailable.
    }

    await this.retryLayout();
  }

  /** A failed load never installs a renderable/default layout or enables autosave. */
  async retryLayout(): Promise<void> {
    if (this.recoveryBusy) return;
    this.startup.clear();
    this.recoveryBusy = true;
    this.recoveryRequired = true;
    this.layout = null;
    this.firstRun = false;
    this.recoveryError = null;
    clearTimeout(this.saveTimer ?? undefined);
    this.saveTimer = null;
    this.saving = false;
    try {
      const raw = await invoke<unknown>("load_layout");
      this.layout = raw == null ? defaultLayout() : sanitizeLayout(raw);
      this.firstRun = raw == null;
      this.recoveryRequired = false;
      this.loadError = null;
      this.saveError = null;
      void this.sweepOrphanedPtys();
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
    this.startup.clear();
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
      void this.sweepOrphanedPtys();
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

  setUiScale(pct: number): void {
    const clamped = clampUiScale(pct);
    if (clamped === this.uiScale) return;
    this.uiScale = clamped;
    try {
      window.localStorage.setItem("ubra.uiScale", String(clamped));
    } catch (e) {
      console.error("ubra: failed to save interface scale", e);
    }
  }

  bumpUiScale(delta: 1 | -1): void {
    this.setUiScale(this.uiScale + delta * UI_SCALE_STEP);
  }

  resetUiScale(): void {
    this.setUiScale(DEFAULT_UI_SCALE);
  }

  setUiFont(fontId: UiFontId): void {
    if (!isUiFontId(fontId) || fontId === this.uiFontId) return;
    this.uiFontId = fontId;
    this.savePref("ubra.uiFont", fontId);
    void this.waitForUiFont(fontId);
  }

  resetUiFont(): void {
    if (this.uiFontId === DEFAULT_UI_FONT_ID) return;
    this.uiFontId = DEFAULT_UI_FONT_ID;
    this.savePref("ubra.uiFont", DEFAULT_UI_FONT_ID);
    void this.waitForUiFont(DEFAULT_UI_FONT_ID);
  }

  /**
   * Tracks webfont activation so the UI can show an "applying" state while
   * the browser parses the face and repaints. Never rejects: the font stack
   * already falls back to system fonts on failure.
   */
  private async waitForUiFont(fontId: UiFontId): Promise<void> {
    if (typeof document === "undefined") return;
    this.uiFontApplying = fontId;
    try {
      await document.fonts.load(`16px ${UI_FONTS[fontId].stack}`, "Ag");
      await document.fonts.ready;
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => resolve());
      });
    } catch {
      // Fall through: the system-font fallback already rendered.
    } finally {
      if (this.uiFontApplying === fontId) this.uiFontApplying = null;
    }
  }

  setSidebarWidth(px: number): void {
    if (!this.setSidebarWidthLive(px)) return;
    this.saveSidebarWidth();
  }

  /**
   * Live drag update without persistence; drag handlers persist once on
   * release via `saveSidebarWidth` so sync storage writes stay off the
   * pointermove path. Returns true when the value changed.
   */
  setSidebarWidthLive(px: number): boolean {
    const clamped = clampSidebarWidth(px);
    if (clamped === this.sidebarWidth) return false;
    this.sidebarWidth = clamped;
    return true;
  }

  saveSidebarWidth(): void {
    try {
      window.localStorage.setItem("ubra.sidebarWidth", String(this.sidebarWidth));
    } catch (e) {
      console.error("ubra: failed to save sidebar width", e);
    }
  }

  setRightPanelWidth(px: number): void {
    if (!this.setRightPanelWidthLive(px)) return;
    this.saveRightPanelWidth();
  }

  /** Live drag update without persistence; see `setSidebarWidthLive`. */
  setRightPanelWidthLive(px: number): boolean {
    const clamped = clampSidebarWidth(px, DEFAULT_RIGHT_PANEL_WIDTH);
    if (clamped === this.rightPanelWidth) return false;
    this.rightPanelWidth = clamped;
    return true;
  }

  saveRightPanelWidth(): void {
    try {
      window.localStorage.setItem(
        "ubra.rightPanelWidth",
        String(this.rightPanelWidth),
      );
    } catch (e) {
      console.error("ubra: failed to save right panel width", e);
    }
  }

  setSidebarSplit(ratio: number): void {
    if (!this.setSidebarSplitLive(ratio)) return;
    this.saveSidebarSplit();
  }

  /** Live drag update without persistence; see `setSidebarWidthLive`. */
  setSidebarSplitLive(ratio: number): boolean {
    const clamped = clampSplitRatio(ratio);
    if (clamped === this.sidebarSplit) return false;
    this.sidebarSplit = clamped;
    return true;
  }

  saveSidebarSplit(): void {
    try {
      window.localStorage.setItem("ubra.sidebarSplit", String(this.sidebarSplit));
    } catch (e) {
      console.error("ubra: failed to save sidebar split", e);
    }
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

  setTermGpu(enabled: boolean): void {
    this.termGpu = enabled;
    this.savePref("ubra.termGpu", String(enabled));
  }

  setAutoLaunchAgent(enabled: boolean): void {
    this.autoLaunchAgent = enabled;
    this.savePref("ubra.autoLaunchAgent", String(enabled));
  }

  setTrayTitleEnabled(enabled: boolean): void {
    this.trayTitleEnabled = enabled;
    this.savePref("ubra.trayTitle", String(enabled));
  }

  setTrayMenuListEnabled(enabled: boolean): void {
    this.trayMenuListEnabled = enabled;
    this.savePref("ubra.trayMenuList", String(enabled));
  }

  setAutoCheckUpdates(enabled: boolean): void {
    this.autoCheckUpdates = enabled;
    this.savePref("ubra.autoCheckUpdates", String(enabled));
  }

  setAgentMuted(cli: string, muted: boolean): void {
    const lower = cli.toLowerCase();
    this.mutedAgents = muted
      ? [...new Set([...this.mutedAgents, lower])]
      : this.mutedAgents.filter((m) => m.toLowerCase() !== lower);
    this.savePref("ubra.mutedAgents", JSON.stringify(this.mutedAgents));
  }

  setRightPanelOpen(open: boolean): void {
    this.rightPanelOpen = open;
    this.savePref("ubra.rightPanelOpen", String(open));
  }

  setLeftPanelOpen(open: boolean): void {
    this.leftPanelOpen = open;
    this.savePref("ubra.leftPanelOpen", String(open));
  }

  setRightPanelView(view: RightPanelView): void {
    this.rightPanelView = view;
    this.savePref("ubra.rightPanelView", view);
  }

  isAgentMuted(cli: string): boolean {
    return this.mutedAgents.some((m) => m.toLowerCase() === cli.toLowerCase());
  }

  saveSoon(immediate = false): void {
    if (this.layout) {
      const placed = new Set<string>();
      for (const ws of this.layout.workspaces) {
        for (const tab of ws.tabs) {
          for (const id of collectPaneIds(tab.root)) placed.add(id);
        }
      }
      this.startup.retain(placed);
    }
    clearTimeout(this.saveTimer ?? undefined);
    this.saveTimer = null;
    if (!this.loaded || this.recoveryRequired || this.recoveryBusy || !this.layout) return;
    this.saving = true;
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
    if (!this.layout || !this.loaded || this.recoveryRequired || this.recoveryBusy) {
      this.saving = false;
      return;
    }
    try {
      const layout = sanitizeLayout($state.snapshot(this.layout));
      await invoke("save_layout", { layout });
      this.saveError = null;
    } catch (e) {
      this.saveError = e instanceof Error ? e.message : String(e);
    } finally {
      this.saving = false;
    }
  }

  /** New workspace: native folder picker, defaulting to the last used agent CLI. */
  addWorkspace(preset: WorkspaceLayoutPreset = "single"): void {
    void this.addWorkspaceFromPicker(preset);
  }

  async addWorkspaceFromPicker(preset: WorkspaceLayoutPreset = "single"): Promise<void> {
    if (!this.layout) return;
    let dir: string | string[] | null;
    try {
      dir = await open({
        directory: true,
        multiple: false,
        title: "Choose a project folder",
      });
    } catch {
      toasts.push("Couldn't open the folder picker. Try again.", "", "");
      return;
    }
    if (typeof dir !== "string" || dir.trim() === "") return;
    const existing = findWorkspaceByRoot(this.layout.workspaces, dir);
    if (existing) {
      this.requestSwitchWorkspace(existing.id);
      toasts.push(`"${existing.name}" is already open.`, "", "", {
        kind: "copy",
      });
      return;
    }
    this.createWorkspace(
      dir,
      implicitLaunchCommand(this.autoLaunchAgent, this.lastUsedAgentCli || null),
      preset,
    );
  }

  /** Build a workspace for a folder; every pane starts in that directory. */
  createWorkspace(
    projectDirectory: string,
    command: string | null,
    preset: WorkspaceLayoutPreset,
  ): string | null {
    if (!this.layout) return null;
    const workspace = defaultWorkspace(baseName(projectDirectory) || "Project");
    if (preset !== "single") {
      const tab = presetTab(preset);
      workspace.tabs = [tab];
      workspace.activeTabId = tab.id;
    }
    workspace.defaultCwd = projectDirectory;
    workspace.root = projectDirectory;
    this.layout.workspaces.push(workspace);
    this.layout.activeWorkspaceId = workspace.id;
    this.workspaceSwitchToken += 1;
    const tab = workspace.tabs[0];
    for (const id of collectPaneIds(tab.root)) {
      const node = findPane(tab.root, id);
      if (node) node.cwd = projectDirectory;
      this.queueAgentCommand(id, command ?? "");
    }
    const first = collectPaneIds(tab.root)[0];
    const pane = first ? findPane(tab.root, first) : null;
    if (!pane) return null;
    if (preset !== "single") this.paneFocusTarget = pane.id;
    if (command?.trim()) {
      workspace.defaultCli = command.trim();
    }
    this.saveSoon(true);
    return pane.id;
  }

  /** Blank workspace without onboarding; only the empty-state escape hatch. */
  addBlankWorkspace(): void {
    if (!this.layout) return;
    const ws = defaultWorkspace(`Workspace ${this.layout.workspaces.length + 1}`);
    this.layout.workspaces.push(ws);
    this.layout.activeWorkspaceId = ws.id;
    this.workspaceSwitchToken += 1;
    this.saveSoon();
  }

  switchWorkspace(id: string): void {
    if (!this.layout) return;
    if (
      this.layout.workspaces.some((w) => w.id === id) &&
      this.layout.activeWorkspaceId !== id
    ) {
      this.layout.activeWorkspaceId = id;
      this.workspaceSwitchToken += 1;
      this.saveSoon();
    }
  }

  /**
   * UI-initiated workspace switch: the sidebar highlights `id` synchronously
   * while the canvas reveal commits on the next frame, so the click always
   * paints first. Rapid requests collapse; only the latest commits.
   */
  requestSwitchWorkspace(id: string): void {
    if (!this.layout) return;
    if (!this.layout.workspaces.some((w) => w.id === id)) return;
    if (id === this.layout.activeWorkspaceId) return;
    this.deferredSwitch.request(id, () => this.switchWorkspace(id));
  }

  /**
   * UI-initiated reveal: highlights the pane's workspace synchronously and
   * switches workspace+tab on the next frame. Already-visible panes reveal
   * synchronously since there is nothing to defer.
   */
  requestRevealPane(nodeId: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, nodeId);
    if (!found) return;
    if (
      found.ws.id === this.layout.activeWorkspaceId &&
      found.tab.id === found.ws.activeTabId
    ) {
      this.revealPane(nodeId);
      return;
    }
    this.deferredSwitch.request(found.ws.id, () => this.revealPane(nodeId));
  }

  renameWorkspace(id: string, name: string): void {
    const ws = this.layout?.workspaces.find((w) => w.id === id);
    if (ws && name.trim()) {
      ws.name = name.trim();
      this.saveSoon();
    }
  }

  setWorkspaceRoot(id: string, root: string): void {
    const ws = this.layout?.workspaces.find((w) => w.id === id);
    if (ws && root.trim()) {
      ws.root = root;
      this.saveSoon();
    }
  }

  clearWorkspaceRoot(id: string): void {
    const ws = this.layout?.workspaces.find((w) => w.id === id);
    if (ws && ws.root !== undefined) {
      delete ws.root;
      this.saveSoon();
    }
  }

  /** Folder the right sidebar shows for the active workspace, if any. */
  activeWorkspaceRoot(): string | undefined {
    const ws = this.workspace();
    return ws ? resolveWorkspaceRoot(ws, this.focusedPaneId) : undefined;
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
    if (this.pendingWorkspaceId === id) this.deferredSwitch.cancel();
    const closing = this.layout.workspaces.find((w) => w.id === id);
    if (closing) {
      for (const tab of closing.tabs) {
        this.pendingTerminalCommands.dropMany(collectPaneIds(tab.root));
      }
    }
    this.layout.workspaces = this.layout.workspaces.filter((w) => w.id !== id);
    if (this.layout.activeWorkspaceId === id) {
      this.layout.activeWorkspaceId =
        this.layout.workspaces[this.layout.workspaces.length - 1]?.id ?? "";
      this.workspaceSwitchToken += 1;
    }
    // Closing the last workspace reveals the empty-state overlay instead of
    // resurrecting a blank workspace.
    this.saveSoon(true);
  }

  addTab(): string | null {
    const ws = this.workspace();
    if (!ws) return null;
    const tab = defaultTab(`Tab ${ws.tabs.length + 1}`);
    if (tab.root.kind === "pane") this.applyWorkspaceDefaults(ws, tab.root);
    ws.tabs.push(tab);
    ws.activeTabId = tab.id;
    this.tabSwitchToken += 1;
    this.saveSoon();
    return tab.root.kind === "pane" ? tab.root.id : null;
  }

  /** Open a tab and queue an agent command to run in its pane. */
  addTabWithCommand(command: string): void {
    const paneId = this.addTab();
    if (paneId) this.queueAgentCommand(paneId, command);
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
    const closing =
      ws.tabs.length <= 1 ? ws.tabs : ws.tabs.filter((t) => t.id === id);
    for (const tab of closing) {
      this.pendingTerminalCommands.dropMany(collectPaneIds(tab.root));
    }
    if (ws.tabs.length <= 1) {
      ws.tabs = [defaultTab()];
      ws.activeTabId = ws.tabs[0].id;
      this.tabSwitchToken += 1;
    } else {
      ws.tabs = ws.tabs.filter((t) => t.id !== id);
      if (ws.activeTabId === id) {
        ws.activeTabId = ws.tabs[ws.tabs.length - 1].id;
        this.tabSwitchToken += 1;
      }
    }
    this.saveSoon(true);
  }

  switchTab(id: string): void {
    const ws = this.workspace();
    if (ws && ws.tabs.some((t) => t.id === id) && ws.activeTabId !== id) {
      ws.activeTabId = id;
      this.tabSwitchToken += 1;
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

  splitPane(paneId: string, dir: "row" | "col", ratio = 0.5): string | null {
    if (!this.layout) return null;
    const found = findTabByPane(this.layout, paneId);
    if (!found) return null;
    const sibling = splitPaneInTab(found.tab, paneId, dir, ratio);
    if (sibling) {
      // A split made while zoomed would hide the new sibling; show both.
      setZoomedPane(found.tab, null);
      // The sibling keeps its source's directory, else the workspace default.
      const source = findPane(found.tab.root, paneId);
      const cwd = source?.cwd ?? found.ws.defaultCwd;
      if (cwd) sibling.cwd = cwd;
      const siblingCli = implicitLaunchCommand(this.autoLaunchAgent, found.ws.defaultCli);
      if (siblingCli) this.queueAgentCommand(sibling.id, siblingCli);
      // The new pane takes focus (outline + keyboard).
      this.focusedPaneId = sibling.id;
      this.paneFocusTarget = sibling.id;
      this.saveSoon();
      return sibling.id;
    }
    return null;
  }

  /** Stamp a fresh pane with its workspace defaults (cwd + auto-run command). */
  private applyWorkspaceDefaults(ws: Workspace, node: PaneNode): void {
    if (ws.defaultCwd) node.cwd = ws.defaultCwd;
    const cli = implicitLaunchCommand(this.autoLaunchAgent, ws.defaultCli);
    if (cli) this.queueAgentCommand(node.id, cli);
  }

  setWorkspaceDefaultCli(id: string, cli: string | null): void {
    const ws = this.layout?.workspaces.find((w) => w.id === id);
    if (!ws) return;
    const trimmed = cli?.trim() ?? "";
    if (trimmed) {
      ws.defaultCli = trimmed;
      this.lastUsedAgentCli = trimmed;
      this.savePref("ubra.lastAgentCli", trimmed);
    } else delete ws.defaultCli;
    this.saveSoon();
  }

  /**
   * Agent command to prefill for a new workspace: the active workspace
   * default first, then the last used CLI, then the newest other default.
   */
  preferredAgentCli(): string {
    return pickPreferredAgentCli(
      this.layout?.workspaces ?? [],
      this.layout?.activeWorkspaceId ?? "",
      this.lastUsedAgentCli,
    );
  }

  setWorkspaceDefaultCwd(id: string, cwd: string | null): void {
    const ws = this.layout?.workspaces.find((w) => w.id === id);
    if (!ws) return;
    const trimmed = cwd?.trim() ?? "";
    if (trimmed) ws.defaultCwd = trimmed;
    else delete ws.defaultCwd;
    this.saveSoon();
  }

  /** Split and queue an agent command to run in the new sibling pane. */
  splitPaneWithCommand(paneId: string, dir: "row" | "col", command: string): void {
    const siblingId = this.splitPane(paneId, dir);
    if (siblingId) this.queueAgentCommand(siblingId, command);
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
    this.pendingTerminalCommands.drop(paneId);
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

  /**
   * Quit request from menus, shortcuts, or the tray. Warn whenever panes
   * exist (quitting stops every pane) unless the user remembered a choice.
   * `agents` is the attached-agent count for the dialog.
   */
  requestQuit(agents: number): void {
    if (this.pendingQuit) return;
    const panes = countLayoutPanes(this.layout);
    if (resolveQuitRequest(this.quitAction, panes) === "dialog") {
      this.pendingQuit = { panes, agents };
      return;
    }
    void this.quitNow();
  }

  cancelQuit(): void {
    this.pendingQuit = null;
  }

  confirmQuit(remember: boolean): void {
    if (remember) this.setQuitAction("quit");
    this.pendingQuit = null;
    void this.quitNow();
  }

  private async quitNow(): Promise<void> {
    try {
      await invoke("quit_app");
    } catch (e) {
      console.error("ubra: quit failed", e);
      toasts.push("Couldn't quit Ubra", String(e), "", { kind: "copy" });
    }
  }

  setQuitAction(action: QuitAction): void {
    this.quitAction = action;
    this.savePref(QUIT_ACTION_KEY, action);
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
    const wsChanged = this.layout.activeWorkspaceId !== found.ws.id;
    if (wsChanged) {
      this.workspaceSwitchToken += 1;
    } else if (found.ws.activeTabId !== found.tab.id) {
      // One reveal per jump: the workspace stagger covers cross-workspace
      // reveals, the tab fade covers same-workspace ones.
      this.tabSwitchToken += 1;
    }
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

  /**
   * First-run welcome, reused when onboarding again after closing the last
   * workspace. With no workspaces there is no current pane to configure, so
   * build one instead of failing.
   */
  completeOnboarding(
    projectDirectory: string | null,
    command: string | null,
  ): string | null {
    if (!this.layout) return null;
    if (this.layout.workspaces.length === 0) {
      if (!projectDirectory) return null;
      const paneId = this.createWorkspace(projectDirectory, command, "single");
      if (command?.trim()) {
        this.lastUsedAgentCli = command.trim();
        this.savePref("ubra.lastAgentCli", command.trim());
      }
      return paneId;
    }
    if (!this.firstRun) return null;

    const current = this.currentPane();
    if (!current) return null;
    const pane = findPane(current.tab.root, current.paneId);
    if (!pane) return null;
    const workspace = this.layout.workspaces.find((ws) =>
      ws.tabs.some((tab) => tab.id === current.tab.id),
    ) ?? null;
    if (projectDirectory) {
      pane.cwd = projectDirectory;
      if (workspace) {
        workspace.name = baseName(projectDirectory) || "Project";
        workspace.defaultCwd = projectDirectory;
        workspace.root = projectDirectory;
      }
    }
    this.queueAgentCommand(pane.id, command ?? "");

    if (workspace && command?.trim()) {
      workspace.defaultCli = command.trim();
      this.lastUsedAgentCli = command.trim();
      this.savePref("ubra.lastAgentCli", command.trim());
    }
    this.firstRun = false;
    this.saveSoon(true);
    return pane.id;
  }

  /**
   * Revisit-mode onboarding: open the chosen folder as a NEW workspace,
   * never mutating the current one. Returns the new pane id, or null when
   * there is no layout yet.
   */
  completeOnboardingRevisit(
    projectDirectory: string,
    command: string | null,
  ): string | null {
    if (!this.layout) return null;
    const existing = findWorkspaceByRoot(this.layout.workspaces, projectDirectory);
    if (existing) {
      this.requestSwitchWorkspace(existing.id);
      this.onboardingOpen = false;
      return collectPaneIds(activeTab(existing).root)[0] ?? null;
    }
    const paneId = this.createWorkspace(projectDirectory, command, "single");
    if (paneId && command?.trim()) {
      this.lastUsedAgentCli = command.trim();
      this.savePref("ubra.lastAgentCli", command.trim());
    }
    if (paneId) this.onboardingOpen = false;
    return paneId;
  }

  /**
   * First-run fleet launch: the rolled tab with a distinct agent CLI per
   * pane, every pane in the project folder. Commands land largest-pane
   * first, so the primary CLI takes the biggest pane and becomes the
   * workspace default. Returns the fleet pane ids in priority order, or
   * null when there is no layout to build in.
   */
  completeOnboardingFleet(
    projectDirectory: string,
    commands: string[],
    plan: FleetLaunchPlan,
  ): string[] | null {
    if (!this.layout) return null;
    if (this.layout.workspaces.length > 0 && !this.firstRun) return null;
    const planned = commands.map((command) => command.trim()).filter(Boolean);
    if (planned.length === 0 || plan.order.length === 0) return null;
    const workspace = defaultWorkspace(baseName(projectDirectory) || "Project");
    workspace.tabs = [plan.tab];
    workspace.activeTabId = plan.tab.id;
    workspace.defaultCwd = projectDirectory;
    workspace.root = projectDirectory;
    this.layout.workspaces.push(workspace);
    this.layout.activeWorkspaceId = workspace.id;
    plan.order.forEach((id, index) => {
      const node = findPane(plan.tab.root, id);
      if (node) node.cwd = projectDirectory;
      this.queueAgentCommand(id, planned[index] ?? planned[0]);
    });
    workspace.defaultCli = planned[0];
    this.lastUsedAgentCli = planned[0];
    this.savePref("ubra.lastAgentCli", planned[0]);
    this.paneFocusTarget = plan.order[0];
    this.firstRun = false;
    this.saveSoon(true);
    return [...plan.order];
  }

  async skipOnboarding(): Promise<void> {
    if (this.firstRun) {
      this.firstRun = false;
      this.saveSoon(true);
      return;
    }
    // Empty Workspace after closing the last workspace: start over at home,
    // running the first detected agent CLI. With no CLI the terminal idles.
    if (!this.layout || this.layout.workspaces.length > 0) return;
    let home: string | null = null;
    try {
      home = await invoke<string>("home_dir");
    } catch (e) {
      console.error("ubra: home directory lookup failed", e);
    }
    if (!home) {
      this.addBlankWorkspace();
      return;
    }
    const command = implicitLaunchCommand(
      this.autoLaunchAgent,
      (await agentClis.ensure())[0]?.cli ?? null,
    );
    this.createWorkspace(home, command, "single");
    if (command) {
      this.lastUsedAgentCli = command;
      this.savePref("ubra.lastAgentCli", command);
    }
  }

  takePendingTerminalCommand(paneId: string): string | null {
    return this.pendingTerminalCommands.take(paneId);
  }

  /**
   * Queue an agent command to type on the pane's next spawn, and stamp the
   * pane so a fresh spawn after a restart can rerun it. Callers save.
   */
  private queueAgentCommand(paneId: string, command: string): void {
    this.pendingTerminalCommands.queue(paneId, command);
    if (this.layout) stampPaneAgentCli(this.layout, paneId, command);
  }

  /**
   * The pane's persisted agent CLI for a fresh (non-attached) spawn. Unlike
   * the pending queue this is not consumed: rerunning the same agent on
   * every fresh spawn is the point. Null when the pane never ran an agent.
   */
  /** Explicit relaunch for the respawn button; bypasses the auto-launch gate. */
  relaunchPaneAgent(paneId: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, paneId);
    const node = found ? findPane(found.tab.root, paneId) : null;
    const cli = node?.agentCli?.trim();
    if (cli && node) this.queueAgentCommand(paneId, restoreCommandFor(cli, node.agentSession));
  }

  takeRestoreAgent(paneId: string): string | null {
    if (!this.layout || !this.autoLaunchAgent) return null;
    const found = findTabByPane(this.layout, paneId);
    const node = found ? findPane(found.tab.root, paneId) : null;
    const cli = node?.agentCli?.trim();
    return cli && node ? restoreCommandFor(cli, node.agentSession) : null;
  }

  /**
   * Stamp an observed agent session for resume-after-restart. True when
   * the layout changes; callers save.
   */
  stampPaneAgentSession(paneId: string, cli: string, value: string): boolean {
    return !!this.layout && stampPaneAgentSession(this.layout, paneId, cli, value);
  }

  /**
   * Reap backend sessions no layout pane can adopt: keys from panes that no
   * longer exist (or pre-reattach orphans). Keyless sessions are left
   * alone. Best-effort; failures only log.
   */
  private async sweepOrphanedPtys(): Promise<void> {
    const layout = this.layout;
    if (!layout) return;
    try {
      const sessions = await invoke<PtySessionInfo[]>("pty_list");
      const placed = new Set<string>();
      for (const ws of layout.workspaces) {
        for (const tab of ws.tabs) {
          for (const id of collectPaneIds(tab.root)) placed.add(id);
        }
      }
      for (const session of sessions) {
        if (session.key !== null && session.key !== undefined && !placed.has(session.key)) {
          await invoke("pty_kill", { id: session.id }).catch((error) => {
            console.error("ubra: orphan reap failed", error);
          });
        }
      }
    } catch (error) {
      console.error("ubra: orphan sweep failed", error);
    }
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
    // Cycle from the pending target while a switch is in flight so rapid
    // key repeats walk forward instead of re-requesting the same workspace.
    const from = this.pendingWorkspaceId ?? this.layout.activeWorkspaceId;
    const at = all.findIndex((w) => w.id === from);
    const cur = at < 0 ? 0 : at;
    this.requestSwitchWorkspace(all[(cur + dir + all.length) % all.length].id);
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

  /** Resolve the spawn argv for a pane; throws when the pane is gone. */
  commandForSpawn(paneId: string): string[] | undefined {
    if (!this.layout) return undefined;
    const found = findTabByPane(this.layout, paneId);
    const node = found ? findPane(found.tab.root, paneId) : null;
    if (!node) throw new Error("Pane is no longer in the layout.");
    return this.startup.resolve(node);
  }

  /** Authorize a single current configured-command pane (explicit rerun). */
  authorizePaneCommand(paneId: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, paneId);
    const node = found ? findPane(found.tab.root, paneId) : null;
    if (node?.cmd !== undefined && node.cmd.length > 0) this.startup.authorize([paneId]);
  }
}

export const store = new AppStore();

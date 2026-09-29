import { invoke } from "@tauri-apps/api/core";
import {
  activeTab,
  activeWorkspace,
  clearStaleZoom,
  closePaneInTab,
  collectPaneIds,
  countPanes,
  defaultLayout,
  defaultTab,
  defaultWorkspace,
  findPane,
  findTabByPane,
  sanitizeLayout,
  setZoomedPane,
  splitPaneInTab,
  type Layout,
  type Tab,
  type Workspace,
} from "./layout";
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

export const DEFAULT_TERM_FONT_SIZE = 13;
export const MIN_TERM_FONT_SIZE = 9;
export const MAX_TERM_FONT_SIZE = 24;

class AppStore {
  layout = $state<Layout | null>(null);
  loaded = $state(false);
  loadError = $state<string | null>(null);
  themeId = $state<ThemeId>(DEFAULT_THEME_ID);
  termFontSize = $state<number>(DEFAULT_TERM_FONT_SIZE);
  notifyDelivery = $state<NotifyDelivery>(DEFAULT_DELIVERY);
  toastPosition = $state<ToastPosition>(DEFAULT_TOAST_POSITION);
  soundEnabled = $state<boolean>(true);
  /** Lowercase agent clis muted for sounds (Herdr mutes droid by default). */
  mutedAgents = $state<string[]>(["droid"]);
  /** Custom notification sound file (blank = synthesized default chime). */
  soundFile = $state<string>("");
  settingsOpen = $state(false);
  /** Last-focused pane node id (session-only, for shortcuts). */
  focusedPaneId = $state<string | null>(null);
  /** Pane node id that should enter rename editing (F2); cleared on take. */
  paneRenameTarget = $state<string | null>(null);
  private saveTimer: ReturnType<typeof setTimeout> | null = null;

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
    } catch {
      // The app can still start with its defaults if storage is unavailable.
    }

    try {
      const raw = await invoke<unknown>("load_layout");
      if (raw == null) {
        this.layout = defaultLayout();
        this.saveSoon();
      } else {
        this.layout = sanitizeLayout(raw);
      }
    } catch (e) {
      this.loadError = e instanceof Error ? e.message : String(e);
      this.layout = defaultLayout();
    } finally {
      this.loaded = true;
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
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = null;
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
    if (!this.layout) return;
    try {
      await invoke("save_layout", { layout: this.layout });
    } catch (e) {
      console.error("ubra: failed to save layout", e);
    }
  }

  addWorkspace(): void {
    if (!this.layout) return;
    const ws = defaultWorkspace(`Workspace ${this.layout.workspaces.length + 1}`);
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

  closeWorkspace(id: string): void {
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

  closeTab(id: string): void {
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

  splitPane(paneId: string, dir: "row" | "col"): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, paneId);
    if (found && splitPaneInTab(found.tab, paneId, dir)) {
      // A split made while zoomed would hide the new sibling; show both.
      setZoomedPane(found.tab, null);
      this.saveSoon();
    }
  }

  closePane(paneId: string): void {
    if (!this.layout) return;
    const found = findTabByPane(this.layout, paneId);
    if (found && closePaneInTab(found.tab, paneId)) {
      clearStaleZoom(found.tab);
      this.saveSoon(true);
    }
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
  closePaneOrTab(): void {
    const cur = this.currentPane();
    if (!cur) return;
    if (countPanes(cur.tab.root) > 1) this.closePane(cur.paneId);
    else this.closeTab(cur.tab.id);
  }

  requestPaneRename(): void {
    const cur = this.currentPane();
    if (cur) this.paneRenameTarget = cur.paneId;
  }

  paneSizesChanged(): void {
    this.saveSoon();
  }
}

export const store = new AppStore();

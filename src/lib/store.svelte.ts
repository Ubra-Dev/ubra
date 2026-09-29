import { invoke } from "@tauri-apps/api/core";
import {
  activeTab,
  activeWorkspace,
  clearStaleZoom,
  closePaneInTab,
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
  DEFAULT_THEME_ID,
  THEMES,
  isThemeId,
  type ThemeId,
} from "./themes";

class AppStore {
  layout = $state<Layout | null>(null);
  loaded = $state(false);
  loadError = $state<string | null>(null);
  themeId = $state<ThemeId>(DEFAULT_THEME_ID);
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
    } catch {
      // The app can still start with its default theme if storage is unavailable.
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

  paneSizesChanged(): void {
    this.saveSoon();
  }
}

export const store = new AppStore();

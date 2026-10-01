import { invoke } from "@tauri-apps/api/core";
import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu } from "@tauri-apps/api/menu";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { acceleratorFor, canDispatch, menuGroups, type AppCommand, type CommandContext } from "./appCommands";
import { dispatchCommand } from "./appCommandRuntime";
import { toasts } from "./toasts.svelte.ts";

export interface MenuSnapshot {
  context: CommandContext;
  workspaces: Array<{ id: string; name: string }>;
  activeWorkspaceId: string;
  fullscreen: boolean;
}

let owner: NativeMenuController | null = null;
let latest: NativeMenuController | null = null;
const accelerators = new Set<AppCommand>();
export function nativeOwnsShortcut(action: AppCommand): boolean {
  return !!owner?.ready && accelerators.has(action);
}

/** Native mutations are serialized; pending updates are replaced by the latest snapshot. */
export class NativeMenuController {
  ready = false;
  private disposed = false;
  private menu: Menu | null = null;
  private previous: Menu | null = null;
  private workspaceMenu: Submenu | null = null;
  private resources: Array<{ close(): Promise<void> }> = [];
  private commands = new Map<AppCommand, MenuItem | CheckMenuItem>();
  private cache = new Map<AppCommand, string>();
  private workspaceItems: CheckMenuItem[] = [];
  private workspaceSignature = "";
  private workspaceChecks = new Map<string, string>();
  private pending: MenuSnapshot | null = null;
  private draining: Promise<void> | null = null;
  private starting: Promise<void> | null = null;
  private releasing: Promise<void> | null = null;

  constructor(private readonly isMac: boolean, private readonly invalidate: () => void) {}

  private own<T extends { close(): Promise<void> }>(resource: T): T {
    this.resources.push(resource);
    return resource;
  }

  start(): Promise<void> {
    if (!this.starting) {
      const predecessor = latest;
      latest = this;
      this.starting = this.initialize(predecessor);
    }
    return this.starting;
  }

  private async initialize(predecessor: NativeMenuController | null): Promise<void> {
    try {
      // A replacement waits for initialization AND cleanup of its predecessor.
      // Otherwise a late unmount can restore a menu over the new controller.
      await predecessor?.dispose();
      if (this.disposed) return;
      const info = await invoke<{ name: string; version: string }>("app_info");
      const groups: Submenu[] = [];
      let windowMenu: Submenu | undefined;
      let helpMenu: Submenu | undefined;
      for (const group of menuGroups(this.isMac)) {
        const items: Array<MenuItem | CheckMenuItem | PredefinedMenuItem | Submenu> = [];
        for (const entry of group.items) {
          if (!entry) {
            items.push(this.own(await PredefinedMenuItem.new({ item: "Separator" })));
          } else if (entry.system) {
            items.push(this.own(await PredefinedMenuItem.new({
              text: entry.label,
              item: entry.system === "About" ? { About: { name: info.name, version: info.version } } : entry.system,
            })));
          } else if (entry.action) {
            const action = entry.action;
            const options = { id: `app:${entry.id}`, text: entry.label, enabled: false,
              action: () => { if (!this.disposed) void dispatchCommand({ action }).then(() => this.invalidate()); } };
            const item = this.own(entry.checked === undefined
              ? await MenuItem.new(options) : await CheckMenuItem.new({ ...options, checked: false }));
            this.commands.set(action, item);
            items.push(item);
          }
        }
        if (group.label === "Window") {
          this.workspaceMenu = this.own(await Submenu.new({ text: "Workspaces", items: [] }));
          items.push(this.workspaceMenu);
        }
        const submenu = this.own(await Submenu.new({ text: group.label, items }));
        groups.push(submenu);
        if (group.label === "Window") windowMenu = submenu;
        if (group.label === "Help") helpMenu = submenu;
      }
      this.menu = this.own(await Menu.new({ items: groups }));
      if (this.disposed) return;
      this.previous = this.isMac ? await this.menu.setAsAppMenu() : await this.menu.setAsWindowMenu();
      owner = this;
      accelerators.clear();
      if (this.isMac) {
        await windowMenu?.setAsWindowsMenuForNSApp();
        await helpMenu?.setAsHelpMenuForNSApp();
      }
      if (this.disposed) return;
      this.ready = true;
      this.invalidate();
      this.drain();
    } catch (error) {
      console.error("ubra: native menu initialization failed", error);
      toasts.push("Couldn't initialize application menus", String(error), "", { kind: "copy" });
      this.disposed = true;
    } finally {
      if (this.disposed) await this.release();
    }
  }

  sync(snapshot: MenuSnapshot): void {
    if (this.disposed) return;
    this.pending = snapshot;
    this.drain();
  }

  private drain(): void {
    if (!this.ready || this.draining || this.disposed) return;
    this.draining = (async () => {
      while (this.pending && !this.disposed) {
        const snapshot = this.pending;
        this.pending = null;
        await this.apply(snapshot);
      }
    })().catch(async (error) => {
      console.error("ubra: native menu update failed", error);
      // Release shortcut ownership so the DOM dispatcher stays usable.
      this.ready = false;
      if (owner === this) accelerators.clear();
      toasts.push("Couldn't update application menus", String(error), "", { kind: "copy" });
      this.disposed = true;
      await this.release();
    }).finally(() => { this.draining = null; });
  }

  private async apply(snapshot: MenuSnapshot): Promise<void> {
    const { context } = snapshot;
    for (const [action, item] of this.commands) {
      const enabled = canDispatch({ action }, context);
      const accelerator = enabled ? acceleratorFor(action, this.isMac) : null;
      const text = action === "restart-terminal" ? context.savedCommand ? "Run Saved Command" : "Restart Terminal" :
        action === "toggle-zoom" ? context.zoomed ? "Unzoom Pane" : "Zoom Pane" : null;
      const checked = action === "fullscreen" ? snapshot.fullscreen : undefined;
      const signature = JSON.stringify([enabled, accelerator, text, checked]);
      if (this.cache.get(action) === signature) continue;
      await item.setAccelerator(null);
      if (owner === this) accelerators.delete(action);
      await item.setEnabled(enabled);
      if (text) await item.setText(text);
      if (checked !== undefined && item instanceof CheckMenuItem) await item.setChecked(checked);
      if (accelerator) {
        await item.setAccelerator(accelerator);
        if (owner === this) accelerators.add(action);
      }
      this.cache.set(action, signature);
    }
    const signature = JSON.stringify(snapshot.workspaces);
    if (this.workspaceMenu && signature !== this.workspaceSignature) {
      for (const item of this.workspaceItems) { await this.workspaceMenu.remove(item); await item.close(); }
      this.workspaceItems = [];
      this.workspaceChecks.clear();
      for (const ws of snapshot.workspaces) {
        const checked = ws.id === snapshot.activeWorkspaceId;
        const enabled = canDispatch({ action: "switch-workspace", workspaceId: ws.id }, context);
        const item = await CheckMenuItem.new({
          id: `workspace:${ws.id}`, text: this.isMac ? ws.name : ws.name.replace(/&/g, "&&"),
          checked, enabled,
          action: () => { if (!this.disposed) void dispatchCommand({ action: "switch-workspace", workspaceId: ws.id }); },
        });
        this.workspaceItems.push(item);
        await this.workspaceMenu.append(item);
        this.workspaceChecks.set(ws.id, JSON.stringify([checked, enabled]));
      }
      await this.workspaceMenu.setEnabled(snapshot.workspaces.length > 0);
      this.workspaceSignature = signature;
    }
    // Focus and active-workspace changes only update existing items.
    for (const [index, ws] of snapshot.workspaces.entries()) {
      const item = this.workspaceItems[index];
      const checked = ws.id === snapshot.activeWorkspaceId;
      const enabled = canDispatch({ action: "switch-workspace", workspaceId: ws.id }, context);
      const state = JSON.stringify([checked, enabled]);
      if (item && this.workspaceChecks.get(ws.id) !== state) {
        await item.setChecked(checked);
        await item.setEnabled(enabled);
        this.workspaceChecks.set(ws.id, state);
      }
    }
  }

  async dispose(): Promise<void> {
    this.disposed = true;
    this.pending = null;
    this.ready = false;
    if (owner === this) accelerators.clear();
    await this.starting;
    await this.draining;
    await this.release();
  }

  private release(): Promise<void> {
    return this.releasing ??= this.releaseResources();
  }

  private async releaseResources(): Promise<void> {
    try {
      if (owner === this) {
        this.ready = false;
        accelerators.clear();
        // A window can initially have no menu on Windows/Linux.
        const previous = this.previous ?? await Menu.default();
        try {
          const replaced = this.isMac ? await previous.setAsAppMenu() : await previous.setAsWindowMenu(getCurrentWindow());
          await replaced?.close();
        } finally { await previous.close(); }
      }
    } catch (error) {
      console.error("ubra: native menu cleanup failed", error);
      // Even if restoring the previous menu fails, remove our accelerators.
      for (const item of this.commands.values()) {
        await item.setAccelerator(null).catch(console.error);
        await item.setEnabled(false).catch(console.error);
      }
    } finally {
      if (owner === this) owner = null;
      if (latest === this) latest = null;
      this.previous = null;
      for (const resource of [...this.workspaceItems, ...this.resources.reverse()]) {
        await resource.close().catch(console.error);
      }
      this.workspaceItems = [];
      this.resources = [];
    }
  }
}

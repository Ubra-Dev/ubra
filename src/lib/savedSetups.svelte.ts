import { invoke } from "@tauri-apps/api/core";
import {
  emptySavedSetups,
  sanitizeSavedSetups,
  type SavedSetups,
} from "./savedSetups";

class SavedSetupsStore {
  library = $state<SavedSetups | null>(null);
  busy = $state(false);
  loadError = $state<string | null>(null);
  saveError = $state<string | null>(null);
  backupPath = $state<string | null>(null);
  private loading: Promise<void> | null = null;

  async load(): Promise<void> {
    if (this.loading) return this.loading;
    this.loading = (async () => {
      this.busy = true;
      try {
        const raw = await invoke<unknown>("load_saved_setups");
        this.library = raw == null ? emptySavedSetups() : sanitizeSavedSetups(raw);
        this.loadError = null;
      } catch (e) {
        this.library = null;
        this.loadError = e instanceof Error ? e.message : String(e);
      } finally {
        this.busy = false;
        this.loading = null;
      }
    })();
    return this.loading;
  }

  async save(next: SavedSetups): Promise<boolean> {
    if (this.busy) return false;
    this.busy = true;
    this.saveError = null;
    try {
      const validated = sanitizeSavedSetups(JSON.parse(JSON.stringify(next)));
      await invoke("save_saved_setups", { setups: validated });
      this.library = validated;
      return true;
    } catch (e) {
      this.saveError = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      this.busy = false;
    }
  }

  async exportOriginal(): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    try {
      this.backupPath = await invoke<string>("export_saved_setups");
    } catch (e) {
      this.saveError = e instanceof Error ? e.message : String(e);
    } finally {
      this.busy = false;
    }
  }

  async reset(): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    this.saveError = null;
    try {
      const fresh = emptySavedSetups();
      const backup = await invoke<string | null>("reset_saved_setups", { setups: fresh });
      if (backup) this.backupPath = backup;
      this.library = fresh;
    } catch (e) {
      this.saveError = e instanceof Error ? e.message : String(e);
    } finally {
      this.busy = false;
    }
  }
}

export const savedSetups = new SavedSetupsStore();

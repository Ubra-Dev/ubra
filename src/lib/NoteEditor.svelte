<script lang="ts">
  import { Crepe } from "@milkdown/crepe";
  import "@milkdown/crepe/theme/common/style.css";
  import { onDestroy, onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import {
    readNote,
    writeNote,
    type NoteEntry,
    type NoteScope,
  } from "./notes";
  import { toasts } from "./toasts.svelte.ts";

  interface Props {
    scope: NoteScope;
    workspaceId: string | null;
    name: string;
    /** Called when the user navigates back to the list. */
    onBack: () => void;
    /** Called after every successful save with the fresh entry. */
    onSaved: (entry: NoteEntry) => void;
  }
  let { scope, workspaceId, name, onBack, onSaved }: Props = $props();

  /** Idle time after the last keystroke before autosaving. */
  const AUTOSAVE_MS = 800;

  let rootEl: HTMLDivElement | undefined = $state(undefined);
  let crepe: Crepe | null = null;
  let starting = $state(true);
  let startError = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let dirty = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  // Bumped on every save/back/unmount so a stale async completion can never
  // clear a newer dirty flag or touch a destroyed editor.
  let token = 0;

  function currentMarkdown(): string | null {
    try {
      return crepe?.getMarkdown() ?? null;
    } catch {
      return null;
    }
  }

  async function persist(markdown: string, mine: number): Promise<boolean> {
    saving = true;
    saveError = null;
    try {
      const entry = await writeNote(scope, workspaceId, name, markdown);
      if (mine !== token) return false;
      dirty = false;
      onSaved(entry);
      return true;
    } catch (e) {
      if (mine !== token) return false;
      saveError = e instanceof Error ? e.message : String(e);
      toasts.push("Couldn't save note", saveError, "");
      return false;
    } finally {
      if (mine === token) saving = false;
    }
  }

  function scheduleAutosave(): void {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      saveTimer = null;
      const markdown = currentMarkdown();
      if (markdown === null) return;
      void persist(markdown, token);
    }, AUTOSAVE_MS);
  }

  function onEdit(): void {
    dirty = true;
    saveError = null;
    scheduleAutosave();
  }

  /** Flush a pending autosave and wait for it (Back navigation). */
  async function flush(): Promise<void> {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (!dirty || saving) return;
    const markdown = currentMarkdown();
    if (markdown === null) return;
    await persist(markdown, token);
  }

  onMount(() => {
    const mine = token;
    let cancelled = false;
    void (async () => {
      let content: string;
      try {
        content = (await readNote(scope, workspaceId, name)).content;
      } catch (e) {
        if (!cancelled) {
          startError = e instanceof Error ? e.message : String(e);
          starting = false;
        }
        return;
      }
      if (cancelled || !rootEl) return;
      try {
        const editor = new Crepe({
          root: rootEl,
          defaultValue: content,
          features: {
            // Attachments and LaTeX are non-goals for v1 notes.
            [Crepe.Feature.ImageBlock]: false,
            [Crepe.Feature.Latex]: false,
          },
          featureConfigs: {
            [Crepe.Feature.Placeholder]: { text: "Start writing…", mode: "block" },
          },
        });
        editor.on((listener) => {
          listener.markdownUpdated(() => onEdit());
        });
        crepe = editor;
        await editor.create();
      } catch (e) {
        if (!cancelled) {
          startError = e instanceof Error ? e.message : String(e);
        }
        return;
      }
      if (cancelled) {
        // Unmounted while starting: tear down the orphan, don't paint it.
        const orphan = crepe;
        crepe = null;
        void orphan?.destroy().catch(() => {});
        return;
      }
      if (mine === token) starting = false;
    })();
    return () => {
      cancelled = true;
    };
  });

  onDestroy(() => {
    token += 1;
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    // Best-effort flush: view switches remount us, and the IPC call usually
    // completes even though unmount doesn't wait for it.
    if (dirty && !saving) {
      const markdown = currentMarkdown();
      if (markdown !== null) void writeNote(scope, workspaceId, name, markdown);
    }
    const editor = crepe;
    crepe = null;
    void editor?.destroy().catch(() => {});
  });

  async function back(): Promise<void> {
    await flush();
    onBack();
  }

  function retry(): void {
    startError = null;
    starting = true;
    // Simplest reliable restart: remount through the parent key.
    onBack();
  }
</script>

<div class="editor">
  <div class="head">
    <button class="icon-btn" title="Back to notes" aria-label="Back to notes" onclick={() => void back()}>
      <Icon name="chevron-left" size={14} />
    </button>
    <span class="title">{name}</span>
    <span class="state">
      {#if saving}Saving…{:else if saveError}Save failed{:else if dirty}Unsaved{:else}Saved{/if}
    </span>
  </div>
  {#if starting}
    <div class="status">Loading…</div>
  {:else if startError}
    <div class="status error">
      <span>{startError}</span>
      <button class="retry" onclick={retry}>Back to list</button>
    </div>
  {/if}
  <!-- The editor root stays mounted once created; hidden only while (re)starting. -->
  <div class="milkdown-wrap" class:hidden={starting || startError !== null} bind:this={rootEl}></div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1 1 auto;
  }
  .head {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 4px;
    padding: 6px 6px 6px 4px;
  }
  .title {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-strong);
    font-weight: 600;
  }
  .state {
    flex: 0 0 auto;
    color: var(--text-subtle);
    font-size: 11px;
    padding-right: 4px;
  }
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 26px;
    height: 26px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .icon-btn:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .milkdown-wrap {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: thin;
  }
  .milkdown-wrap.hidden {
    display: none;
  }
  /* Map Crepe's theme tokens onto Ubra's so notes follow the app theme. */
  .milkdown-wrap :global(.milkdown) {
    --crepe-color-background: var(--sidebar-bg);
    --crepe-color-on-background: var(--text);
    --crepe-color-surface: var(--surface-bg);
    --crepe-color-surface-low: var(--surface-bg);
    --crepe-color-on-surface: var(--text-strong);
    --crepe-color-on-surface-variant: var(--text-muted);
    --crepe-color-outline: var(--border);
    --crepe-color-primary: var(--accent);
    --crepe-color-secondary: var(--surface-active);
    --crepe-color-on-secondary: var(--text-strong);
    --crepe-color-inline-code: var(--accent);
    --crepe-color-error: var(--error-text, #f87171);
    --crepe-color-hover: var(--surface-bg);
    --crepe-color-selected: var(--surface-active);
    --crepe-color-inline-area: var(--surface-active);
    --crepe-base-font-size: 13px;
    --crepe-font-default: var(--font-ui, system-ui, sans-serif);
    --crepe-font-title: var(--font-ui, system-ui, sans-serif);
    --crepe-font-code: ui-monospace, SFMono-Regular, Menlo, monospace;
    padding: 4px 12px 24px;
  }
  .milkdown-wrap :global(.milkdown .ProseMirror) {
    padding: 0;
  }
  .milkdown-wrap :global(.milkdown .ProseMirror:focus) {
    outline: none;
  }
  .status {
    padding: 12px;
    color: var(--text-muted);
  }
  .status.error {
    color: var(--error-text, #f87171);
  }
  .retry {
    margin-left: 6px;
    background: var(--surface-bg);
    color: var(--text-strong);
    border: 1px solid var(--border);
    border-radius: 4px;
    font: inherit;
    font-size: 11px;
    padding: 1px 8px;
    cursor: pointer;
  }
  .retry:hover {
    border-color: var(--accent);
  }
</style>

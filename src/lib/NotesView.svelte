<script module lang="ts">
  import type { NoteScope } from "./notes";

  // View switches (Explorer/Source Control) remount this component; remember
  // the open note per workspace so returning reopens it instead of the list.
  const openCache = new Map<string, { scope: NoteScope; name: string }>();
</script>

<script lang="ts">
  import { onMount, untrack } from "svelte";
  import ContextMenu, { announceMenuOpen } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";
  import type NoteEditorType from "./NoteEditor.svelte";
  import {
    deleteNote,
    formatNoteDate,
    listNotes,
    newNoteName,
    renameNote,
    searchNotes,
    writeNote,
    type NoteEntry,
    type NoteHit,
  } from "./notes";
  import { toasts } from "./toasts.svelte.ts";

  interface Props {
    workspaceId: string;
  }
  let { workspaceId }: Props = $props();

  function loadScope(): NoteScope {
    try {
      return window.localStorage.getItem("ubra.notesScope") === "global"
        ? "global"
        : "workspace";
    } catch {
      return "workspace";
    }
  }

  let scope = $state<NoteScope>(loadScope());
  let entries = $state<NoteEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let truncated = $state(false);
  let query = $state("");
  let hits = $state<NoteHit[]>([]);
  let searching = $state(false);
  let searchTruncated = $state(false);
  let creating = $state(false);
  // Snapshot on purpose: the parent remounts us per workspace, so the
  // initial id is the only one this instance ever sees.
  const initialOpen = untrack(() => openCache.get(workspaceId) ?? null);
  let editing = $state<{ scope: NoteScope; name: string } | null>(initialOpen);
  let menu = $state<{ name: string; x: number; y: number; opener: HTMLElement | null } | null>(null);
  let renaming = $state<{ name: string; draft: string } | null>(null);
  // The Milkdown editor loads on first open so its chunk never weighs
  // down app start; the type-only import above keeps this code-split.
  let EditorComp = $state<typeof NoteEditorType | null>(null);
  let editorFailed = $state(false);

  $effect(() => {
    if (!editing || EditorComp || editorFailed) return;
    import("./NoteEditor.svelte")
      .then((m) => {
        EditorComp = m.default;
      })
      .catch((e: unknown) => {
        editorFailed = true;
        toasts.push(
          "Couldn't load the editor",
          e instanceof Error ? e.message : String(e),
          "",
        );
      });
  });

  const showingSearch = $derived(query.trim() !== "");
  let token = 0;

  async function load(): Promise<void> {
    const mine = ++token;
    loading = true;
    error = null;
    try {
      const listing = await listNotes(scope, workspaceId);
      if (mine !== token) return;
      entries = listing.entries;
      truncated = listing.truncated;
    } catch (e) {
      if (mine !== token) return;
      error = e instanceof Error ? e.message : String(e);
    } finally {
      if (mine === token) loading = false;
    }
  }

  onMount(() => {
    // Let the reveal frame paint before kicking off IPC (mirrors ExplorerView).
    const frame = requestAnimationFrame(() => void load());
    return () => cancelAnimationFrame(frame);
  });

  function flipScope(): void {
    scope = scope === "workspace" ? "global" : "workspace";
    try {
      window.localStorage.setItem("ubra.notesScope", scope);
    } catch {
      // Scope preference is best-effort; the list still reloads.
    }
    void load();
  }

  // Debounced search across global + this workspace's notes.
  $effect(() => {
    const q = query;
    if (q.trim() === "") {
      hits = [];
      searching = false;
      return;
    }
    searching = true;
    const timer = setTimeout(() => void runSearch(q), 200);
    return () => clearTimeout(timer);
  });

  async function runSearch(q: string): Promise<void> {
    try {
      const results = await searchNotes(q, workspaceId);
      if (q !== query) return;
      hits = results.hits;
      searchTruncated = results.truncated;
    } catch (e) {
      if (q !== query) return;
      hits = [];
      searchTruncated = false;
      toasts.push("Couldn't search notes", e instanceof Error ? e.message : String(e), "");
    } finally {
      if (q === query) searching = false;
    }
  }

  function openNote(noteScope: NoteScope, name: string): void {
    const open = { scope: noteScope, name };
    editing = open;
    openCache.set(workspaceId, open);
    menu = null;
  }

  function closeEditor(): void {
    editing = null;
    openCache.delete(workspaceId);
  }

  function resort(): void {
    entries = [...entries].sort((a, b) => {
      const time = (b.updatedMs ?? 0) - (a.updatedMs ?? 0);
      return time !== 0 ? time : a.name.localeCompare(b.name);
    });
  }

  function onSaved(entry: NoteEntry): void {
    if (!editing || entry.name !== editing.name) return;
    entries = entries.map((e) => (e.name === entry.name ? entry : e));
    resort();
  }

  async function create(): Promise<void> {
    if (creating) return;
    creating = true;
    try {
      const taken = new Set(entries.map((e) => e.name));
      const name = newNoteName("new note", (n) => taken.has(n));
      const entry = await writeNote(scope, workspaceId, name, "# New note\n");
      entries = [entry, ...entries];
      openNote(scope, name);
    } catch (e) {
      toasts.push("Couldn't create note", e instanceof Error ? e.message : String(e), "");
    } finally {
      creating = false;
    }
  }

  async function remove(name: string): Promise<void> {
    menu = null;
    try {
      await deleteNote(scope, workspaceId, name);
      entries = entries.filter((e) => e.name !== name);
      if (editing?.name === name && editing.scope === scope) closeEditor();
      toasts.push("Note deleted", name, "");
    } catch (e) {
      toasts.push("Couldn't delete note", e instanceof Error ? e.message : String(e), "");
    }
  }

  /** Client-side mirror of the backend name rules for instant feedback. */
  function validName(draft: string): boolean {
    if (draft === "" || draft.length > 100) return false;
    if (draft.startsWith(".") || draft.includes("..")) return false;
    return /^[A-Za-z0-9 _.\-]+$/.test(draft);
  }

  function commitRename(): void {
    const r = renaming;
    renaming = null;
    if (!r) return;
    const draft = r.draft.trim();
    if (draft === r.name) return;
    if (!validName(draft)) {
      toasts.push("Invalid note name", "Use letters, numbers, spaces, dots, dashes.", "");
      return;
    }
    if (entries.some((e) => e.name === draft)) {
      toasts.push("Couldn't rename note", "A note with that name already exists.", "");
      return;
    }
    renameNote(scope, workspaceId, r.name, draft)
      .then((entry) => {
        entries = entries.map((e) => (e.name === r.name ? entry : e));
        if (editing?.name === r.name && editing.scope === scope) {
          openNote(scope, draft);
        }
      })
      .catch((e: unknown) =>
        toasts.push("Couldn't rename note", e instanceof Error ? e.message : String(e), ""),
      );
  }

  function openMenu(e: MouseEvent, name: string): void {
    e.preventDefault();
    e.stopPropagation();
    announceMenuOpen();
    menu = { name, x: e.clientX, y: e.clientY, opener: e.currentTarget as HTMLElement };
  }

  function onPick(action: string): void {
    const m = menu;
    menu = null;
    if (!m) return;
    if (action === "open") openNote(scope, m.name);
    else if (action === "rename") renaming = { name: m.name, draft: m.name };
    else if (action === "delete") void remove(m.name);
  }

  function focus(el: HTMLInputElement): void {
    el.focus();
    el.select();
  }
</script>

<div class="notes">
  {#if editing}
    {#if EditorComp}
      {#key editing.scope + editing.name}
        <EditorComp
          scope={editing.scope}
          workspaceId={workspaceId}
          name={editing.name}
          onBack={closeEditor}
          onSaved={onSaved}
        />
      {/key}
    {:else}
      <div class="head">
        <button
          class="icon-btn"
          title="Back to notes"
          aria-label="Back to notes"
          onclick={closeEditor}
        >
          <Icon name="chevron-left" size={14} />
        </button>
      </div>
      {#if editorFailed}
        <div class="status error">Couldn't load the editor. Try again.</div>
      {:else}
        <div class="status">Loading editor…</div>
      {/if}
    {/if}
  {:else}
    <div class="head">
      <button
        class="scope"
        title={scope === "workspace" ? "Project notes — click for global notes" : "Global notes — click for project notes"}
        onclick={flipScope}
      >
        <Icon name="book" size={13} />
        <span>{scope === "workspace" ? "Project" : "Global"}</span>
      </button>
      <button
        class="icon-btn"
        title="Refresh"
        aria-label="Refresh"
        onclick={() => void load()}
      >
        <Icon name="refresh" size={13} />
      </button>
      <button
        class="icon-btn"
        title="New note"
        aria-label="New note"
        disabled={creating}
        onclick={() => void create()}
      >
        <Icon name="plus" size={13} />
      </button>
    </div>
    <div class="search">
      <Icon name="search" size={13} />
      <input
        type="search"
        placeholder="Search notes"
        aria-label="Search notes"
        bind:value={query}
      />
    </div>
    <div class="list" role="listbox" aria-label="Notes">
      {#if showingSearch}
        {#if hits.length === 0}
          <div class="status dim">{searching ? "Searching…" : "No matches"}</div>
        {:else}
          {#each hits as hit (hit.scope + hit.name)}
            <button
              class="row"
              title={hit.snippet}
              onclick={() => openNote(hit.scope === "global" ? "global" : "workspace", hit.name)}
            >
              <span class="row-title">{hit.title}</span>
              <span class="row-sub">
                <span class="badge">{hit.scope === "global" ? "Global" : "Project"}</span>
                <span class="snippet">{hit.snippet}</span>
              </span>
            </button>
          {/each}
          {#if searchTruncated}
            <div class="status dim">More matches hidden — refine the search</div>
          {/if}
        {/if}
      {:else if loading}
        <div class="status">Loading…</div>
      {:else if error}
        <div class="status error">
          <span>{error}</span>
          <button class="retry" onclick={() => void load()}>Retry</button>
        </div>
      {:else if entries.length === 0}
        <div class="empty">
          <p>No notes yet.</p>
          <button class="choose" disabled={creating} onclick={() => void create()}>
            {creating ? "Creating…" : "New note"}
          </button>
        </div>
      {:else}
        {#each entries as entry (entry.name)}
          {#if renaming?.name === entry.name}
            <div class="rename">
              <input
                use:focus
                bind:value={renaming.draft}
                aria-label="Note name"
                onblur={commitRename}
                onkeydown={(e) => {
                  if (e.key === "Enter") commitRename();
                  if (e.key === "Escape") renaming = null;
                }}
              />
            </div>
          {:else}
            <button
              class="row"
              title={entry.name}
              onclick={() => openNote(scope, entry.name)}
              oncontextmenu={(e) => openMenu(e, entry.name)}
            >
              <span class="row-title">{entry.title}</span>
              <span class="row-sub">
                <span class="date">{formatNoteDate(entry.updatedMs)}</span>
              </span>
            </button>
          {/if}
        {/each}
        {#if truncated}
          <div class="status dim">List truncated — too many notes to show</div>
        {/if}
      {/if}
    </div>
  {/if}
  {#if menu}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      opener={menu.opener}
      items={[
        { id: "open", label: "Open", icon: "external" },
        { id: "rename", label: "Rename", icon: "edit" },
        { id: "delete", label: "Delete", icon: "x", danger: true },
      ]}
      onPick={onPick}
      onDismiss={() => (menu = null)}
    />
  {/if}
</div>

<style>
  .notes {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1 1 auto;
    font: 12px system-ui, sans-serif;
    color: var(--text);
  }
  .head {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 2px;
    padding: 6px 6px 6px 10px;
  }
  .scope {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: 1 1 auto;
    min-width: 0;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-strong);
    font: inherit;
    font-weight: 600;
    text-align: left;
    padding: 4px 6px;
    cursor: pointer;
  }
  .scope:hover {
    background: var(--surface-bg);
  }
  .scope span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .icon-btn:hover:not(:disabled) {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .icon-btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }
  .search {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 6px;
    margin: 0 8px 6px;
    padding: 5px 8px;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-muted);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    flex: 1 1 auto;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text);
    font: inherit;
    padding: 0;
  }
  .list {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0 4px 8px;
    scrollbar-width: thin;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    box-sizing: border-box;
    background: transparent;
    border: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    padding: 6px 8px;
    border-radius: 6px;
    cursor: pointer;
  }
  .row:hover {
    background: var(--surface-bg);
  }
  .row-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-strong);
  }
  .row-sub {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--text-subtle);
    font-size: 11px;
  }
  .snippet {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    flex: 0 0 auto;
    background: var(--surface-active);
    color: var(--text-muted);
    border-radius: 4px;
    padding: 0 5px;
    font-size: 10px;
    line-height: 1.6;
  }
  .date {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rename {
    padding: 2px 4px;
  }
  .rename input {
    width: 100%;
    box-sizing: border-box;
    background: var(--surface-bg);
    color: var(--text-strong);
    border: 1px solid var(--accent);
    border-radius: 4px;
    font: inherit;
    padding: 5px 8px;
    outline: none;
  }
  .status {
    padding: 8px;
    color: var(--text-muted);
  }
  .status.dim {
    opacity: 0.7;
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
  .empty {
    padding: 18px 14px;
    color: var(--text-muted);
    line-height: 1.5;
  }
  .empty p {
    margin: 0 0 12px;
  }
  .choose {
    background: var(--surface-bg);
    color: var(--text-strong);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 12px;
    font: inherit;
    cursor: pointer;
  }
  .choose:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .choose:disabled {
    opacity: 0.6;
    cursor: wait;
  }
</style>

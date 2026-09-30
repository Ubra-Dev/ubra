<script module lang="ts">
  // Per-root UI state survives Source Control / Explorer switches, which
  // remount this component. Listings always re-fetch for freshness.
  const uiCache = new Map<string, { expanded: string[]; showHidden: boolean }>();
</script>

<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import ContextMenu, { announceMenuOpen } from "./ContextMenu.svelte";
  import Icon from "./Icon.svelte";
  import {
    COPY_TOAST_DISMISS_MS,
    copyTextToClipboard,
    truncatePreview,
  } from "./clipboard";
  import { baseName } from "./layout";
  import {
    childRel,
    isHiddenName,
    joinFsPath,
    listDir,
    type DirEntry,
  } from "./files";
  import { store } from "./store.svelte";
  import { toasts } from "./toasts.svelte.ts";

  interface Props {
    root: string;
  }
  let { root }: Props = $props();

  interface DirState {
    status: "loading" | "loaded" | "error";
    entries: DirEntry[];
    error: string | null;
    truncated: boolean;
  }

  interface Row {
    rel: string;
    name: string;
    depth: number;
    entry: DirEntry | null;
    status: "ok" | "loading" | "error" | "empty" | "truncated";
    error: string | null;
  }

  const nodes = new SvelteMap<string, DirState>();
  const expanded = new SvelteSet<string>();
  let showHidden = $state(false);
  let selected = $state<string | null>(null);
  let picking = $state(false);
  let menu = $state<{
    rel: string;
    x: number;
    y: number;
    opener: HTMLElement | null;
  } | null>(null);

  $effect(() => {
    uiCache.set(root, { expanded: [...expanded], showHidden });
  });

  async function loadDir(rel: string): Promise<void> {
    nodes.set(rel, { status: "loading", entries: [], error: null, truncated: false });
    try {
      const listing = await listDir(root, rel);
      nodes.set(rel, {
        status: "loaded",
        entries: listing.entries,
        error: null,
        truncated: listing.truncated,
      });
    } catch (e) {
      nodes.set(rel, {
        status: "error",
        entries: [],
        error: e instanceof Error ? e.message : String(e),
        truncated: false,
      });
    }
  }

  function toggle(rel: string): void {
    if (expanded.has(rel)) expanded.delete(rel);
    else {
      expanded.add(rel);
      if (!nodes.has(rel)) void loadDir(rel);
    }
  }

  function refresh(): void {
    const paths = [...expanded];
    nodes.clear();
    for (const rel of paths) void loadDir(rel);
  }

  onMount(() => {
    const cached = uiCache.get(root);
    for (const rel of cached?.expanded ?? [""]) expanded.add(rel);
    expanded.add("");
    showHidden = cached?.showHidden ?? false;
    for (const rel of expanded) void loadDir(rel);
  });

  const rows = $derived.by(() => {
    const out: Row[] = [];
    const visit = (rel: string, depth: number): void => {
      const node = nodes.get(rel);
      if (!node || node.status === "loading") {
        out.push({ rel, name: "", depth, entry: null, status: "loading", error: null });
        return;
      }
      if (node.status === "error") {
        out.push({ rel, name: "", depth, entry: null, status: "error", error: node.error });
        return;
      }
      const visible = node.entries.filter((e) => showHidden || !isHiddenName(e.name));
      if (visible.length === 0) {
        out.push({ rel, name: "", depth, entry: null, status: "empty", error: null });
        return;
      }
      for (const entry of visible) {
        const child = childRel(rel, entry.name);
        out.push({ rel: child, name: entry.name, depth, entry, status: "ok", error: null });
        if (entry.isDir && expanded.has(child)) visit(child, depth + 1);
      }
      if (node.truncated) {
        out.push({ rel, name: "", depth, entry: null, status: "truncated", error: null });
      }
    };
    visit("", 0);
    return out;
  });

  function activate(rel: string, entry: DirEntry): void {
    selected = rel;
    if (entry.isDir) toggle(rel);
  }

  async function openRow(rel: string, entry: DirEntry): Promise<void> {
    selected = rel;
    if (entry.isDir && !entry.isSymlink) {
      toggle(rel);
      return;
    }
    try {
      await openPath(joinFsPath(root, rel));
    } catch (e) {
      toasts.push("Couldn't open file", e instanceof Error ? e.message : String(e), "");
    }
  }

  function openMenu(e: MouseEvent, rel: string): void {
    e.preventDefault();
    e.stopPropagation();
    selected = rel;
    announceMenuOpen();
    menu = { rel, x: e.clientX, y: e.clientY, opener: e.currentTarget as HTMLElement };
  }

  async function copyText(text: string): Promise<void> {
    const ok = await copyTextToClipboard(text);
    if (ok) {
      toasts.push("Copied to clipboard", truncatePreview(text), "", {
        dismissMs: COPY_TOAST_DISMISS_MS,
        kind: "copy",
      });
    } else {
      toasts.push("Couldn't copy to clipboard", truncatePreview(text), "");
    }
  }

  function onPick(action: string): void {
    const m = menu;
    menu = null;
    if (!m) return;
    // The opener plugin is unscoped: only ever pass workspace-joined paths.
    const full = joinFsPath(root, m.rel);
    if (action === "open") {
      openPath(full).catch((e: unknown) =>
        toasts.push("Couldn't open", e instanceof Error ? e.message : String(e), ""),
      );
    } else if (action === "reveal") {
      revealItemInDir(full).catch((e: unknown) =>
        toasts.push("Couldn't reveal file", e instanceof Error ? e.message : String(e), ""),
      );
    } else if (action === "copy-full") {
      void copyText(full);
    } else if (action === "copy-rel") {
      void copyText(m.rel);
    }
  }

  async function changeRoot(): Promise<void> {
    const ws = store.workspace();
    if (!ws || picking) return;
    picking = true;
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: "Choose a project folder",
      });
      if (typeof path === "string") store.setWorkspaceRoot(ws.id, path);
    } catch (e) {
      console.error("ubra: project folder picker failed", e);
      toasts.push("Couldn't open the folder picker", "Try again.", "");
    } finally {
      picking = false;
    }
  }
</script>

<div class="explorer">
  <div class="head">
    <span class="title" title={root}>{baseName(root) || root}</span>
    <button
      class="icon-btn"
      class:on={showHidden}
      title={showHidden ? "Hide hidden files" : "Show hidden files"}
      aria-label={showHidden ? "Hide hidden files" : "Show hidden files"}
      aria-pressed={showHidden}
      onclick={() => (showHidden = !showHidden)}
    >
      <Icon name="eye" size={13} />
    </button>
    <button class="icon-btn" title="Refresh" aria-label="Refresh" onclick={refresh}>
      <Icon name="refresh" size={13} />
    </button>
    <button
      class="icon-btn"
      title="Change folder"
      aria-label="Change folder"
      disabled={picking}
      onclick={changeRoot}
    >
      <Icon name="edit" size={13} />
    </button>
  </div>
  <div class="tree" role="tree" aria-label="Workspace files">
    {#each rows as row (row.rel + "|" + row.status)}
      {#if row.entry}
        {@const entry = row.entry}
        <button
          role="treeitem"
          class="row"
          class:selected={selected === row.rel}
          aria-selected={selected === row.rel}
          aria-expanded={entry.isDir ? expanded.has(row.rel) : undefined}
          style:padding-left={`${6 + row.depth * 14}px`}
          title={joinFsPath(root, row.rel)}
          onclick={() => activate(row.rel, entry)}
          ondblclick={() => void openRow(row.rel, entry)}
          onkeydown={(e) => {
            // preventDefault suppresses the button's native Enter click.
            if (e.key === "Enter") {
              e.preventDefault();
              if (entry.isDir) toggle(row.rel);
              else void openRow(row.rel, entry);
            }
          }}
          oncontextmenu={(e) => openMenu(e, row.rel)}
        >
          {#if entry.isDir}
            <span class="twisty" class:open={expanded.has(row.rel)}>
              <Icon name="chevron-right" size={12} />
            </span>
          {:else}
            <span class="twisty-sp"></span>
          {/if}
          <Icon name={entry.isDir ? "folder" : "file"} size={13} />
          <span class="name">{row.name}</span>
        </button>
      {:else if row.status === "loading"}
        <div class="status" style:padding-left={`${8 + row.depth * 14}px`}>Loading…</div>
      {:else if row.status === "error"}
        <div class="status error" style:padding-left={`${8 + row.depth * 14}px`}>
          <span>{row.error}</span>
          <button class="retry" onclick={() => void loadDir(row.rel)}>Retry</button>
        </div>
      {:else if row.status === "empty"}
        <div class="status dim" style:padding-left={`${8 + row.depth * 14}px`}>(empty folder)</div>
      {:else}
        <div class="status dim" style:padding-left={`${8 + row.depth * 14}px`}>
          List truncated — too many entries to show
        </div>
      {/if}
    {/each}
  </div>
  {#if menu}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      opener={menu.opener}
      items={[
        { id: "open", label: "Open", icon: "external" },
        { id: "reveal", label: "Reveal in File Manager", icon: "folder" },
        { id: "copy-full", label: "Copy Path" },
        { id: "copy-rel", label: "Copy Relative Path" },
      ]}
      onPick={onPick}
      onDismiss={() => (menu = null)}
    />
  {/if}
</div>

<style>
  .explorer {
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
  .title {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-strong);
    font-weight: 600;
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
  .icon-btn.on {
    color: var(--accent);
  }
  .icon-btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }
  .tree {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding-bottom: 8px;
    scrollbar-width: thin;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    box-sizing: border-box;
    background: transparent;
    border: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    padding-top: 3px;
    padding-bottom: 3px;
    padding-right: 8px;
    border-radius: 4px;
    cursor: pointer;
    white-space: nowrap;
  }
  .row:hover {
    background: var(--surface-bg);
  }
  .row.selected {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .twisty {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--text-subtle);
  }
  .twisty.open {
    transform: rotate(90deg);
  }
  .twisty-sp {
    flex: 0 0 auto;
    width: 12px;
  }
  .name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .status {
    padding-top: 3px;
    padding-bottom: 3px;
    padding-right: 8px;
    color: var(--text-muted);
    white-space: normal;
    overflow-wrap: anywhere;
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
</style>

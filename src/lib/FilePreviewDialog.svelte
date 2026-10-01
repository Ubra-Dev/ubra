<script lang="ts">
  import { openPath } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import {
    COPY_TOAST_DISMISS_MS,
    copyTextToClipboard,
    truncatePreview,
  } from "./clipboard";
  import { baseName } from "./layout";
  import { formatBytes, joinFsPath, readFile } from "./files";
  import { highlightCode } from "./highlight";
  import { overlayFocus } from "./overlayFocus";
  import { toasts } from "./toasts.svelte.ts";

  interface Props {
    root: string;
    rel: string;
    onClose: () => void;
  }
  let { root, rel, onClose }: Props = $props();

  let loading = $state(true);
  let error = $state<string | null>(null);
  let text = $state("");
  let html = $state("");
  let language = $state("plaintext");
  let truncated = $state(false);
  let binary = $state(false);
  let size = $state(0);

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      const file = await readFile(root, rel);
      binary = file.binary;
      truncated = file.truncated;
      size = file.size;
      if (!file.binary) {
        text = file.content;
        const code = highlightCode(file.content, rel);
        html = code.html;
        language = code.language;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
  });

  async function copyContent(): Promise<void> {
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

  function openExternal(): void {
    // The opener plugin is unscoped: only ever pass workspace-joined paths.
    openPath(joinFsPath(root, rel)).catch((e: unknown) =>
      toasts.push("Couldn't open file", e instanceof Error ? e.message : String(e), ""),
    );
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="backdrop"
  data-keyboard-overlay
  onclick={(e) => e.target === e.currentTarget && onClose()}
  onkeydown={(e) => e.key === "Escape" && onClose()}
>
  <div
    class="panel"
    role="dialog"
    aria-modal="true"
    aria-label={`File preview: ${rel}`}
    tabindex="-1"
    use:overlayFocus={{ initial: ".preview-close" }}
  >
    <div class="head">
      <div class="meta">
        <span class="name" title={rel}>{baseName(rel) || rel}</span>
        <span class="sub">
          {#if loading}
            Loading…
          {:else if binary}
            binary · {formatBytes(size)}
          {:else}
            {language} · {formatBytes(size)}
          {/if}
        </span>
      </div>
      {#if !loading && !error && !binary && text !== ""}
        <button class="mini" title="Copy file contents" onclick={copyContent}>
          Copy
        </button>
      {/if}
      <button class="mini" title="Open in the default app" onclick={openExternal}>
        Open
      </button>
      <button
        class="icon-btn preview-close"
        title="Close preview"
        aria-label="Close preview"
        onclick={onClose}
      >
        <Icon name="x" size={13} />
      </button>
    </div>
    <div class="body">
      {#if loading}
        <div class="status dim">Loading…</div>
      {:else if error}
        <div class="status error" role="alert">{error}</div>
        <button class="mini" onclick={() => void load()}>Retry</button>
      {:else if binary}
        <div class="binary">
          <p>This is a binary file ({formatBytes(size)}) and can't be previewed as text.</p>
          <button class="mini" onclick={openExternal}>Open externally</button>
        </div>
      {:else}
        {#if truncated}
          <div class="truncated">
            Showing the first {formatBytes(text.length)} of {formatBytes(size)}.
          </div>
        {/if}
        {#if text === ""}
          <div class="status dim">(empty file)</div>
        {:else}
          <pre class="code"><code class="hljs">{@html html}</code></pre>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 3000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    box-sizing: border-box;
    background: rgba(0, 0, 0, 0.55);
  }
  .panel {
    display: flex;
    flex-direction: column;
    width: min(760px, 100%);
    max-height: 80vh;
    overflow: hidden;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: 12px system-ui, sans-serif;
    color: var(--text);
  }
  .head {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 6px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .meta {
    display: flex;
    flex: 1 1 auto;
    min-width: 0;
    align-items: baseline;
    gap: 8px;
  }
  .name {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-strong);
    font-weight: 600;
  }
  .sub {
    flex: 0 0 auto;
    color: var(--text-muted);
    font-size: 11px;
    white-space: nowrap;
  }
  .mini {
    flex: 0 0 auto;
    background: transparent;
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 5px;
    font: inherit;
    font-size: 11px;
    padding: 3px 8px;
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--text-strong);
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
    background: var(--surface-active);
  }
  .mini:focus-visible,
  .icon-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    padding: 10px 12px;
    background: var(--input-bg);
    scrollbar-width: thin;
  }
  .code {
    margin: 0;
    font: 12px ui-monospace, Menlo, Consolas, monospace;
    line-height: 1.55;
    tab-size: 4;
    white-space: pre;
    color: var(--text);
  }
  .code .hljs {
    background: transparent;
  }
  /* Token colors use app variables so every theme adapts. Injected
     highlight nodes carry no Svelte scope, hence :global. */
  .code :global(.hljs-comment),
  .code :global(.hljs-quote) {
    color: var(--text-subtle);
    font-style: italic;
  }
  .code :global(.hljs-keyword),
  .code :global(.hljs-selector-tag),
  .code :global(.hljs-built_in) {
    color: var(--accent);
  }
  .code :global(.hljs-string),
  .code :global(.hljs-regexp),
  .code :global(.hljs-addition) {
    color: var(--success);
  }
  .code :global(.hljs-number),
  .code :global(.hljs-literal),
  .code :global(.hljs-attr),
  .code :global(.hljs-attribute),
  .code :global(.hljs-selector-class),
  .code :global(.hljs-selector-id),
  .code :global(.hljs-selector-attr) {
    color: var(--attention);
  }
  .code :global(.hljs-title),
  .code :global(.hljs-title.function_),
  .code :global(.hljs-title.class_),
  .code :global(.hljs-section) {
    color: var(--text-strong);
  }
  .code :global(.hljs-operator),
  .code :global(.hljs-punctuation),
  .code :global(.hljs-symbol),
  .code :global(.hljs-bullet),
  .code :global(.hljs-link),
  .code :global(.hljs-code) {
    color: var(--text-muted);
  }
  .code :global(.hljs-meta),
  .code :global(.hljs-doctag) {
    color: var(--text-subtle);
  }
  .code :global(.hljs-deletion) {
    color: var(--error-text);
  }
  .code :global(.hljs-emphasis) {
    font-style: italic;
  }
  .code :global(.hljs-strong) {
    font-weight: 700;
  }
  .status {
    padding: 4px 0;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .status.dim {
    color: var(--text-muted);
  }
  .status.error {
    color: var(--error-text, #f87171);
    margin-bottom: 8px;
  }
  .binary {
    color: var(--text-muted);
    line-height: 1.5;
  }
  .binary p {
    margin: 0 0 12px;
  }
  .truncated {
    position: sticky;
    top: 0;
    margin-bottom: 8px;
    padding: 4px 8px;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-muted);
    font-size: 11px;
  }
</style>

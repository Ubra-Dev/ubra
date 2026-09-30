<script lang="ts">
  import { tick } from "svelte";
  import SetupCommandFields from "./SetupCommandFields.svelte";
  import { overlayFocus } from "./overlayFocus";
  import {
    captureWorkspaceTemplate,
    emptySavedSetups,
    instantiateProfile,
    instantiateTemplate,
    type AgentProfile,
    type WorkspaceTemplate,
  } from "./savedSetups";
  import { savedSetups } from "./savedSetups.svelte";
  import { collectPaneIds, findPane, newId, type LayoutNode, type Workspace } from "./layout";
  import { store } from "./store.svelte";

  type Category = "profiles" | "templates";

  let category = $state<Category>("profiles");
  let selectedId = $state<string | null>(null);
  let editingProfile = $state<{ id: string | null; name: string; cwd: string; cmd: string[] } | null>(null);
  let editingTemplate = $state<{ id: string | null; name: string; workspace: Workspace } | null>(null);
  let confirmDelete = $state<string | null>(null);
  let confirmReset = $state(false);
  let formError = $state<{ field: string; message: string } | null>(null);
  let launchError = $state<string | null>(null);
  let captureError = $state<string | null>(null);
  let captureConsumed = $state(false);
  let profileForPane = $state<Record<string, string>>({});

  const request = $derived(store.savedSetupsRequest);
  const library = $derived(savedSetups.library);
  const profiles = $derived(
    [...(library?.profiles ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
  );
  const templates = $derived(
    [...(library?.templates ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
  );
  const selectedProfile = $derived(profiles.find((p) => p.id === selectedId) ?? null);
  const selectedTemplate = $derived(templates.find((t) => t.id === selectedId) ?? null);
  const unavailable = $derived(library === null || savedSetups.loadError !== null);

  $effect(() => {
    if (request) void savedSetups.load();
  });

  $effect(() => {
    if (request?.mode === "capture" && !captureConsumed && store.layout) {
      captureConsumed = true;
      const source = store.layout.workspaces.find((w) => w.id === request.workspaceId);
      if (!source) {
        captureError = "Workspace is no longer available.";
        category = "templates";
      } else {
        try {
          const captured = captureWorkspaceTemplate(
            JSON.parse(JSON.stringify(source)) as Workspace,
            source.name,
          );
          category = "templates";
          selectedId = null;
          editingProfile = null;
          editingTemplate = { id: null, name: captured.name, workspace: captured.workspace };
        } catch (e) {
          captureError = e instanceof Error ? e.message : String(e);
          category = "templates";
        }
      }
    }
    if (!request) {
      captureConsumed = false;
      captureError = null;
    }
  });

  function close(): void {
    editingProfile = null;
    editingTemplate = null;
    confirmDelete = null;
    confirmReset = false;
    formError = null;
    launchError = null;
    store.closeSavedSetups();
  }

  function onDialogKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  }

  function fail(field: string, message: string): void {
    formError = { field, message };
    void tick().then(() => document.getElementById(field)?.focus());
  }

  function startNewProfile(): void {
    editingTemplate = null;
    confirmDelete = null;
    formError = null;
    editingProfile = { id: null, name: "", cwd: "", cmd: [""] };
    void tick().then(() => document.getElementById("profile-name")?.focus());
  }

  function editProfile(entry: AgentProfile): void {
    editingTemplate = null;
    confirmDelete = null;
    formError = null;
    editingProfile = { id: entry.id, name: entry.name, cwd: entry.cwd, cmd: [...entry.cmd] };
    void tick().then(() => document.getElementById("profile-name")?.focus());
  }

  function startTemplateFromCurrent(): void {
    const ws = store.workspace();
    if (!ws) {
      fail("template-name", "Workspace is no longer available.");
      return;
    }
    try {
      const captured = captureWorkspaceTemplate(
        JSON.parse(JSON.stringify(ws)) as Workspace,
        ws.name,
      );
      editingProfile = null;
      confirmDelete = null;
      formError = null;
      editingTemplate = { id: null, name: captured.name, workspace: captured.workspace };
      void tick().then(() => document.getElementById("template-name")?.focus());
    } catch (e) {
      fail("template-name", e instanceof Error ? e.message : String(e));
    }
  }

  function editTemplate(entry: WorkspaceTemplate): void {
    editingProfile = null;
    confirmDelete = null;
    formError = null;
    editingTemplate = {
      id: entry.id,
      name: entry.name,
      workspace: JSON.parse(JSON.stringify(entry.workspace)) as Workspace,
    };
    void tick().then(() => document.getElementById("template-name")?.focus());
  }

  function nameTaken(name: string, kind: "profile" | "template", excludeId: string | null): boolean {
    const key = name.trim().toLowerCase();
    const list = kind === "profile" ? (library?.profiles ?? []) : (library?.templates ?? []);
    return list.some((e) => e.id !== excludeId && e.name.toLowerCase() === key);
  }

  async function saveProfile(): Promise<void> {
    const draft = editingProfile;
    if (!draft || !library) return;
    formError = null;
    launchError = null;
    const name = draft.name.trim();
    if (!name) {
      fail("profile-name", "Name is required.");
      return;
    }
    if (!draft.cwd.trim()) {
      fail("profile-cwd", "A project folder is required.");
      return;
    }
    const program = (draft.cmd[0] ?? "").trim();
    if (!program) {
      fail("profile-program", "A program is required.");
      return;
    }
    if (nameTaken(name, "profile", draft.id)) {
      fail("profile-name", "A profile with this name already exists.");
      return;
    }
    const entry: AgentProfile = {
      id: draft.id ?? newId("profile"),
      name,
      cwd: draft.cwd,
      cmd: [program, ...draft.cmd.slice(1)],
    };
    const next = {
      ...emptySavedSetups(),
      profiles:
        draft.id === null
          ? [...library.profiles, entry]
          : library.profiles.map((p) => (p.id === draft.id ? entry : p)),
      templates: library.templates,
    };
    const ok = await savedSetups.save(next);
    if (!ok) {
      formError = { field: "profile-name", message: savedSetups.saveError ?? "Couldn't save setup." };
      return;
    }
    editingProfile = null;
    selectedId = entry.id;
  }

  async function saveTemplate(): Promise<void> {
    const draft = editingTemplate;
    if (!draft || !library) return;
    formError = null;
    launchError = null;
    const name = draft.name.trim();
    if (!name) {
      fail("template-name", "Name is required.");
      return;
    }
    if (nameTaken(name, "template", draft.id)) {
      fail("template-name", "A template with this name already exists.");
      return;
    }
    for (const tab of draft.workspace.tabs) {
      const visit = (node: LayoutNode): string | null => {
        if (node.kind === "pane") {
          if (node.cmd !== undefined && node.cmd.length > 0 && !node.cmd[0].trim()) {
            return `tpl-${node.id}-program`;
          }
          return null;
        }
        return visit(node.first) ?? visit(node.second);
      };
      const bad = visit(tab.root);
      if (bad) {
        fail(bad, "A program is required when a custom command is set.");
        return;
      }
    }
    const entry: WorkspaceTemplate = {
      id: draft.id ?? newId("template"),
      name,
      workspace: draft.workspace,
    };
    const next = {
      ...emptySavedSetups(),
      profiles: library.profiles,
      templates:
        draft.id === null
          ? [...library.templates, entry]
          : library.templates.map((t) => (t.id === draft.id ? entry : t)),
    };
    const ok = await savedSetups.save(next);
    if (!ok) {
      formError = { field: "template-name", message: savedSetups.saveError ?? "Couldn't save setup." };
      return;
    }
    editingTemplate = null;
    selectedId = entry.id;
  }

  function launchProfile(entry: AgentProfile): void {
    if (!library || savedSetups.busy) return;
    launchError = null;
    try {
      store.launchSavedWorkspace(instantiateProfile(entry));
      close();
    } catch (e) {
      launchError = e instanceof Error ? e.message : String(e);
    }
  }

  function launchTemplate(entry: WorkspaceTemplate): void {
    if (!library || savedSetups.busy) return;
    launchError = null;
    try {
      store.launchSavedWorkspace(instantiateTemplate(entry));
      close();
    } catch (e) {
      launchError = e instanceof Error ? e.message : String(e);
    }
  }

  async function deleteEntry(id: string): Promise<void> {
    if (!library) return;
    const next = {
      ...emptySavedSetups(),
      profiles: library.profiles.filter((p) => p.id !== id),
      templates: library.templates.filter((t) => t.id !== id),
    };
    const ok = await savedSetups.save(next);
    if (!ok) {
      formError = { field: "", message: savedSetups.saveError ?? "Couldn't save setup." };
      return;
    }
    confirmDelete = null;
    if (selectedId === id) selectedId = null;
  }

  function updateTemplatePane(paneId: string, value: { cwd?: string; cmd?: string[] }): void {
    const draft = editingTemplate;
    if (!draft) return;
    for (const tab of draft.workspace.tabs) {
      const node = findPane(tab.root, paneId);
      if (node) {
        if (value.cwd === undefined) delete node.cwd;
        else node.cwd = value.cwd;
        if (value.cmd === undefined) {
          delete node.cmd;
          delete node.cmdOnRestore;
        } else {
          node.cmd = value.cmd;
          node.cmdOnRestore = false;
        }
        editingTemplate = { ...draft };
        return;
      }
    }
  }

  function updateTemplatePaneTitle(paneId: string, title: string): void {
    const draft = editingTemplate;
    if (!draft) return;
    for (const tab of draft.workspace.tabs) {
      const node = findPane(tab.root, paneId);
      if (node) {
        if (title.trim()) node.title = title;
        else delete node.title;
        editingTemplate = { ...draft };
        return;
      }
    }
  }

  function useProfileForPane(paneId: string, profileId: string): void {
    const profile = library?.profiles.find((p) => p.id === profileId);
    if (!profile) return;
    updateTemplatePane(paneId, { cwd: profile.cwd, cmd: [...profile.cmd] });
  }

  function backToLibrary(): void {
    captureError = null;
    editingTemplate = null;
    store.savedSetupsRequest = { mode: "library" };
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
  class="backdrop"
  onclick={(e) => {
    if (e.target === e.currentTarget) close();
  }}
>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Saved Setups"
    tabindex="-1"
    data-keyboard-overlay
    use:overlayFocus
    onkeydown={onDialogKeydown}
  >
    <div class="header">
      <span>Saved Setups</span>
      <button onclick={close} title="Close Saved Setups" aria-label="Close Saved Setups">✕</button>
    </div>

    {#if !library && savedSetups.loadError}
      <div class="recovery">
        <p><strong>Couldn't load saved setups</strong></p>
        <pre>{savedSetups.loadError}</pre>
        <div class="row">
          <button onclick={() => void savedSetups.load()} disabled={savedSetups.busy}>Retry</button>
          <button onclick={() => void savedSetups.exportOriginal()} disabled={savedSetups.busy}>
            Export original
          </button>
          {#if !confirmReset}
            <button onclick={() => (confirmReset = true)} disabled={savedSetups.busy}>
              Back up and reset
            </button>
          {:else}
            <span>Back up the existing saved setups and replace them with an empty library?</span>
            <button
              onclick={() => {
                confirmReset = false;
                void savedSetups.reset();
              }}
              disabled={savedSetups.busy}
            >
              Confirm reset
            </button>
            <button onclick={() => (confirmReset = false)}>Cancel</button>
          {/if}
        </div>
        {#if savedSetups.backupPath}<p class="backup">Original copied to: {savedSetups.backupPath}</p>{/if}
        {#if savedSetups.saveError}<p role="alert">{savedSetups.saveError}</p>{/if}
      </div>
    {:else}
      <div class="tabs">
        <button
          class:active={category === "profiles"}
          onclick={() => {
            category = "profiles";
            selectedId = null;
            editingProfile = null;
            editingTemplate = null;
            formError = null;
            launchError = null;
          }}
        >
          Agent profiles
        </button>
        <button
          class:active={category === "templates"}
          onclick={() => {
            category = "templates";
            selectedId = null;
            editingProfile = null;
            editingTemplate = null;
            formError = null;
            launchError = null;
          }}
        >
          Workspace templates
        </button>
      </div>

      {#if captureError}
        <p role="alert">{captureError}</p>
        <button onclick={backToLibrary}>Back to library</button>
      {/if}

      <div class="body">
        <div class="list">
          {#if category === "profiles"}
            {#if profiles.length === 0}
              <p class="muted">No saved agent profiles</p>
            {:else}
              {#each profiles as p (p.id)}
                <button
                  class="entry"
                  class:selected={selectedId === p.id}
                  onclick={() => {
                    selectedId = p.id;
                    editingProfile = null;
                    formError = null;
                  }}
                >
                  {p.name}
                </button>
              {/each}
            {/if}
            <button class="primary" onclick={startNewProfile} disabled={unavailable || savedSetups.busy}>
              New profile
            </button>
          {:else}
            {#if templates.length === 0}
              <p class="muted">No saved workspace templates</p>
            {:else}
              {#each templates as t (t.id)}
                <button
                  class="entry"
                  class:selected={selectedId === t.id}
                  onclick={() => {
                    selectedId = t.id;
                    editingTemplate = null;
                    formError = null;
                  }}
                >
                  {t.name}
                </button>
              {/each}
            {/if}
            <button class="primary" onclick={startTemplateFromCurrent} disabled={unavailable || savedSetups.busy}>
              Save current workspace
            </button>
          {/if}
        </div>

        <div class="details">
          {#if savedSetups.busy && !library}
            <p class="muted">Loading…</p>
          {:else if category === "profiles"}
            {#if editingProfile}
              <h2>{editingProfile.id ? "Edit profile" : "New profile"}</h2>
              {#if formError}<p role="alert">{formError.message}</p>{/if}
              <label class="field" for="profile-name">
                <span>Name</span>
                <input
                  id="profile-name"
                  type="text"
                  value={editingProfile.name}
                  oninput={(e) => (editingProfile!.name = (e.target as HTMLInputElement).value)}
                />
              </label>
              <SetupCommandFields
                cwd={editingProfile.cwd || undefined}
                cmd={editingProfile.cmd}
                allowDefaultShell={false}
                labelPrefix="profile"
                onChange={(v) => {
                  editingProfile!.cwd = v.cwd ?? "";
                  editingProfile!.cmd = v.cmd ?? [""];
                }}
              />
              <div class="row">
                <button class="primary" onclick={saveProfile} disabled={savedSetups.busy}>
                  Save profile
                </button>
                <button onclick={() => (editingProfile = null)}>Cancel</button>
              </div>
              {#if savedSetups.saveError}<p role="alert">Couldn't save setup: {savedSetups.saveError}</p>{/if}
            {:else if selectedProfile}
              {@const entry = selectedProfile}
              <h2>{entry.name}</h2>
              <p class="mono">{entry.cwd}</p>
              <p class="mono">Program: {entry.cmd[0]}</p>
              {#each entry.cmd.slice(1) as arg, i (i)}
                <p class="mono">Arg {i + 1}: {arg === "" ? '""' : arg}</p>
              {/each}
              <div class="row">
                <button
                  class="primary"
                  onclick={() => launchProfile(entry)}
                  disabled={unavailable || savedSetups.busy}
                >
                  Launch profile
                </button>
                <button onclick={() => editProfile(entry)} disabled={savedSetups.busy}>Edit</button>
                {#if confirmDelete === entry.id}
                  <span>Delete “{entry.name}”?</span>
                  <button onclick={() => void deleteEntry(entry.id)} disabled={savedSetups.busy}>
                    Delete
                  </button>
                  <button onclick={() => (confirmDelete = null)}>Cancel</button>
                {:else}
                  <button onclick={() => (confirmDelete = entry.id)} disabled={savedSetups.busy}>
                    Delete
                  </button>
                {/if}
              </div>
              {#if launchError}<p role="alert">{launchError}</p>{/if}
            {:else}
              <p class="muted">Select a profile or create a new one.</p>
            {/if}
          {:else if editingTemplate}
            <h2>{editingTemplate.id ? "Edit template" : "Save template"}</h2>
            {#if formError}<p role="alert">{formError.message}</p>{/if}
            <p class="muted">
              Commands typed in a terminal are not captured. Configure launch commands for those
              panes here.
            </p>
            <p class="muted">Saved commands run only when you launch this setup. Restored panes open shells.</p>
            <label class="field" for="template-name">
              <span>Name</span>
              <input
                id="template-name"
                type="text"
                value={editingTemplate.name}
                oninput={(e) => (editingTemplate!.name = (e.target as HTMLInputElement).value)}
              />
            </label>
            {#each editingTemplate.workspace.tabs as tab (tab.id)}
              <section class="tab-group">
                <h3>{tab.name}</h3>
                {#each collectPaneIds(tab.root) as paneId (paneId)}
                  {@const node = findPane(tab.root, paneId)}
                  {#if node}
                    <div class="pane-group">
                      <label class="field" for={`tpl-${node.id}-title`}>
                        <span>Pane title</span>
                        <input
                          id={`tpl-${node.id}-title`}
                          type="text"
                          value={node.title ?? ""}
                          placeholder="Default shell"
                          oninput={(e) =>
                            updateTemplatePaneTitle(node.id, (e.target as HTMLInputElement).value)}
                        />
                      </label>
                      <SetupCommandFields
                        cwd={node.cwd}
                        cmd={node.cmd}
                        allowDefaultShell={true}
                        labelPrefix={`tpl-${node.id}`}
                        onChange={(v) => updateTemplatePane(node.id, v)}
                      />
                      {#if (library?.profiles.length ?? 0) > 0}
                        <label class="field" for={`tpl-${node.id}-profile`}>
                          <span>Use profile</span>
                          <select
                            id={`tpl-${node.id}-profile`}
                            value={profileForPane[node.id] ?? ""}
                            onchange={(e) => {
                              const pid = (e.target as HTMLSelectElement).value;
                              profileForPane[node.id] = pid;
                              if (pid) useProfileForPane(node.id, pid);
                            }}
                          >
                            <option value="">Choose a profile…</option>
                            {#each library?.profiles ?? [] as p (p.id)}
                              <option value={p.id}>{p.name}</option>
                            {/each}
                          </select>
                        </label>
                      {/if}
                    </div>
                  {/if}
                {/each}
              </section>
            {/each}
            <div class="row">
              <button class="primary" onclick={saveTemplate} disabled={savedSetups.busy}>
                Save template
              </button>
              <button onclick={() => (editingTemplate = null)}>Cancel</button>
            </div>
            {#if savedSetups.saveError}<p role="alert">Couldn't save setup: {savedSetups.saveError}</p>{/if}
          {:else if selectedTemplate}
            {@const entry = selectedTemplate}
            <h2>{entry.name}</h2>
            <p class="muted">
              {entry.workspace.tabs.length} tab{entry.workspace.tabs.length === 1 ? "" : "s"} ·
              {entry.workspace.tabs.reduce((n, t) => n + collectPaneIds(t.root).length, 0)} panes
            </p>
            {#each entry.workspace.tabs as tab (tab.id)}
              <p class="muted">{tab.name}</p>
            {/each}
            <div class="row">
              <button
                class="primary"
                onclick={() => launchTemplate(entry)}
                disabled={unavailable || savedSetups.busy}
              >
                Launch template
              </button>
              <button onclick={() => editTemplate(entry)} disabled={savedSetups.busy}>Edit</button>
              {#if confirmDelete === entry.id}
                <span>Delete “{entry.name}”?</span>
                <button onclick={() => void deleteEntry(entry.id)} disabled={savedSetups.busy}>
                  Delete
                </button>
                <button onclick={() => (confirmDelete = null)}>Cancel</button>
              {:else}
                <button onclick={() => (confirmDelete = entry.id)} disabled={savedSetups.busy}>
                  Delete
                </button>
              {/if}
            </div>
            {#if launchError}<p role="alert">{launchError}</p>{/if}
          {:else}
            <p class="muted">Select a template or save the current workspace.</p>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 60;
  }
  .dialog {
    width: 760px;
    max-width: calc(100vw - 48px);
    height: 600px;
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--app-bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    color: var(--text);
    font: 12px var(--font-ui);
    overflow: hidden;
  }
  .dialog :focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 8px 10px 16px;
    border-bottom: 1px solid var(--border);
    color: var(--text-strong);
    font-size: 14px;
    font-weight: 600;
  }
  .header button {
    display: inline-flex;
    align-items: center;
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 4px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .header button:hover {
    color: var(--text-strong);
    background: var(--surface-bg);
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 8px 16px 0;
  }
  .tabs button {
    font: inherit;
    color: var(--text-muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    padding: 6px 12px;
    cursor: pointer;
  }
  .tabs button.active {
    color: var(--text-strong);
    background: var(--surface-bg);
    border-color: var(--border);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 12px;
    padding: 12px 16px 16px;
  }
  .list {
    flex: 0 0 200px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    padding-right: 12px;
  }
  .details {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .entry {
    text-align: left;
    font: inherit;
    color: var(--text);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    padding: 6px 10px;
    cursor: pointer;
  }
  .entry:hover {
    background: var(--surface-bg);
  }
  .entry.selected {
    background: var(--surface-active);
    color: var(--text-strong);
    border-color: var(--border);
  }
  button.primary {
    font: inherit;
    color: var(--text-strong);
    background: var(--surface-active);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 12px;
    cursor: pointer;
  }
  button.primary:disabled,
  button:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .row button {
    font: inherit;
    color: var(--text);
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 12px;
    cursor: pointer;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field input,
  .field select {
    font: inherit;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 6px;
    padding: 5px 8px;
  }
  .mono {
    font-family: ui-monospace, Menlo, Consolas, monospace;
  }
  .muted {
    color: var(--text-muted);
    margin: 0;
  }
  h2 {
    font-size: 14px;
    color: var(--text-strong);
    margin: 0;
  }
  h3 {
    font-size: 12px;
    color: var(--text-strong);
    margin: 8px 0 4px;
  }
  .tab-group {
    border-top: 1px solid var(--border);
    padding-top: 4px;
  }
  .pane-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
    margin: 6px 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--sidebar-bg);
  }
  .recovery {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
  }
  pre {
    white-space: pre-wrap;
    font-size: 11px;
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
  }
  .backup {
    color: var(--text-muted);
  }
  [role="alert"] {
    color: var(--text-strong);
  }
  @media (max-width: 640px) {
    .body {
      flex-direction: column;
    }
    .list {
      flex: none;
      border-right: none;
      border-bottom: 1px solid var(--border);
      padding-right: 0;
      padding-bottom: 8px;
      max-height: 180px;
    }
  }
</style>

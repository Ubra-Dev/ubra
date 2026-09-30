<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";

  let {
    cwd,
    cmd,
    allowDefaultShell,
    labelPrefix,
    onChange,
  }: {
    cwd?: string;
    cmd?: string[];
    allowDefaultShell: boolean;
    labelPrefix: string;
    onChange: (value: { cwd?: string; cmd?: string[] }) => void;
  } = $props();

  let pickerError = $state<string | null>(null);
  let picking = $state(false);

  const useDefaultShell = $derived(allowDefaultShell && cmd === undefined);
  const program = $derived(cmd?.[0] ?? "");
  const args = $derived(cmd?.slice(1) ?? []);

  async function browse(): Promise<void> {
    if (picking) return;
    picking = true;
    pickerError = null;
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: "Choose a project folder",
      });
      if (typeof path === "string") onChange({ cwd: path, cmd });
    } catch (error) {
      console.error("ubra: folder picker failed", error);
      pickerError = "Couldn't open the folder picker. Try again.";
    } finally {
      picking = false;
    }
  }

  function setProgram(value: string): void {
    const rest = cmd?.slice(1) ?? [];
    onChange({ cwd, cmd: [value, ...rest] });
  }

  function setArg(index: number, value: string): void {
    const head = cmd?.[0] ?? "";
    const rest = [...(cmd?.slice(1) ?? [])];
    rest[index] = value;
    onChange({ cwd, cmd: [head, ...rest] });
  }

  function addArg(): void {
    const head = cmd?.[0] ?? "";
    onChange({ cwd, cmd: [head, ...(cmd?.slice(1) ?? []), ""] });
  }

  function removeArg(index: number): void {
    const head = cmd?.[0] ?? "";
    const rest = (cmd?.slice(1) ?? []).filter((_, i) => i !== index);
    onChange({ cwd, cmd: [head, ...rest] });
  }
</script>

<div class="cmd-fields">
  <label class="field" for={`${labelPrefix}-cwd`}>
    <span class="field-label">Project folder</span>
    <span class="folder-row">
      <input
        id={`${labelPrefix}-cwd`}
        class="mono"
        type="text"
        value={cwd ?? ""}
        placeholder={allowDefaultShell ? "Default directory" : "Required"}
        oninput={(e) => onChange({ cwd: (e.target as HTMLInputElement).value || undefined, cmd })}
      />
      <button type="button" class="mini" onclick={browse} disabled={picking}>
        {picking ? "…" : "Browse"}
      </button>
      {#if allowDefaultShell && cwd !== undefined}
        <button type="button" class="mini" onclick={() => onChange({ cwd: undefined, cmd })}>
          Clear
        </button>
      {/if}
    </span>
  </label>
  {#if pickerError}<p class="inline-error" role="alert">{pickerError}</p>{/if}

  {#if allowDefaultShell}
    <label class="check">
      <input
        type="checkbox"
        checked={useDefaultShell}
        onchange={(e) =>
          onChange({
            cwd,
            cmd: (e.target as HTMLInputElement).checked ? undefined : [""],
          })}
      />
      <span>Default shell</span>
    </label>
  {/if}

  {#if !useDefaultShell}
    <label class="field" for={`${labelPrefix}-program`}>
      <span class="field-label">Program{allowDefaultShell ? "" : " (required)"}</span>
      <input
        id={`${labelPrefix}-program`}
        class="mono"
        type="text"
        value={program}
        placeholder="e.g. /opt/homebrew/bin/node"
        oninput={(e) => setProgram((e.target as HTMLInputElement).value)}
      />
    </label>
    <div class="field">
      <span class="field-label" id={`${labelPrefix}-args-label`}>Arguments</span>
      <div class="args" role="group" aria-labelledby={`${labelPrefix}-args-label`}>
        {#each args as arg, i (i)}
          <span class="arg-row">
            <input
              id={`${labelPrefix}-arg-${i}`}
              class="mono"
              type="text"
              value={arg}
              placeholder={arg === "" ? 'empty argument ("")' : `Argument ${i + 1}`}
              oninput={(e) => setArg(i, (e.target as HTMLInputElement).value)}
            />
            <button type="button" class="mini" onclick={() => removeArg(i)} aria-label={`Remove argument ${i + 1}`}>
              Remove
            </button>
          </span>
        {:else}
          <p class="muted">No arguments.</p>
        {/each}
        <button type="button" class="mini" onclick={addArg}>Add argument</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .cmd-fields {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
  }
  .field-label {
    color: var(--text);
    font-size: 12px;
  }
  input.mono {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    border-radius: 6px;
    padding: 5px 8px;
    min-width: 0;
    flex: 1;
  }
  .folder-row,
  .arg-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  button.mini {
    flex: 0 0 auto;
    font-size: 12px;
    color: var(--text);
    background: var(--surface-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
  }
  button.mini:hover {
    background: var(--surface-hover);
  }
  button.mini:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
  }
  .args {
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: flex-start;
  }
  .arg-row {
    width: 100%;
  }
  .muted {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
  }
  .inline-error {
    margin: 0;
    font-size: 12px;
    color: var(--text-strong);
  }
</style>

# Saved Agent Profiles and Workspace Templates Implementation Plan

## Context
The user selected saved profiles and templates after requesting feature recommendations for Ubra. Build named agent launches and reusable whole-workspace terminal layouts within the existing desktop application. The user explicitly chose: saved setup commands run only after an explicit Launch action; restarting Ubra restores panes as shells without rerunning those commands.

## Findings
- `src/lib/layout.ts` already defines `PaneNode.cwd`, `title`, `cmd: string[]`, binary split trees, tab zoom and active workspace/tab references. `sanitizeLayout` enforces layout identities, geometry, byte/entity/depth limits, and returns an independent parsed tree.
- `src/lib/store.svelte.ts::completeOnboarding` records a session-only free-text command, consumed by `takePendingTerminalCommand`; it does not populate persistent `PaneNode.cmd`.
- `src/lib/PaneView.svelte` is the only located caller of `<TerminalPane>`; it currently forwards `node.cmd` as executable/arguments. `TerminalPane.svelte` uses `acquireSession` to single-flight new spawns and retain the same PTY across moves/remounts.
- `src/routes/+page.svelte` mounts every workspace/tab, hiding inactive ones. Explicitly launched multi-tab setups must therefore authorize all intended startup commands, not only the visible tab.
- `src/lib/overlayFocus.ts` traps focus and restores the opener; route-level `data-keyboard-overlay` detection owns keyboard shortcuts. Reuse both for the saved-setups surface.
- `src-tauri/src/layout_store.rs` resolves `UBRA_DATA_DIR`, distinguishes missing data from read/parse/version failures, blocks ordinary saves over failed documents, and backs up exact bytes before explicit reset. Reuse this persistence/recovery convention for saved setups.
- Language-server references were attempted for `PaneNode`, but no server was available. Scoped source search found consumers in `layout.ts` and `PaneView.svelte`, command round-trip assertions in `tests/layout.test.ts`, and no second TerminalPane caller.
- Read-only scouts found no fresh-ID workspace clone helper; identity-preserving move/extract helpers are inappropriate for reusable templates.
- portable-pty can fall back to home for an invalid explicit cwd. Validate supplied project directories before spawning.
- Native layout storage currently validates object/version/size; frontend validation owns the tree. Preserve that division for the independent library.


## Approach
Execute steps in order. Keep this desktop-local: no daemon migration, remote hosts, environment-variable storage, template sharing/import, or shell-history extraction.

### 1. Version the explicit-launch policy and migrate existing layouts
Add `cmdOnRestore?: boolean` to `src/lib/layout.ts::PaneNode`; `false` requires explicit launch authorization, absence retains existing startup-command behavior. Set frontend and native `LAYOUT_VERSION` to `2`. All feature-created command panes use `cmdOnRestore: false`.

`sanitizeLayout` accepts versions `1` and `2`, applies existing validation, and returns version `2`. Preserve legacy cwd/cmd/selection/zoom; version-1 input migrates with the new policy absent. For version-2 nodes, preserve a supplied boolean and reject any other type with `Invalid saved pane command restore policy.` Keep existing cmd validation for legacy layouts; stronger validation is specific to saved definitions.

Native `load_layout_from` accepts versions `1` and `2`; normal layout writes/resets require `2`. Loading legacy data alone must not rewrite the file. Existing AppStore boot/flush already run `sanitizeLayout`, supplying the migration before saving. Unsupported versions still require recovery. Bumping the document version prevents an older app from ignoring the flag and executing manual-only commands.

Affected contracts: `layout.ts::{LAYOUT_VERSION,PaneNode,sanitizeNode,sanitizeLayout}`, `layout_store.rs::{LAYOUT_VERSION,load_layout_from,validate_version,write_layout_to,reset_layout_to}`, AppStore boot/flush, and layout tests. The exact search `LAYOUT_VERSION|sanitizeLayout|load_layout_from|save_layout_to|validate_version` in `src;src-tauri/src;src-tauri/tests;tests` enumerates existing consumers. Use LSP references at execution if available; this session had no server.

Update valid outgoing native test fixtures to the current constant; retain version-1 read/migration cases. Replace the frontend literal-default-version assertion with migration/round-trip behavior, not another pinned incidental default. Existing split/move helpers retain their original live identities and behavior.

### 2. Define the library and create independent workspace instances
Create pure `src/lib/savedSetups.ts`. No existing launch-library or fresh-ID copy utility was found. Exact public contracts:

```ts
export const SAVED_SETUPS_VERSION = 1;
export interface AgentProfile {
  id: string;
  name: string;
  cwd: string;
  cmd: string[];
}
export interface WorkspaceTemplate {
  id: string;
  name: string;
  workspace: Workspace;
}
export interface SavedSetups {
  version: 1;
  profiles: AgentProfile[];
  templates: WorkspaceTemplate[];
}
export function emptySavedSetups(): SavedSetups;
export function sanitizeSavedSetups(raw: unknown): SavedSetups;
export function captureWorkspaceTemplate(workspace: Workspace, name: string): WorkspaceTemplate;
export function instantiateProfile(profile: AgentProfile): Workspace;
export function instantiateTemplate(template: WorkspaceTemplate): Workspace;
export function validateSetupInsertion(layout: Layout, workspace: Workspace): void;
```

Reuse `Workspace`, `LayoutNode`, `newId`, `defaultWorkspace`, `findPane`, `computeLayout`, `collectPaneIds`, and `sanitizeLayout`. Validate a standalone workspace by wrapping it in a current-version single-workspace Layout with matching activeWorkspaceId. Do not duplicate the split-tree validator.
`validateSetupInsertion` validates `{ ...layout, workspaces: [...layout.workspaces, workspace], activeWorkspaceId: workspace.id }` through `sanitizeLayout` and returns void without mutating either input. It is the shared pure aggregate-limit/identity check used by AppStore and Node tests; do not import the reactive store into the Node suite.

Library rules:
- Require an object, numeric version exactly `1`, and both arrays. Only an absent file means an empty library.
- Nonblank saved IDs/names; trim names. Saved-entry IDs are unique across both lists. Names are case-insensitively unique within their category, excluding the entry being edited; a profile and template may share a name. Conflict literals: `A profile with this name already exists.` / `A template with this name already exists.`
- Reuse `MAX_LAYOUT_ENTITIES` as the combined saved-entry count limit and `MAX_LAYOUT_BYTES` as the raw UTF-8 serialized size limit. Each template also satisfies existing layout depth/entity/geometry/reference limits.
- Profile cwd is required/nonblank. Command argv is nonempty with a nonblank program. Trim only program/name; preserve arguments literally, including empty arguments and spaces. Reject NUL in programs, arguments, and directories.
- Template cmd absent means default shell; supplied cmd has the same argv rules. Empty optional template cwd becomes absent. Preserve other paths without variable/tilde expansion. Saving need not find the directory/executable; actual launch reports availability failures.
- Return detached whitelisted objects, like `sanitizeLayout`, rather than retaining input references.

Capture the configured whole workspace: tab order/active tab, zoom, names, pane titles/cwd/cmd, directions and ratios. Give the definition `newId("template")`, detach mutable arrays/objects, and set captured command panes to manual policy. An empty legacy cmd array means no configured command when capturing; other invalid launch definitions must be corrected in the editor. Ordinary shell history and consumed onboarding commands are not recoverable: such panes show `Default shell` until the user configures them.

Instantiate a profile as a new one-pane workspace, tab and pane named after the profile, with copied cwd/cmd and manual policy. Instantiate a template only after validation; recursively regenerate every workspace/tab/split/pane ID using existing `ws`, `tab`, `split`, `pane` prefixes and copy cmd/sizes/tabs arrays. Remap activeTabId and every zoomedPaneId to corresponding new entities; preserve their selection rather than dropping zoom or selecting the first tab. Stored definitions, the source workspace, and repeated instances share neither identities nor mutable objects. Existing extract/move helpers are forbidden here because they move the original PTY-owning node.

Profile edits do not propagate into launched workspaces/templates. Applying a profile to a template pane copies its cwd/cmd, with no live reference.

### 3. Store the library independently using existing recovery mechanics
Extend `src-tauri/src/layout_store.rs` for `<data_dir>/saved-setups.json`, version `1`, maximum `4 * 1024 * 1024` bytes. Reuse `data_dir`/`UBRA_DATA_DIR`; no new dependencies or daemon-state storage.

Extract the existing read/write/backup/reset mechanics into private helpers in this same module, selected by fixed internal descriptors:

```rust
struct DocumentSpec {
    file: &'static str,
    backup_stem: &'static str,
    label: &'static str,
    current_version: u32,
    readable_versions: &'static [u32],
}
```

Layout descriptor: `layout.json`, backup stem `layout`, label `layout`, current version `2`, readable `[1, 2]`. Library descriptor: `saved-setups.json`, backup stem `saved-setups`, label `saved setups`, current/read version `1`. Both use `MAX_LAYOUT_BYTES`.

Share bounded missing-aware JSON reads, version checks, same-directory `<filename>.tmp` replacement, exact-byte backup, pre-save failed-document checks, and backup-before-reset. Keep public layout function signatures unchanged. Native validation owns object/version/size; frontend owns complete saved-definition validation, matching existing tree ownership. Do not introduce a generic frontend path API/storage trait or broaden this into filesystem hardening. Preserve layout recovery errors except accepted schema versions.

Add these public functions with existing anyhow conventions:

```rust
pub fn load_saved_setups_from(dir: &Path) -> anyhow::Result<Option<serde_json::Value>>;
pub fn save_saved_setups_to(dir: &Path, setups: &serde_json::Value) -> anyhow::Result<()>;
pub fn backup_saved_setups_from(dir: &Path) -> anyhow::Result<String>;
pub fn reset_saved_setups_to(dir: &Path, setups: &serde_json::Value) -> anyhow::Result<Option<String>>;
```

Backups are `saved-setups.backup-<32-hex-token>.json`; retain the existing entropy/create-new/copy/sync pattern. Save/reset replacement uses `saved-setups.json.tmp`, like layout storage. Serialize feature-library mutations in the frontend, preventing overlapping writes. Do not claim crash-durable or multi-process transactional storage.

Missing library installs an empty in-memory document without writing on load. Read/parse/version/size or frontend schema failure leaves the library unavailable and blocks ordinary mutation; never overwrite it as empty. Reset requires successful original-byte backup before replacement. Layout and library failures/reset are independent.

Register these commands in `src-tauri/src/lib.rs`; use existing data_dir/error-string conventions and exact IPC names:

```rust
fn load_saved_setups(app: AppHandle) -> Result<Option<serde_json::Value>, String>;
fn save_saved_setups(app: AppHandle, setups: serde_json::Value) -> Result<(), String>;
fn export_saved_setups(app: AppHandle) -> Result<String, String>;
fn reset_saved_setups(app: AppHandle, setups: serde_json::Value) -> Result<Option<String>, String>;
```

IPC payload key is `setups`. Export is a recovery backup inside app data, not general template sharing.

Create `src/lib/savedSetups.svelte.ts` exporting `savedSetups`, with `library: SavedSetups | null`, `busy`, `loadError`, `saveError`, and `backupPath`. Methods: `load(): Promise<void>`, `save(next: SavedSetups): Promise<boolean>`, `exportOriginal(): Promise<void>`, `reset(): Promise<void>`. Load lazily on the modal's first opening, not during AppStore boot; Retry forces a new load. Validate before saving, hold busy through IPC, and replace in-memory library only after success. Failure retains the old entries and edit draft. Block launches/mutations if library is unavailable. Reset sends `emptySavedSetups()`, recording backupPath and installing empty data only after success.

### 4. Authorize commands at actual spawn, not mount/restore
Create pure `src/lib/startupCommands.ts`:

```ts
export class StartupCommands {
  authorize(paneIds: Iterable<string>): void;
  resolve(pane: PaneNode): string[] | undefined;
  retain(paneIds: ReadonlySet<string>): void;
  clear(): void;
}
```

Private Set holds authorized stable pane IDs. `resolve` removes authorization for that ID, then returns cmd for legacy/automatic panes (`cmdOnRestore !== false`) or explicitly authorized panes; otherwise returns undefined/default shell. Nothing is serialized. `retain` drops closed IDs and preserves moved IDs.

AppStore owns this class and adds:

```ts
launchSavedWorkspace(workspace: Workspace): void;
commandForSpawn(paneId: string): string[] | undefined;
authorizePaneCommand(paneId: string): void;
```

`launchSavedWorkspace` first calls `validateSetupInsertion($state.snapshot(this.layout), workspace)`, catching aggregate size/entity limits and identity collisions before mutation. On failure, do not authorize commands, change active selection, or touch live layout. On success authorize every configured-command pane across all tabs, append/activate the independent workspace with its saved active tab, focus its zoomed pane or first active-tab pane, and use existing `saveSoon`. Keep original live tree objects/IDs; validation must not replace the original graph. Each launch creates a new workspace; repeated display names are allowed.

`commandForSpawn` finds the current node using `findTabByPane`/`findPane` and resolves its authorization. A pane closed before pending spawn throws `Pane is no longer in the layout.` instead of starting an unintended shell. Clear authorizations before layout retry/reset/restore. Prune absent IDs from current placed panes in `saveSoon`; moves retain IDs. `authorizePaneCommand` only authorizes a current configured-command pane.

Remove `shell`/`args` props/destructuring from `TerminalPane.svelte` and their forwarding from sole caller `PaneView.svelte`. Inside the existing `acquireSession` spawn callback, call `store.commandForSpawn(sessionKey)` and derive native shell/args from argv. Keep cwd, geometry, snapshot/events and lease ownership flow unchanged. Resolve only in that callback so adopting an existing pending/live lease never executes again. Keep onboarding's existing one-time input path separate.

On PaneView's existing exited-process button, authorize the pane before incrementing runId. Label it `Run saved command` for a manual-policy command pane, otherwise retain `Restart terminal`. This is explicit rerun consent. A failed spawn consumes that attempted authorization, shows existing failed-to-start output, and waits for another deliberate action; no automatic retry or executable-failure shell fallback. A partly successful template remains visible with failed panes, without killing successful siblings or rolling back processes.

At `PtyManager::spawn`, after geometry validation and before ID/PTy allocation, check a supplied cwd using `std::fs::metadata`. Missing/unreadable error context: `Working directory is unavailable: {cwd}`; non-directory error: `Working directory is not a directory: {cwd}`. Do not create directories, expand paths, or substitute home for an explicitly invalid project. Keep absent cwd defaults and valid relative paths. This shared boundary also affects existing GUI/daemon callers and addresses the concrete dependency fallback.

### 5. Implement the saved-setups surface and all create/edit/launch paths
Create `src/lib/SavedSetupsModal.svelte` and reusable `src/lib/SetupCommandFields.svelte`. Follow SettingsModal conventions: system-ui body text, mono commands/paths, existing `--app-bg`, `--sidebar-bg`, `--surface-bg`, `--border`, `--text`, `--text-strong`, `--text-muted`, `--accent`; 10px modal and 6px control radii, 12px body/14px section titles. No app redesign or new UI framework.

Modal: 760×600px capped at viewport minus 48px, 200px independently scrolling entry list beside scrolling details/editor; below 640px viewport width stack list/details. Header `Saved setups`; categories `Agent profiles` / `Workspace templates`; Close button. Reuse overlayFocus, tabindex=-1, role=dialog, aria-modal/title linkage, and data-keyboard-overlay. Escape/backdrop-only click closes; unsaved local drafts are discarded without writing or launching.

AppStore adds:

```ts
savedSetupsRequest:
  | { mode: "library" }
  | { mode: "capture"; workspaceId: string }
  | null;
openSavedSetups(workspaceId?: string): void;
closeSavedSetups(): void;
```

Opening requires loaded valid layout and no recovery/first-run/other app overlay. Add Sidebar footer `Saved setups` and workspace context item `{ id: "save-template", label: "Save as template", icon: "layers" }`, opening capture mode for that ID. Preserve existing Workspace/grid controls.

Mount the modal from `src/routes/+page.svelte`; include savedSetupsRequest in app inert/overlay checks and `AgentStore::isSeen`, preventing hidden-background terminal focus from acknowledging completion while obscured. Use one modal's internal states for edits/delete/reset, not overlapping Settings/ConfirmDialog ownership.

Profile flow:
- Empty state `No saved agent profiles`, action `New profile`; no seeded commands run.
- Form: Name, required Project folder with single-directory picker, Program, one editable Argument row per argv element with Add/Remove. Preserve empty arguments; no shell-string parser. Pane title defaults to profile name.
- `Save profile` persists only. Saved detail shows directory and exact program/arguments; `Launch profile` instantiates and appends a workspace. Disable launching unsaved drafts and during busy/loading/error states. Structural launch error keeps modal open; successful insertion closes/reveals it.
- `Edit` only changes the definition. `Delete` uses inline `Delete “<name>”?` with Delete/Cancel; deleting a definition never closes launched workspaces.

Template flow:
- Empty state `No saved workspace templates`, action `Save current workspace`. Context capture targets the selected ID, not later active selection.
- Snapshot source using `$state.snapshot` on editor opening. Show editable Name and per-pane title, cwd, Program/Arguments grouped by tab. Geometry, tab order/names, active tab and zoom remain captured/read-only; users arrange topology on the main canvas.
- Per-pane command fields offer `Default shell` (removes cmd), custom executable/argv, optional folder with Clear selecting default directory, and `Use profile`. Profile selection copies cwd/cmd into the draft. Capture configured PaneNode.cwd, not guessed live shell cwd.
- Notice: `Commands typed in a terminal are not captured. Configure launch commands for those panes here.` Also show `Saved commands run only when you launch this setup. Restored panes open shells.`
- `Save template` validates/persists a detached draft without spawning. Saved preview shows tab/pane count and commands; `Launch template` instantiates then appends. `Edit` preserves stored topology but allows name/pane-title/cwd/command changes. Delete uses the same inline-confirmation behavior as profiles.

Command-field component props:

```ts
{
  cwd?: string;
  cmd?: string[];
  allowDefaultShell: boolean;
  labelPrefix: string;
  onChange: (value: { cwd?: string; cmd?: string[] }) => void;
}
```

Use unique label/input IDs per pane. Reuse FirstRun's Tauri directory-dialog pattern; cancellation preserves values, picker errors are inline and do not clear the directory. Profile mode requires a program; template mode permits Default shell. A profile save with no folder fails validation. Display individual arguments without joining them into an executable shell string.

New profiles use `newId("profile")`; new template drafts use the capture helper's `newId("template")`. Editing preserves the saved-entry ID. Display entries alphabetically by name within the selected category. New capture defaults the draft name to the source workspace name; name conflicts are shown for the user to rename, never silently overwrite an existing template. Profile/template drafts remain local until Save succeeds. Preview argument rows as literal values, including a visible `""` for an empty argument.

Library recovery state: `Couldn't load saved setups`, detailed error, `Retry`, `Export original`, and inline-confirmed `Back up and reset`. Reset asks `Back up the existing saved setups and replace them with an empty library?`; show backup path after success. Failed backup/reset preserves data and recovery state. Save failures show `Couldn't save setup` and retain the draft. Validation errors use role=alert, label the field, and focus the first invalid input. If a capture source was removed before snapshot, show `Workspace is no longer available.` and return to the library; do not capture another workspace.

## Critical files & anchors
- `src/lib/ptySessions.ts::acquireSession` — existing single-flight PTY ownership; command authorization must be resolved only in its new-spawn callback.
- `src/lib/layout.ts::sanitizeLayout` and private node/tab/workspace sanitizers — reuse structural limits and valid-reference checks; fresh-ID copying is separate.
- `src/lib/store.svelte.ts::saveSoon/flush/retryLayout` — preserve original live objects during insertion, prune/clear only session-local launch authorization, and retain existing recovery guards.
- `src/lib/agent.svelte.ts::isSeen` — the new modal must suppress background unread acknowledgment.
- `src/lib/overlayFocus.ts::overlayFocus` — traps focus but does not supply Escape, inert state, ARIA roles or shortcut ownership; the new surface must provide those.

## Verification
Planning has not run builds, tests, or the app. The checks below are execution requirements, not claimed results. Run automated suites once after integration; do not run build/lint/tests/formatters during delegated edits.

### Behavior-focused regression coverage
Use existing node:test/node:assert/strict and pure `.ts` imports:
- In `tests/layout.test.ts`, prove a valid version-1 layout migrates to version 2 without losing argv, geometry, active references or zoom; version-2 false policy survives a save/load round trip and a malformed policy is rejected. Future version remains rejected.
- Create `tests/savedSetups.test.ts`: instantiate a nested two-tab template twice with the second tab active and a nested zoom target; assert every entity ID is pairwise disjoint across definition/original/two instances, active/zoom references map to corresponding panes, and mutation of one instance's argv/ratios/cwd cannot affect another. Assert destination aggregate-limit rejection leaves existing layout unchanged. Cover command validation, literal spaces/empty arguments, per-category name collisions, bad active/zoom references, duplicate entry IDs and unsupported library version; avoid tests of default wording/metadata copies alone.
- Create `tests/startupCommands.test.ts`: a false-policy restored pane resolves to shell; explicit authorization produces its command exactly once; unrelated panes remain unauthorized; move with the same identity preserves authorization; removal prunes it; legacy policy still produces its command. Combine with `acquireSession` in a deferred-spawn case to prove same-key remount resolves/executes once and a fresh template pane key gets an independent process. Clean every owned session key.
- Extend inline `layout_store.rs::tests` with separate-library round trip, layout/library file independence, missing versus invalid document, corrupt/unsupported/oversized library overwrite refusal, and exact-byte backup/reset. Force replacement failure using a directory at the library's temporary path; old data must survive. Reuse existing scratch directories and cleanup conventions.
- Add a regression in `src-tauri/tests/pty_echo.rs` using existing ChannelSink/Handshake: explicitly missing cwd and a file-as-cwd both fail without creating a session, while a valid sibling still executes a command. Do not pin platform OS-error wording.

From repository root, with Node >=24 and the existing Rust/Tauri toolchain:

```sh
npm run test:unit
cargo test --manifest-path src-tauri/Cargo.toml
```

If using this checkout's vendored Rust toolchain, source `.tooling/env.sh` first; do not install packages/toolchains unless prerequisites are actually missing. All new tests must be deterministic, isolated and compatible with the existing full suites.

### Actual desktop and command-execution smoke
Run on the current native platform with an isolated data directory. Close/quit must mean explicit Quit, not tray hiding. Do not disrupt a user's running instance: the single-instance plugin may redirect a second launch to it. If one is active, use an isolated OS user/session or VM for native smoke rather than stopping the user's work.

macOS/Linux preparation and launch from repository root:

```sh
SMOKE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ubra-setups-smoke.XXXXXX")"
mkdir -p "$SMOKE_ROOT/data" "$SMOKE_ROOT/project"
command -v node
UBRA_DATA_DIR="$SMOKE_ROOT/data" npm run tauri dev
```

Use the absolute Node executable reported above as the profile Program; use `$SMOKE_ROOT/project` via folder picker. Create profile `Smoke agent`, with two argument rows: `-e` and this literal JavaScript argument:

```js
const fs = require("node:fs");
fs.appendFileSync("profile-runs.txt", "run\n");
console.log("PROFILE_EXECUTED", process.cwd());
setInterval(() => console.log("profile-alive"), 1000);
```

1. Save the profile without launching: `profile-runs.txt` must not exist. Close/reopen Saved setups; the definition and argument rows remain. Launch: a new one-pane workspace appears, output contains `PROFILE_EXECUTED` and the selected project path, and the marker has exactly one line. Read the actual file; input echo is not proof.
2. Move/split/switch the launched pane and return to it while its heartbeat runs: the marker stays one line and the original PTY remains alive. This exercises authorization with real remounts rather than only unit tests.
3. Build a second tab and a nested split using existing canvas controls, select that second tab, and zoom a nested pane. Save as template `Smoke workspace`; in its editor configure shell-origin panes using custom Node argv as above, with distinct marker basenames per pane. Saving must not create those markers or change source terminals.
4. Launch the template twice. Each instance preserves tab order/ratios/titles/active second tab/zoom but has independent terminals. Marker counts prove every configured pane—including hidden tabs—executes once per launch. Close only one copied workspace; the original heartbeat and the other copy continue. Inspect saved layout identities if needed using file reads, not source-text assertions.
5. Explicitly Quit and relaunch with the same UBRA_DATA_DIR. Library entries and instantiated layouts return; marker counts do not increase and saved-command panes open shells. Launch a saved setup deliberately: its markers increase exactly once. Do not claim conversation or process resume.
6. Save a profile with a nonexistent executable and another with a project folder that is removed after saving. Launch each: its failed pane shows a visible startup error, no marker appears in home, and an existing sibling keeps running. Saving definitions remains possible even when tools/directories are temporarily absent.
7. Edit and delete a saved definition: edits affect only future launches; deleting it does not terminate an already launched pane. Create a same-category duplicate name and verify a field error rather than overwrite. Cancel a directory picker and verify the old path remains.
For literal-argv smoke, edit a profile to add rows `--`, `argument with spaces`, `$(not-a-shell-command)`, and an empty row after its `-e` script. Make the script write `JSON.stringify(process.argv.slice(1))` to a distinct marker. Launch and read that file: values must be exactly `["argument with spaces","$(not-a-shell-command)",""]`, with no shell substitution or argument splitting. The edit must not alter an already running workspace or previously saved template.
8. Keyboard-only modal check: Tab/Shift+Tab remain inside; Escape returns to the opener; app new-workspace/split shortcuts do not mutate the background; completions behind the modal remain unread. Capture the actual surface at normal size, a narrow viewport and both existing light/dark themes; check scroll containment and visible focus.
9. Quit, write invalid bytes to scratch `saved-setups.json`, and relaunch: normal layout/terminals still load; library modal enters its own recovery. Retry/export/back-up-and-reset preserves exact original bytes and resets only the library. In a separate scratch run corrupt layout.json: its recovery/reset must leave a valid library file untouched. Never modify the user's normal files.
10. Exercise a library-save failure by putting a directory at scratch `saved-setups.json.tmp` while app is running. Save an edit: show failure, retain the draft, and preserve old saved entries/file. Remove that scratch obstacle and retry the same draft successfully.

For Windows native verification, use a PowerShell-created scratch directory, set `$env:UBRA_DATA_DIR` to its data subdirectory before `npm run tauri dev`, and use `(Get-Command node).Source`; the same Node arguments/markers work cross-platform. Existing ConPTY handshake behavior remains covered by native tests.

### Visual access contingency
Verify the actual native window using available OS UI/accessibility tools and capture a screenshot. A plain Vite browser cannot execute native Tauri IPC. If this environment cannot inspect the native window, use the browser tool against a throwaway Tauri-adapter fixture for the modal's rendered/focus states and separately run real native PTY/storage checks; disclose that native-window interaction remains unverified rather than representing browser mocks as end-to-end proof. Do not add a permanent mock backend or change production boot for the fixture.

## Assumptions & contingencies
Preserve the established Svelte/Tauri/xterm architecture and existing in-flight user edits. The shared daemon/remote runtime roadmap is not part of saved setups. Templates capture configured directories and commands, not shell history, running-process handles, terminal output, native agent conversations, or secrets from the environment.

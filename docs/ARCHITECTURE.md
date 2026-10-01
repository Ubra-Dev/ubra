# Architecture notes

Runtime behavior contracts for Ubra's desktop backend. These are narrow
compatibility exceptions and ownership rules, not claims that packaged
behavior has been verified — see [VERIFYING.md](VERIFYING.md) for the
manual gates.

## Desktop security and process ownership

Production CSP permits application assets and Tauri IPC, not remote scripts or
arbitrary remote connections. `style-src 'unsafe-inline'` is required by
Svelte/xterm runtime styling; `asset:`/`http://asset.localhost` and `data:` image
sources support local application assets, while `data:` fonts support embedded
fonts. Scripts remain restricted to `'self'`; objects, document base overrides
and framing are denied.

On Unix, pane close targets the pane's terminal session with bounded TERM/KILL
escalation. Intentionally detached `setsid` processes are outside that session and
may survive; detached jobs must close inherited terminal handles. On Windows,
each pane uses a kill-on-close job for ordinary descendants. Native Windows smoke
must include children started immediately during spawn to exercise job-assignment
timing, and pane isolation must be checked on every supported platform.

Tray initialization failure requests a native warning explaining that window
close quits. A hide failure likewise warns before quitting, rather than
silently leaving an unreachable app. The tray and app menus provide Quit
independently of the window. For a repeatable
tray-unavailable smoke, use a debug build with
`UBRA_DISABLE_TRAY=1` and a fresh `UBRA_DATA_DIR`; release builds ignore this
switch. Native window-close, hide-failure, and warning appearance remain manual
release checks.

The tray menu is a live status surface. The frontend owns pane labels and the
effective rollup, so it pushes a `TraySummary` via `tray_update` whenever
agent state, layout, or tray prefs change (debounced, with
identical payloads skipped on both ends); the backend rebuilds the menu and
sets the menu-bar title and tooltip. Agent rows carry `tray-agent-<node>` ids
and route clicks back through `tray-focus-pane`, which the frontend validates
against the current layout before revealing the pane. `set_title` renders on
macOS and Linux and is a no-op on Windows; the menu content is the
cross-platform surface. Disabling the agent list restores the static
Show/Quit menu while title and tooltip keep updating.

PTY geometry is validated before OS/emulator mutation: 2–1000 columns,
1–1000 rows, at most 250,000 cells. Shutdown closes spawn admission and
terminates ownership sets in one bounded batch, including children that outlive
their root shell after closing terminal handles.

Moved terminals restore an atomic output watermark, primary/alternate buffers,
cursor and supported input modes, including unfinished escape input; only newer
chunks replay. Pending spawns belong to stable pane identities and are adopted
across component remounts. Backend sessions carry their stable pane key, so a
restarted frontend adopts its survivors via `pty_list` instead of spawning
replacements; sessions whose keys left the layout are reaped on load, and
unmounts while the layout is unloaded never kill. Snapshots replay scrollback
history before the visible screen. Panes persist their last agent CLI and
rerun it on fresh spawns; adopted sessions never rerun.

Layout recovery blocks autosave until Retry succeeds or an explicit Reset
preserves the original in an exact-byte backup. Export copies the original
without changing recovery state. Saved documents are limited to 4 MiB,
32 tree levels and 4096 workspace/tab/tree entities, with globally unique
nonempty IDs and nondegenerate finite split ratios. Unsafe documents are
rejected rather than silently repaired and overwritten.

## Agent resume

After an app restart the original processes are gone; panes restore via
agent-native resume (gated by the auto-launch setting; the respawn button
always resumes explicitly):

| CLI    | Strategy (verified against real installs)                |
| ------ | -------------------------------------------------------- |
| claude | `claude --continue` (cwd-scoped, needs no capture)       |
| codex  | newest `~/.codex/sessions` rollout for the pane cwd → `codex resume <id>` |
| else   | bare-command retype (previous behavior)                  |

Session ids are discovered in the status service (60s TTL per pane,
re-resolved on identity change) and ride along on `AgentStatus.sessionRef`
(`{kind: "id", value}`), persisting to `layout.json` (`agentSession`)
only on change. Resume argv are validated before use (plain command,
no control bytes or apostrophes, size caps); edited layouts fall back
to the original command. To add a CLI: verify its flags against a real
install, add capture in `src-tauri/src/agent_session.rs` when an id is
needed, and add a row to `src/lib/agentResume.ts`.

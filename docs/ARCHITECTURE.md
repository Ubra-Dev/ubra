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
silently leaving an unreachable app. The tray and app menus provide Quit and
Stop Agents and Quit independently of the window. For a repeatable
tray-unavailable smoke, use a debug build with
`UBRA_DISABLE_TRAY=1` and a fresh `UBRA_DATA_DIR`; release builds ignore this
switch. Native window-close, hide-failure, and warning appearance remain manual
release checks.

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

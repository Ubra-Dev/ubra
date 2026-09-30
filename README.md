# Ubra

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)](https://github.com/stackwares/ubra-tauri/releases)
[![Tauri](https://img.shields.io/badge/desktop-Tauri%20v2-ffc131.svg)](https://v2.tauri.app/)
[![Svelte](https://img.shields.io/badge/web-Svelte%205-ff3e00.svg)](https://svelte.dev/)

An agent runtime as a cross-platform desktop app (macOS, Linux, Windows).
Workspaces → tabs → terminal panes running real coding-agent CLIs, with agent
state badges, layout persistence, and agents that keep running while the window is hidden.

Built with [Tauri v2](https://v2.tauri.app/) (Rust backend), Svelte + TypeScript,
and [xterm.js](https://xtermjs.org/) for terminal rendering.

## Features

- **Multiplexer model** — workspaces containing tabs containing split terminal panes,
  with draggable dividers, pane zoom, and per-node rename.
- **Layout persistence and recovery** — window size/position and the full workspace
  layout restore across relaunches. Invalid or unsupported saved layouts enter
  recovery instead of being silently overwritten.
- **Tray behavior** — closing the window hides the app only when the tray is
  available; otherwise normal close quits. Settings also provides an explicit Quit.
- **Agent awareness** — panes running agent CLIs (`claude`, `codex`, `opencode`, …)
  surface working/finished badges in pane headers, tabs, and the sidebar, plus a
  notification and sound when an agent finishes while you're elsewhere.
- **Explorer & Source Control** — a collapsible right sidebar shows the
  active workspace's folder tree (open, reveal, copy path) and git status
  with per-file diffs, staging, commits, branch switching, and push/pull,
  all scoped to the workspace folder.
- **Notifications & sounds** — finished-agent alerts delivered as a system
  notification, an in-app toast, or off, with done/needs-attention chimes
  (custom sound file and per-agent muting supported) that play even when the
  window is hidden to the tray.
- **Headless automation (experimental)** — a separate authenticated local daemon
  and JSON CLI expose terminal and agent-state operations without a window.

## Install

Download the latest bundle for your OS from
[Releases](https://github.com/stackwares/ubra-tauri/releases).

Or build from source:

```sh
npm ci
npm run tauri build
```

Bundles land in `src-tauri/target/release/bundle/`.

## Prerequisites

- Rust via [rustup](https://rustup.rs/) + OS deps from the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- Node.js **24+** and npm. CI uses Node 24; direct TypeScript execution in the
  frontend test runner is part of this baseline.

On Debian/Ubuntu, install the Tauri dependencies **and ALSA development headers**
(required by the audio backend):

```sh
sudo apt update
sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev
```

Note: this workspace vendors a local Rust toolchain under `.tooling/` (gitignored)
for sandboxed builds. On your own machine, install Rust normally and ignore that
directory (or `source .tooling/env.sh` from the repo root to reuse it here).

## Develop

```sh
npm ci
npm run tauri dev
```

## Checks

```sh
npm run check                    # svelte-check
npm run test:unit                # frontend unit tests (node:test)
npm run build                    # static frontend
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
npm audit
npm run tauri build              # native bundle on the current platform
```

Run these commands from the repository root. The checked-in `CI` workflow runs
on pushes to `main`/`master`, pull requests, and calls from the tag-release
workflow. Its macOS/Linux/Windows matrix runs frontend checks, tests, build and
npm advisory checks, Rust tests/Clippy/formatting, and native bundle builds.
Checked-in triggers do not establish that remote Actions is enabled or that a
particular platform has passed.

### Dependency advisory resolution

SvelteKit 2.70.3 still declares `cookie ^0.6.0`. The narrowly scoped
`@sveltejs/kit` → `cookie 0.7.2` override fixes
[GHSA-pxg6-pf52-xh8x](https://github.com/advisories/GHSA-pxg6-pf52-xh8x)
without downgrading SvelteKit or its adapter. The upstream
[0.7.0 fix](https://github.com/jshttp/cookie/releases/tag/v0.7.0) retains the
`parse`/`serialize` API and rejects invalid cookie names, paths and domains;
[0.7.2](https://github.com/jshttp/cookie/releases/tag/v0.7.2) also fixes
`hasOwnProperty` parsing. Ubra ships a static SPA with SSR disabled, not an
application cookie-handling server. Keep the override until SvelteKit adopts a
patched dependency, and verify future updates with the complete frontend and
packaged smoke checks. Run `cargo audit` separately when `cargo-audit` is
available; a missing scanner is not a clean Rust advisory result.

### Release gates and native smoke

Pushing `v*` tags invokes the full CI matrix first. Only after every platform
passes does the release workflow build and stage its bundles. GitHub release
publication is then **blocked by default** until all three repository Actions
variables below contain the tagged commit's full SHA:

- `NATIVE_SMOKE_SHA_MACOS`
- `NATIVE_SMOKE_SHA_LINUX`
- `NATIVE_SMOKE_SHA_WINDOWS`

Download the `bundle-macOS-arm64`, `bundle-macOS-intel`, `bundle-Linux`, and
`bundle-Windows` artifacts from that release run and smoke-test each on its
native OS. Both macOS artifacts share the `NATIVE_SMOKE_SHA_MACOS` variable:
smoke each architecture where hardware allows and record which builds were
covered. Record the commit, OS, bundle, checks and failures in the release
review before setting that platform's variable. A failed or unavailable
platform remains unverified: leave its variable unset and do not publish.
After all evidence is recorded, rerun the failed publish job. The `release`
environment can additionally require maintainer approval; the SHA checks fail
closed even if environment reviewers are not configured.

Neither automated bundle creation nor this documentation establishes a successful
native smoke run.

### Release signing and updates (one-time maintainer setup)

macOS releases are signed with a Developer ID identity, notarized, and stapled
automatically by the Tauri bundler when the workflow secrets below exist. The
release refuses to build for macOS without `APPLE_SIGNING_IDENTITY`, so an
unsigned DMG can never ship silently. Set these repository secrets once:

- `APPLE_CERTIFICATE` — base64 of the exported Developer ID Application `.p12`
- `APPLE_CERTIFICATE_PASSWORD` — the `.p12` export password
- `APPLE_SIGNING_IDENTITY` — e.g. `Developer ID Application: Name (TEAMID)`
- `APPLE_ID`, `APPLE_PASSWORD` (app-specific password), `APPLE_TEAM_ID`
- `TAURI_SIGNING_PRIVATE_KEY` (+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) — see below

Generate the updater keypair once with `npm run tauri signer generate -w
~/.tauri/ubra.key`, paste the **public** key into `tauri.conf.json` →
`plugins.updater.pubkey` (replacing the placeholder), and store the private key
as `TAURI_SIGNING_PRIVATE_KEY`. Never commit the private key; losing it breaks
the update chain for installed apps, since each release's bundles are verified
against the embedded public key.

The workflow builds separate arm64 and Intel macOS targets (one universal
build is not possible: the Tauri CLI only lipo-merges the main binary, while
Ubra must bundle the sibling `ubra-daemon` and `ubra-cli` binaries), fails
when the tag does not match `tauri.conf.json`'s version, and publishes
`latest.json` alongside the bundles so installed apps can update from
Settings → Updates.

Native macOS smoke must additionally verify the signature chain on the staged
DMG: `codesign -dv --verbose=4 Ubra.app` shows the Developer ID identity,
`spctl -a -vv Ubra.app` accepts it, `stapler validate` passes on the app and
the DMG, and a quarantine-flagged copy opens without a Gatekeeper block. After
the first signed release, updater smoke is: install release N, publish N+1,
confirm Settings → Updates offers, installs, and relaunches into N+1.

Local builds without the signing keys must pass `--no-sign`
(`npm run tauri build -- --no-sign`); the bundler fails closed when a public
updater key is configured but `TAURI_SIGNING_PRIVATE_KEY` is absent.

### Desktop security and process ownership

Production CSP permits application assets and Tauri IPC, not remote scripts or
arbitrary remote connections. `style-src 'unsafe-inline'` is required by
Svelte/xterm runtime styling; `asset:`/`http://asset.localhost` and `data:` image
sources support local application assets, while `data:` fonts support embedded
fonts. Scripts remain restricted to `'self'`; objects, document base overrides
and framing are denied. These are narrow compatibility exceptions, not a claim
that packaged CSP behavior has been verified.

On Unix, pane close targets the pane's terminal session with bounded TERM/KILL
escalation. Intentionally detached `setsid` processes are outside that session and
may survive; detached jobs must close inherited terminal handles. On Windows,
each pane uses a kill-on-close job for ordinary descendants. Native Windows smoke
must include children started immediately during spawn to exercise job-assignment
timing, and pane isolation must be checked on every supported platform.

Tray initialization failure requests a native warning explaining that window
close quits. A hide failure likewise warns before quitting, rather than
silently leaving an unreachable app. Settings provides Quit independently of
the tray. For a repeatable tray-unavailable smoke, use a debug build with
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
across component remounts.

Layout recovery blocks autosave until Retry succeeds or an explicit Reset
preserves the original in an exact-byte backup. Export copies the original
without changing recovery state. Saved documents are limited to 4 MiB,
32 tree levels and 4096 workspace/tab/tree entities, with globally unique
nonempty IDs and nondegenerate finite split ratios. Unsafe documents are
rejected rather than silently repaired and overwritten.

## Verifying manually

```sh
npm run tauri dev
```

For release approval, use the **packaged app**, not only `tauri dev`. Launch it
with a fresh `UBRA_DATA_DIR` (for example `UBRA_DATA_DIR="$(mktemp -d)" <app-binary>`
on macOS/Linux, or set `$env:UBRA_DATA_DIR` to a new temporary directory before
launching on Windows). Preserve that scratch directory while testing relaunches.
Never point destructive probes at your normal layout or running daemon.

Native platform checklist:

1. Start the installed bundle, run a command that creates a marker file, and
   verify the file exists; input echo alone is not proof of command execution.
2. Exercise split/move/zoom while output continues and while a full-screen
   terminal program runs. Close one pane with a child process and verify its
   owned processes exit while a sibling continues.
3. Quit/relaunch to check persistence. In a separate scratch data directory,
   load corrupt and unsupported layouts: recovery must preserve the original
   until explicit reset, with retry/export errors visible.
4. Verify Settings, custom-to-built-in chimes, notification delivery, terminal
   clipboard, clickable links, and packaged webview console/CSP diagnostics.
5. Use keyboard-only onboarding, dialogs and context menus; focus must remain
   inside overlays and return on dismissal. Check navigation with many tabs,
   workspaces and agent rows.
6. Hide/show/quit with the tray; on Linux also exercise tray-unavailable close.
   Quitting must terminate owned sessions.
7. On Windows, also build the headless executables from the same tagged source
   (`cargo build --release --manifest-path src-tauri/Cargo.toml --bins`). They are
   separate from the GUI bundle. Run the daemon/CLI with a fresh `--state-dir`,
   execute a marker-file command through ConPTY, read output, close the pane and
   shut down. Check invalid commands return a nonzero exit code on every OS.
8. Open the right sidebar's Explorer on a scratch folder: browse, reveal, and
   copy paths. In a scratch git repo, verify Source Control status groups,
   diffs, staging, a commit, and push/pull against a local bare remote.

The development checklist below complements, but does not replace, these gates.

- Split panes from the hover toolbar, drag the dividers, add tabs/workspaces,
  then quit and relaunch: the layout restores.
- Split a pane running a live process (e.g. `sleep 300`): the original pane
  keeps running after the split, and closing one side never kills the other.
  Panes also survive tab/workspace switches untouched.
- Workspaces can be closed from the sidebar; closing the last one resets fresh.
- Close the window with a working tray: the app hides and panes keep running.
  Left-click the tray icon to show it again; Quit is available in Settings and
  the tray menu. Without a working tray, closing the window quits instead.
- Agent badges use explicit CLI screen evidence: Working means a recognized
  busy indicator, Blocked means an approval/question prompt or a suspended
  process, and Idle means a recognized ready prompt before an observed task.
  A task returning to its ready prompt becomes Done. Unknown means an agent
  is present but its activity cannot be established. Silence, typing, focus,
  and terminal redraws do not count as task activity or completion.
- The sidebar groups agents by workspace and working directory. Clicking a
  row reveals and focuses its pane. Focusing it in the foreground acknowledges
  unread completion/attention without changing runtime status. Hovering a row
  reveals an X that uses the existing pane-close confirmation.
- The right sidebar follows the active workspace folder: Explorer browses,
  opens, and reveals files; Source Control stages, commits, pushes, and pulls.
  Collapse state and commit drafts persist across view switches.
- Unexpected stops show Needs review; deliberate pane closes are silent.
  A directly launched agent's successful exit can confirm completion, but a
  parent shell's exit code does not establish the nested agent's success.
- Bundled screen profiles cover recognized Codex, Claude, Gemini, and OpenCode
  UI markers. Changed UI versions, custom keybindings, narrow truncation, and
  other CLIs can report Unknown. Rule provenance and supported markers are
  documented in [the screen fixture guide](src-tauri/fixtures/agent_screens/README.md).
- Per-agent rules live in `<data-dir>/agent-detection/<cli>.toml`. Existing
  `[[blocked]]` sections retain their forty-line matching behavior. Optional
  `[[working]]` and `[[idle]]` sections support bounded footer matching; a file
  replaces that CLI's complete bundled profile.
- In Settings, switch Agent-finished delivery to in-app toast and finish an
  agent in another tab: a toast appears with a chime, and clicking it jumps
  to the pane. Muting that agent's CLI silences the chime but keeps the
  toast; the test-sound button previews the current chime.
- Right-click workspaces (Rename/Close), tabs (Rename/Close), and panes
  (Rename/Zoom/Move to new tab/Move to new workspace/Close). Zoom fills
  the tab; hidden siblings keep running. Moved panes keep their live
  terminals; the source tab keeps a fresh pane when emptied.
- `Mod+Alt+Arrow` moves focus between panes, `Mod+Alt+Shift+Arrow` swaps
  the focused pane with its neighbor, `Alt+Shift+Arrow` grows it, and
  `Mod+Alt+T` / `Mod+Alt+N` move it to a new tab / workspace.
- Resize/move the window, quit, and relaunch: size and position restore.
- Keyboard shortcuts work with terminal focus: `Mod+T` new tab, `Mod+W` close
  pane, `Mod+N` new workspace, `Mod+D` / `Mod+Shift+D` split, `Mod+Enter`
  zoom, `Mod+[` / `Mod+]` switch tabs, `Mod+,` settings (`Mod` is Cmd on
  macOS, Ctrl elsewhere; the full list lives in Settings).
- `Mod+=` / `Mod+-` / `Mod+0` resize the terminal font; the size persists.
- `Mod+F` (or pane menu → Find) opens terminal find with incremental
  highlight; Enter / Shift+Enter steps through matches, Esc closes.
- Terminal scrollback is configurable in Settings → Appearance; URLs
  printed in any pane are clickable and open in the browser.
- Settings → Appearance has a background opacity slider (10–100%) for
  terminal-style translucency; the sidebar, tabs, and dialogs stay solid.
- Settings (sidebar footer, or `Mod+,`) holds launch-at-login, theme, font
  size, the interface font picker (searchable bundled Google Fonts with
  previews), notification delivery, sounds and per-agent muting, the
  shortcut reference, and version info.

## Headless daemon + CLI (experimental)

`ubra-daemon` owns PTYs and the agent watcher without a window, serving a
JSON-lines protocol over loopback TCP. By default Unix runtime files live in
`<tmp>/ubra-<effective-uid>/`; Windows uses the per-user app data directory.
`ubra-cli` drives it — all output is pretty-printed JSON, and the CLI starts the
daemon on demand.

The macOS GUI bundle includes sibling daemon/CLI executables under
`Ubra.app/Contents/MacOS/`; they are not automatically added to `PATH`.
Other platform packaging must be inspected in its native release gate.
For standalone source builds use
`cargo build --release --manifest-path src-tauri/Cargo.toml --bins`; outputs are
under `src-tauri/target/release/` (with `.exe` on Windows).

Desktop and daemon share the same status service. Status queries read cached
snapshots and never advance classification. State updates include a monotonic
revision, agent-instance identity, and detection reason. The legacy
`agent-states` event remains available; `agent-state-update` additionally
contains explicit `task-completed` and `agent-stopped` transitions with unique
event IDs. Snapshots do not replay these notifications. Process discovery runs
once per second; output evidence is coalesced at 100 ms, with 200 ms confirmation
for busy/ready changes and a 750 ms tolerance for partial redraws.

```sh
cd src-tauri
cargo run -q --bin ubra-cli -- agents        # agent states
cargo run -q --bin ubra-cli -- agents --watch
cargo run -q --bin ubra-cli -- spawn          # {"ok":true,"pane":1}
cargo run -q --bin ubra-cli -- write 1 "echo hi"
cargo run -q --bin ubra-cli -- read 1         # pane screen text
cargo run -q --bin ubra-cli -- prompt 1 "summarize the diff"
cargo run -q --bin ubra-cli -- send-keys 1 enter
cargo run -q --bin ubra-cli -- wait-state 1 idle --timeout 120
cargo run -q --bin ubra-cli -- wait-output 1 "done" --timeout 120
cargo run -q --bin ubra-cli -- snapshot
cargo run -q --bin ubra-cli -- shutdown
```

The GUI does not use the daemon (it keeps its in-process backend). Protocol 2
uses mutual nonce-bound HMAC-SHA256 authentication before commands or pushed pane
data; the credential never travels over TCP. Runtime directories/files are
current-user-only (Unix `0700`/`0600`, Windows protected owner-only DACLs).
Authentication does not isolate clients from processes able to read the same
user's credential. `daemon.lock` is a persistent OS-locked file: do not delete it
during operation or after a crash. Stale endpoint recovery occurs under that lock.

The daemon allows 32 connections including unauthenticated clients, 64 panes,
256 KiB request frames and, per peer, 2 MiB outstanding output / 256 queued
messages. Overflowing or stalled peers disconnect instead of continuing with
dropped terminal bytes.

CLI daemon errors (`ok:false`), malformed replies and transport failures exit
nonzero; daemon JSON errors remain visible. Shutdown succeeds only after an
authenticated matching-ID acknowledgement with `ok:true` and `shutdown:true`.
Shutting down a missing or silent daemon fails without starting one or deleting
runtime files. Requests have a 10-second deadline, waits clamp to 1–600 seconds
with a five-second CLI margin, authentication is bounded to five seconds and
startup to ten. Agent watch is intentionally streaming but has a 60-second idle
deadline.

Use `--state-dir <fresh-directory>` for isolated experiments and shut that daemon
down afterward. The headless interface remains experimental; native Windows ACL,
cross-user authentication and ConPTY execution must pass platform release gates
rather than being inferred from Unix tests.

## Roadmap

- [x] Phase 0: scaffold + verified dev/build loop
- [x] Phase 1: PTY vertical slice (`portable-pty` + xterm.js pane)
- [x] Phase 2: multiplexer model, layout persistence, tray behavior
- [x] Phase 3: agent awareness v1 (process detection + badges)
- [x] Explicit working/blocked/idle/done/unknown status and completion transitions
- [x] Experimental local daemon + CLI (separate from the GUI backend)
- [ ] GUI daemon migration and SSH remotes
- [ ] Native smoke approval for each release on macOS, Linux, and Windows

## Contributing

Small, focused PRs merge fastest — see [CONTRIBUTING.md](CONTRIBUTING.md).
By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Don't open public issues for vulnerabilities — see [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © 2026 Oliver Martinez

# Whole-app code audit: Ubra

## Scope and verdict

Audited the current working tree, including pre-existing modified and untracked application files, against base commit `e3983db`. This is a Svelte 5 / TypeScript / Tauri 2 / Rust app, not a Flutter application. Reviewed frontend modules and components, Rust desktop/daemon/CLI code, configuration, workflows, fixtures, and test coverage. Generated code, downloaded dependencies, and binary assets were not audited exhaustively; relevant dependency implementations were inspected to verify specific findings.

**Verdict:** good foundations and passing automated checks, but not ready for an unrestricted production release. The highest priorities are daemon access control, PTY input validation, process-tree shutdown, terminal reattachment, and preservation of unreadable saved layouts.

**Findings:** 5 high, 11 medium, 2 low. Severity reflects impact, not simply likelihood. Runtime reproductions and static-code findings are distinguished below. No implementation fixes were applied. Remediation is planned in `tasks/plan.md` and `tasks/todo.md`; approval is required before implementation.

## Verification performed

Environment: macOS / arm64, Node `v26.10.0`, Cargo `1.98.1`, Rust `1.98.1`.

| Check | Result |
|---|---|
| `npm run check` | PASS: 0 errors, 0 warnings |
| `npm run test:unit` | PASS: 97 tests |
| `npm run build` | PASS: static frontend output |
| `cargo test --manifest-path src-tauri/Cargo.toml` | PASS: 80 tests: 64 unit + 16 integration |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | FAIL: module ordering in `src-tauri/src/lib.rs:1` |
| `npm audit --json` | FAIL: 3 low-severity dependency entries arising from one cookie advisory |
| Isolated daemon/CLI probes | Confirmed unauthenticated access, zero-row panic, surviving child, and erroneous CLI success exit codes |
| Pure TypeScript probes | Confirmed duplicate IDs accepted, extreme split sizes collapse, shifted shortcut events fail |

Tests used scratch state directories, not the user's real daemon or saved layout. Spawned probe processes were cleaned up. Command logs are session-local at `/tmp/ubra-audit-{check,unit,build,cargo-test,clippy,fmt,repro,layout-repro}.log` and `/tmp/ubra-audit-npm.json`; the essential results are preserved here.

**Not verified:** interactive packaged desktop behavior, signed/notarized installation, Linux/Windows execution, release bundles, measured high-throughput performance, a complete Rust advisory scan (`cargo-audit` unavailable), or actual third-party CLI version compatibility. Existing screen fixtures explicitly represent source-derived examples rather than captured live sessions. Passing unit tests do not establish frontend lifecycle correctness.

## High-severity findings

### A01 — Daemon grants shell access to any local client

**Location:** `src-tauri/src/daemon.rs:16–19,87–113,307–320,470–542`; `src-tauri/src/bin/ubra-daemon.rs:10–25`; `src-tauri/src/cli.rs:325–357`.

**Evidence:** an unauthenticated TCP connection successfully returned `{"ok":true,"protocol":1,"version":"0.1.0"}` for `ping`. The same dispatch path accepts arbitrary programs, arguments, input, reads, kills, and shutdown without checking a credential. Every connection receives pane output. Binding to `127.0.0.1` is not user authentication: other OS users/processes on the host can reach it. This limitation is already documented, but remains a release blocker for enabling the daemon broadly. The GUI currently uses its own backend and is not exposed through this daemon.

**Fix:** authenticate before sending greetings/output or accepting commands; use a securely created per-user credential or principal-checked local IPC. Restrict state/log/credential permissions, refuse symlink substitution, verify daemon identity/protocol, and lock startup to prevent competing daemons overwriting one port file. Do not equate a successful TCP connection with the expected daemon.

### A02 — Invalid terminal dimensions panic the backend

**Location:** `src-tauri/src/pty_manager.rs:105–143,198–219`; both desktop commands and daemon requests reach these methods without range checks.

**Reproduced:** after spawning an ordinary pane, `{"op":"pty_resize","pane":ID,"cols":80,"rows":0}` caused a connection-thread panic in `vt100-0.16.2/src/grid.rs:74`: `attempt to subtract with overflow`. The request timed out rather than returning a validation error. The panic occurs while screen/size mutexes are held; subsequent operations can encounter poisoned state. Release uses `panic = "abort"`, making a reachable panic especially serious. The frontend clamps spawn dimensions but does not protect backend entry points or all resize cases.

**Fix:** validate positive minimum dimensions and sensible maximum dimensions/cell counts inside `PtyManager` before opening/resizing a PTY or mutating the parser. Return a structured error and preserve the existing session. Test zero, one-column wide-character handling, extreme dimensions, and successful operations after rejection.

### A03 — Pane close does not guarantee descendant termination

**Location:** `src-tauri/src/pty_manager.rs:263–292,303–369`.

**Reproduced:** spawned a parent that created a child ignoring SIGHUP, then called `pty_kill`. It returned `{"ok":true}` while the child's PID remained alive. The manager removes the session and kills only the directly spawned process. `portable-pty`'s Unix child killer signals the root PID and eventually force-kills that PID, not the entire descendant tree. A surviving descendant holding the slave PTY can also keep the reader/session resources alive. Windows root termination likewise needs a tree ownership policy.

**Fix:** track and terminate owned process trees with an explicit policy (Unix foreground/session groups plus tracked descendants where appropriate; Windows job objects), bounded graceful shutdown, escalation, and reaping. Preserve isolation from other panes and document treatment of intentionally detached processes. Do not report a successful close while ordinary owned jobs continue untracked.

### A04 — Terminal reattachment has no lossless snapshot/event boundary

**Location:** `src/lib/TerminalPane.svelte:280–295`; `src-tauri/src/pty_manager.rs:225–229,320–323`.

**Static finding:** a moved pane writes the snapshot and then replays every pending output chunk, including chunks already incorporated into that snapshot. The comment acknowledges duplication, but terminal output contains cursor operations and append-only text: replay is not idempotent and can corrupt the visible screen. Snapshot responses carry no sequence watermark. Reattachment also clears `pendingExits` without handling an exit that arrived while the snapshot invocation was in flight, potentially leaving a dead terminal without the respawn control.

**Fix:** establish a per-pane output sequence and atomic snapshot watermark, replay only chunks after that boundary, and reconcile buffered exits. Preserve required terminal modes as well as cells/cursor. Verify alternate-screen, continuously printing, and exit-during-attach cases. This is a code-proven race, not an interactive reproduction in this audit.

### A06 — Fresh fallback can overwrite an unreadable or unsupported saved layout

**Location:** `src/lib/store.svelte.ts:160–172,287–307`; `src/lib/layout.ts:429–433`; `src-tauri/src/layout_store.rs:25–42`.

**Static finding:** a parse/read failure starts a default layout and displays `loadError`, but subsequent tab/workspace/pane actions still save to the original path. A valid document with an unsupported version silently becomes the default layout with no error. A single ordinary mutation can therefore replace the only copy of user configuration after a transient failure, corruption, or downgrade.

**Fix:** distinguish first run from recoverable load failure/unsupported schema; preserve or quarantine the original, inhibit destructive autosave until an explicit recovery choice, and make persistence errors user-visible. Provide a retry/export/reset flow. Atomic rename alone does not prevent this loss.

## Medium-severity findings

### A05 — Moving a pane while spawn is unresolved kills its new session

**Location:** `src/lib/TerminalPane.svelte:280–320,341–355`; `src/lib/ptySessions.ts:10–17`.

**Static finding:** registry ownership is published only after `pty_spawn` resolves. If a pane moves while that invocation is pending, the remounted component sees no registered ID and can spawn a second session. The disposed old component unconditionally kills the first resolved session at line 311 even though its node still belongs to the layout. The later cleanup path's `stillPlaced` guard does not protect this branch.

**Fix:** own in-flight spawn promises/leases by stable node ID outside the component. A remount should adopt the same pending/live session; only a true close should cancel it. Test deferred spawn, rapid successive moves, close-before-resolution, and failure recovery.

### A07 — Saved layout validation does not enforce identity or numeric invariants

**Location:** `src/lib/layout.ts:47–52,364–444`; keyed blocks in `src/routes/+page.svelte` and `src/lib/TabCanvas.svelte`.

**Reproduced:** two panes with the same ID survived sanitization. A split with `[1e308,1e308]` normalized to `[0,0]`, leaving the first pane at zero width. Empty pane/split IDs, excessive tree depth/counts, and globally repeated pane IDs are also unchecked. Duplicate IDs can break Svelte keyed rendering or associate multiple nodes with one PTY registry entry. Generated IDs truncate UUIDs to 32 bits, unnecessarily increasing lifetime collision probability.

**Fix:** validate/repair globally unique nonempty IDs, retain full UUID entropy, use finite stable ratio normalization with bounds, cap document size/depth/node counts, and preserve/report the original when it cannot be safely repaired. Test duplicates across workspaces as well as within one tab.

### A08 — CLI returns exit code 0 for daemon failures

**Location:** `src-tauri/src/cli.rs:361–395,400–529`; `src-tauri/src/bin/ubra-cli.rs:22–25`.

**Reproduced:** `read 424242` returned JSON `{"ok":false,"error":"no such pane: 424242"}` with process exit code **0**. A failed spawn of a nonexistent executable also exited **0**. `run` prints error replies and returns `Ok(())`, so shell automation cannot detect failures. Shutdown prints success even when its first line is merely a greeting or a negative/malformed response.

**Fix:** validate response shape and `ok`, preserve useful JSON output but return nonzero on failure, skip events until the matching shutdown response, and distinguish confirmed shutdown from an unexpected disconnect. Add bounded read/write deadlines; the 1000-line loop is not a timeout when a stale port leads to a silent service.

### A09 — Daemon has unbounded client buffers and allocation surfaces

**Location:** `src-tauri/src/daemon.rs:197–231,470–542`.

**Static finding:** one unbounded channel per connection, a blocking writer without a deadline, two threads per connection, unlimited connections, and `BufRead::lines()` with no line-size limit. A client that stops reading can accumulate copies of all PTY output indefinitely. Unlimited spawns and oversized dimensions add further resource exhaustion paths.

**Fix:** bound connections, frames, sessions, and output queues; disconnect stalled readers rather than silently dropping terminal bytes; enforce write deadlines and remove failed peers promptly. Keep slow clients from blocking the PTY/status producer. Authentication alone does not solve accidental slow-client exhaustion.

### A10 — Headless Windows panes lack a terminal-query responder

**Location:** `src-tauri/src/daemon.rs:197–218`; `src-tauri/src/cli.rs:400–529`; `src-tauri/tests/pty_echo.rs:26–55`; `src-tauri/tests/daemon_e2e.rs:160–312`.

**Static/platform risk:** the PTY tests explicitly document and answer ConPTY's startup `ESC[6n` cursor query. The daemon only broadcasts output, and the ordinary CLI does not act as a terminal emulator. Headless Windows startup can stall waiting for a response; the daemon integration tests also omit the responder. Not reproduced on Windows here.

**Fix:** provide a bounded, chunk-safe terminal-query response strategy for the headless owner, avoiding duplicate responses when a real terminal attaches. Add Windows-specific live protocol tests that prove actual command execution, not simply echoed input.

### A11 — Tray creation failure still enables hide-on-close

**Location:** `src-tauri/src/lib.rs:232–234,250–265`.

**Static finding:** failure to create the tray is logged and ignored; CloseRequested still prevents close and hides the window. On systems with unavailable/broken tray support, the primary recovery/quit surface can disappear while agents keep running. Linux tray availability varies independently of successful compilation.

**Fix:** track tray availability and use a visible fallback close/quit policy. Handle hide failures and provide explicit Quit independently of the tray. Confirm success/failure paths manually on supported desktop environments.

### A12 — A stale custom file overrides a selected built-in chime

**Location:** `src/lib/agent.svelte.ts:217–218`; `src/lib/SettingsModal.svelte:145–149,198–203`; `src-tauri/src/sound.rs:98–111`.

**Static finding:** every playback call passes `store.soundFile` when nonblank, even if `soundStyle` is Bright/Soft/Pop/Default. Rust gives the file precedence, so selecting a built-in after configuring a custom file still plays the custom audio. Hiding the file field does not clear the value.

**Fix:** derive the playback payload centrally; pass a file only for the `custom` selection while retaining the saved path for later reuse. Test both preview and real notification routing.

### A13 — Shifted punctuation shortcuts do not match real key events

**Location:** `src/lib/shortcuts.ts:53–60,110–126`; `tests/shortcuts.test.ts:81–87`.

**Reproduced:** `matchShortcut({key:'+',mod:true,shift:true,alt:false})` returns null. The test uses `+` without Shift, masking typical keyboard behavior. Shift+[ / Shift+] often produce `{` / `}`, so advertised workspace cycling also fails with those event values. Global dispatch treats Ctrl and Cmd as interchangeable even on macOS, potentially intercepting terminal Ctrl combinations.

**Fix:** normalize physical/semantic punctuation intentionally and respect platform-specific Mod. Test actual key/code/modifier combinations, including international keyboard layouts, without broadening unrelated shortcuts.

### A14 — Modal and context-menu keyboard ownership is incomplete

**Location:** `src/routes/+page.svelte:20–48`; `src/lib/FirstRun.svelte:14,50–158`; `src/lib/ContextMenu.svelte:33–65`; `src/lib/ConfirmDialog.svelte:63–69`; `src/lib/PaneView.svelte:313–320`.

**Static findings:**
- Settings does not suppress global app shortcuts when focus is on a button or dialog container; e.g. new-tab can modify the background app behind the modal.
- Reopened onboarding is a full-screen overlay without inert background/focus containment; Tab can reach hidden background controls.
- Context menus do not focus their first item or implement directional menu navigation; typing can stay in the terminal while a menu is open.
- Canceling a close dialog does not restore terminal focus. Pane actions stay transparent when focused by keyboard because visibility is hover-only.

**Fix:** shared overlay keyboard/focus ownership, inert backgrounds where appropriate, menu navigation, focus restoration, and `:focus-within` visibility. Prefer behavioral accessibility tests to merely satisfying the compiler. These are code-level observations; no screen-reader session was performed.

### A15 — Navigation has no overflow strategy for many workspaces/tabs

**Location:** `src/lib/Sidebar.svelte:267–277,494–500`; `src/lib/TabBar.svelte:123–143`; `src/routes/+page.svelte:173–178`.

**Static finding:** the sidebar stacks unbounded workspace and agent rows above the Settings footer without a scrolling region. The tab bar has neither wrapping nor horizontal scrolling and its tabs are shrinkable. Root overflow is hidden. Large layouts can push Settings out of view or collapse/offscreen tab names and controls.

**Fix:** separate scrollable navigation from fixed footer controls; keep tabs at usable widths in an overflow container and scroll the active tab into view. Verify long names, small windows, and many live agents without remounting terminals.

### A16 — Linux workflows omit the ALSA development dependency

**Location:** `.github/workflows/ci.yml:26–35`; `.github/workflows/release.yml:29–38`; `src-tauri/Cargo.toml:36`; locked `rodio → cpal → alsa → alsa-sys` chain.

**Static/platform risk:** the workflows install Tauri/WebKit prerequisites but not `libasound2-dev`. Rodio's default playback feature includes CPAL's Linux ALSA dependency, which requires the development/pkg-config files. Clean Linux builds can fail regardless of frontend health; success depends on packages preinstalled on the runner. Not executed on Linux in this audit.

**Fix:** explicitly install the audio build prerequisite in both workflows and document it. Run tests/build on a clean Linux image rather than assuming hosted runner state.

## Low-severity findings

### A17 — Dependency advisory needs a targeted resolution

**Location:** `package-lock.json`; `package.json:devDependencies`.

`npm audit` reported **GHSA-pxg6-pf52-xh8x**: `cookie <0.7.0` accepts out-of-bounds cookie name/path/domain characters. Three low entries (`cookie`, `@sveltejs/kit`, `@sveltejs/adapter-static`) reflect the same underlying advisory, not three independent vulnerabilities. This app ships a static SPA with SSR disabled and has no cookie-handling application routes; exposure in the shipped app is consequently limited. The audit's proposed historical major downgrades are not a safe fix.

**Fix:** inspect compatible upstream resolutions or a justified narrowly scoped override, regenerate the lockfile, rerun all frontend checks, and record applicability if no supported fix exists. Do not run `npm audit fix --force` blindly. Rust dependencies still need an advisory scan.

### A18 — Repository verification/documentation are inconsistent

**Location:** `src-tauri/src/lib.rs:1–2`; `README.md:49,72–73,179`; `src/app.html:7`; `package.json:scripts`.

Formatting fails only on module ordering. README says CI is disabled although checked-in workflows have push/PR triggers (remote enablement was not checked), still lists implemented daemon/status work as later, and promises Node 20+ although current Vite requires 20.19+/22.12+. Frontend tests invoke TypeScript directly through Node, which also means the test command is not uniformly supported by the documented Node 20 floor; CI already uses Node 24. The HTML document retains the scaffold title. Release tags build bundles without rerunning the test/typecheck/Clippy gates, and no signing/notarization configuration is evident in the workflow.

**Fix:** establish a supported Node/toolchain policy, correct docs/title/formatting, gate release tags on verification, and separately decide signing/notarization requirements. These are not claims that remote CI is currently enabled or that a packaged app was tested.

## Additional hardening and maintainability recommendations

These are not counted as demonstrated bugs:

- **R01 — CSP / native trust boundary:** `src-tauri/tauri.conf.json:24` disables CSP. Enable a tested production policy compatible with Tauri IPC and Svelte/xterm styling. A hypothetical webview injection would be particularly serious because custom commands spawn shells; no such injection was demonstrated. `Icon.svelte` uses `{@html}` only with a fixed local icon table, so it is not evidence of an XSS vulnerability. Current opener URL scope is restricted to default safe schemes, not an unrestricted arbitrary-scheme opener.
- **R02 — Output fan-out:** every `TerminalPane` installs a global output listener, causing each chunk to be dispatched to all terminal components. Many panes plus high output warrant a central dispatcher and bounded batching, but measure before optimizing. Existing hidden panes deliberately remain mounted to preserve processes.
- **R03 — Long-lived status bookkeeping:** `AgentModel.seenEvents` grows and is cloned on every update (`agentStatus.ts:55`); `AgentStore.disposed` grows and is scanned on every update (`agent.svelte.ts:36,190`). Add a revision/retention strategy before long-running scale tests. Agent subscriptions currently have no disposer/retry path; acceptable for one permanent page, fragile for hot reload or later lifecycle changes.
- **R04 — Async preferences:** custom sound validation and launch-at-login toggles can complete out of order. Gate requests by revision or disable controls while applying; show autostart-load failures instead of leaving a permanently disabled toggle with only a console log.
- **R05 — Durable save semantics:** layout persistence writes a fixed `.tmp` then renames with no sync/backup/serialized writer contract. Avoid claiming crash durability beyond atomic replacement; add a last-known-good backup and an explicit shutdown flush of debounced edits.
- **R06 — Audio pressure:** sound messages and appended audio are unbounded; frequent events or long custom files can delay alerts or consume resources. Define queue/duration limits and playback overlap/coalescing behavior.
- **R07 — Accessibility polish:** titles on icon buttons are weaker than explicit contextual labels; status dots lack consistent accessible text. Check all themes for small-text contrast, enable xterm accessibility features intentionally, and verify reduced motion. No automated WCAG certification is implied.

## Strengths to retain

- Pure layout/shortcut/notification/status modules with useful behavioral tests.
- One backend status-history owner and cached read-only snapshots; monotonic revisions, explicit instance identities, transition IDs, and no notification replay from snapshots.
- Conservative source-backed screen rules and explicit Unknown state rather than idle-time guesswork.
- Stable keyed pane layout preserving existing PTYs during splits, zoom, and ordinary tab switches.
- UTF-8 streaming decoder and EINTR retry in the PTY reader.
- Loopback-only daemon binding, bounded status wakeups, dependency lockfiles, and a cross-platform verification workflow.

## Recommended implementation order

1. Validate dimensions, control process ownership, and secure daemon access.
2. Bound daemon resources and correct CLI/headless-platform behavior.
3. Repair attach/spawn lifecycle and make saved-layout recovery safe.
4. Harden schema/tray/CSP behavior.
5. Fix notification settings and keyboard/focus behavior.
6. Address navigation, clean-platform CI, dependencies, and release gates.

See `tasks/todo.md` for small, independently verifiable tasks and review checkpoints. Pending product decisions are listed in `tasks/plan.md`.

## Remediation verification

The findings above are the historical audit, not the current implementation
status. All 18 remediation changes have been applied on `audit-remediation`.
Available checks pass; Tasks 6, 11, and 12 retain native/manual verification
blockers in `tasks/todo.md`.

- Frontend: **108 tests passed**; Svelte check: **0 errors, 0 warnings**.
- Rust: **107 tests passed**; Clippy with `-D warnings` and formatting passed.
- Frontend production build and final macOS `.app`/`.dmg` bundle build passed.
- `npm audit`: **0 vulnerabilities**, after the targeted `cookie@0.7.2` override.
  Cookie parse/serialize compatibility and rejection of injected invalid fields
  were exercised separately; no forced framework downgrade was used.
- `cargo audit` is not installed; **no Rust advisory scan result is claimed**.
- Real daemon/CLI smoke: unauthorized access rejected, invalid dimensions
  rejected without losing the pane, missing-pane failures exited nonzero,
  resistant child termination preserved a writable sibling, split cursor
  queries received replies, and authenticated shutdown succeeded.
- Slow-reader load: 32 MiB requested; the lagging socket disconnected, a healthy
  ping took 6 ms, and shutdown completed in 131 ms. These are single-run smoke
  observations, not performance guarantees.
- Live Chromium UI with instrumented IPC: pending-spawn moves adopted one
  session; snapshot overlap was excluded and a buffered exit showed Respawn;
  recovery Retry/Export/Reset errors and consent paths worked; modal/menu focus,
  shifted shortcuts and terminal Ctrl+D worked; 30 workspaces, 30 tabs, and 59
  agent rows remained scrollable with Settings visible at a 900×600 viewport.
  Both preview and real agent-transition playback selected Bright with a null
  custom-file payload despite a retained custom path; native audibility is not
  claimed.
- Final packaged macOS app: a real terminal command created a marker; Quit
  exited zero and left no owned process. Corrupt and future-version layouts
  remained byte-for-byte intact and spawned no terminal. Forced tray-off debug
  startup remained running, and independent native Quit exited zero.
- The actual release gate script rejected missing/stale smoke SHAs and accepted
  three matching SHAs. Clean Linux/Windows runs, the complete packaged
  clipboard/settings/links/notifications/attachment/tray-close checklist, and
  native visual review remain unverified. Accessibility automation was disabled
  and native window screenshot capture was denied.

Release publication remains blocked until the native checklist is completed for
the release commit and the release environment's per-platform evidence is set.
Signing/notarization and human release approval remain outside these results.

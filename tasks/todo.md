# Audit remediation tasks

Status: **implementation applied for all 18 tasks; 15 verified with available tooling; Tasks 6, 11, and 12 retain native/manual QA blockers**. No release approval is claimed. Preserve pre-existing working-tree changes. Findings remain in `tasks/audit.md`; execution decisions are in `tasks/plan.md`.

Checked boxes describe available macOS and/or live Chromium evidence, not a clean three-platform certification. Chromium UI smoke used instrumented IPC; actual daemon/CLI and packaged macOS smoke used real PTYs. Platform-specific or full packaged interactions remain unchecked where unavailable.

Final checks: **108 frontend tests, 107 Rust tests, Svelte check (0 errors/0 warnings), frontend build, macOS `.app`/`.dmg` build, Clippy, formatting, and `npm audit` (0 vulnerabilities)**. `cargo audit` is unavailable. Detailed runtime evidence and limits are recorded in `tasks/audit.md` → Remediation verification.

| Tasks | Implementation / verification status |
| --- | --- |
| 1–5 | Implemented; macOS tests and real daemon/process/CLI smoke passed. Windows/Linux process ownership and ACL execution remain native release gates. |
| 6 | Implemented; split/repeated headless queries passed on macOS. **Blocked:** actual Windows ConPTY/PowerShell build and execution. |
| 7–10 | Implemented; regression and browser lifecycle/recovery checks passed. Actual packaged corrupt/future-version layouts were preserved and spawned no terminals. |
| 11 | Implemented; forced tray-off debug startup and independent native Quit passed. **Blocked:** native window-close/hide-failure recovery and visual warning checks across supported platforms. |
| 12 | Implemented; packaged macOS bootstrap, IPC, real terminal command, and Quit passed. **Blocked:** full native clipboard/settings/links/notifications/attachment and console/visual smoke. |
| 13–16 | Implemented; tests and live browser sound/keyboard/focus/navigation checks passed. Native audible playback and full desktop interaction remain release checks. |
| 17–18 | Implemented; local checks/bundles/advisory resolution passed. Actual publication gate rejected missing/stale evidence and accepted matching evidence. Clean Linux/Windows execution and human release approval remain outstanding. |

Native UI automation was unavailable (`System Events` reported Accessibility disabled); `screencapture` could not capture the app window. No component runner dependency was introduced. Follow README's native checklist and set the release environment's per-platform smoke SHAs only after completing it for the release commit.

## Task 1: Reject unsafe terminal dimensions

**Description:** Fix A02 at the shared PTY boundary so both GUI and daemon reject unsafe geometry before touching OS/parser state.

**Acceptance criteria:**
- [x] Zero/unsafe dimensions and excessive cell allocations return validation errors without panic or partial mutation.
- [x] Valid resize/spawn behavior remains unchanged, including wide-character output at the supported minimum width.
- [x] Rejected operations leave the same pane writable, readable, and killable.

**Verification:**
- [x] Tests: `cargo test --manifest-path src-tauri/Cargo.toml --test pty_echo` with dimension boundary cases.
- [x] Build: debug application and final macOS Tauri release bundles.
- [x] Real daemon smoke: zero-row resize returned an error and subsequent valid operations succeeded.

**Dependencies:** None.
**Files likely touched:** `src-tauri/src/pty_manager.rs`, `src-tauri/tests/pty_echo.rs`.
**Estimated scope:** Small: 2 files.

## Task 2: Terminate owned pane process trees

**Description:** Fix A03 with bounded platform-appropriate process ownership and shutdown, including child reaping and cleanup on failure.

**Acceptance criteria:**
- [x] Closing/killing a pane terminates ordinary owned descendants, including SIGHUP-resistant children, under the documented detachment policy.
- [x] Closing one pane never signals another pane or unrelated process.
- [x] Shutdown escalates within a bounded interval and releases reader/session resources, with deliberate-close notifications remaining silent.

**Verification:**
- [x] macOS tests: `cargo test --manifest-path src-tauri/Cargo.toml --test pty_echo`, including resistant children, natural EOF after root exit, siblings, and closed spawn admission.
- [x] Build: debug application and final macOS Tauri release bundles.
- [x] Real daemon kill and packaged Quit: owned PIDs exited while a sibling daemon pane remained writable. Linux/Windows native process checks remain release gates.

**Dependencies:** Task 1; approved process-detachment policy.
**Files likely touched:** `src-tauri/src/pty_manager.rs`, `src-tauri/src/process_tree.rs` (new if justified), `src-tauri/src/lib.rs`, `src-tauri/tests/pty_echo.rs`, `src-tauri/Cargo.toml`.
**Estimated scope:** Medium: up to 5 files, plus generated lockfile if a platform dependency is necessary.

## Task 3: Authenticate daemon sessions and secure startup

**Description:** Fix A01 through one approved local authentication/identity contract, secure runtime files, and single-owner startup.

**Acceptance criteria:**
- [x] Unauthenticated/wrong-user clients cannot receive greetings, output, snapshots, or execute commands; valid CLI clients still work.
- [x] Runtime credentials/logs/state are securely created with restrictive access and no symlink-following privilege confusion.
- [x] Concurrent starts produce one daemon owner, and stale/fake port endpoints are not accepted as a valid daemon.

**Verification:**
- [x] Tests: `cargo test --manifest-path src-tauri/Cargo.toml --test daemon_e2e` with auth, stale endpoint, and concurrent startup cases.
- [x] Build: debug application and final macOS Tauri release bundles.
- [x] Real daemon smoke: unauthorized connection received no pane data; CLI authenticated and shutdown succeeded. Windows DACL execution remains a native gate.

**Dependencies:** None; approved IPC/authentication choice.
**Files likely touched:** `src-tauri/src/daemon.rs`, `src-tauri/src/cli.rs`, `src-tauri/src/bin/ubra-daemon.rs`, `src-tauri/tests/daemon_e2e.rs`, `src-tauri/Cargo.toml` if required.
**Estimated scope:** Medium: up to 5 files, plus generated lockfile if needed. If replacing TCP requires more files, split that transport migration into a separate approved plan.

## Checkpoint: After Tasks 1–3
- [x] Available PTY validation and process-tree isolation tests pass.
- [x] Unauthorized access is rejected before data exposure.
- [x] Full frontend/Rust suites, frontend/macOS build, Clippy, and formatting pass.
- [ ] Human reviews platform policy and security boundary before proceeding.

## Task 4: Bound daemon connection and output resources

**Description:** Fix A09 without corrupting terminal streams or blocking PTY/status producers behind slow consumers.

**Acceptance criteria:**
- [x] Connection count, frame size, pane count, and output queue budgets are explicitly bounded.
- [x] Lagging/disconnected consumers are removed with bounded deadlines rather than silently losing bytes.
- [x] A stalled client does not prevent healthy clients from spawning, writing, reading, or shutting down.

**Verification:**
- [x] Tests: `cargo test --manifest-path src-tauri/Cargo.toml --test daemon_e2e` with slow-reader/oversized-request cases.
- [x] Build: debug application and final macOS Tauri release bundles.
- [x] Real slow-reader smoke: 32 MiB output requested; lagging socket disconnected, healthy ping took 6 ms, shutdown took 131 ms.

**Dependencies:** Task 3.
**Files likely touched:** `src-tauri/src/daemon.rs`, `src-tauri/tests/daemon_e2e.rs`.
**Estimated scope:** Small: 2 files.

## Task 5: Enforce CLI failure and shutdown semantics

**Description:** Fix A08 across all commands with a validated response contract and finite operation deadlines.

**Acceptance criteria:**
- [x] Daemon `ok:false`, malformed replies, and transport failures cause nonzero CLI exits while preserving useful JSON/error output.
- [x] Shutdown waits past greetings/events for a matching acknowledgement and does not report arbitrary replies as success.
- [x] Silent endpoints and wait commands obey explicit deadlines without false failure after ordinary event bursts.

**Verification:**
- [x] Tests: `cargo test --manifest-path src-tauri/Cargo.toml --test daemon_e2e` covering nonexistent pane, invalid executable, silent endpoint, and shutdown responses.
- [x] Build: debug application and final macOS Tauri release bundles.
- [x] Real CLI smoke: missing-pane read exited nonzero; valid read succeeded.

**Dependencies:** Tasks 3–4.
**Files likely touched:** `src-tauri/src/cli.rs`, `src-tauri/src/bin/ubra-cli.rs`, `src-tauri/tests/daemon_e2e.rs`.
**Estimated scope:** Medium: 3 files.

## Task 6: Answer headless Windows terminal queries

**Description:** Fix A10 by providing a single-owner query responder when no interactive terminal frontend exists.

**Acceptance criteria:**
- [ ] Windows daemon/CLI panes progress beyond ConPTY startup and execute submitted commands.
- [x] Split/repeated queries are handled by the headless responder; GUI managers leave replies to xterm. macOS real headless reply smoke passed; Windows remains unverified.
- [ ] Tests distinguish executed output from input echo and prove clean shutdown.

**Verification:**
- [ ] Tests on Windows: `cargo test --manifest-path src-tauri/Cargo.toml --test daemon_e2e` and `--test pty_echo`.
- [ ] Build on Windows: `cargo build --manifest-path src-tauri/Cargo.toml`.
- [ ] Manual on Windows: spawn a headless pane, execute a command with an observable side effect, read output, then close.

**Dependencies:** Tasks 3–5.
**Files likely touched:** `src-tauri/src/daemon.rs`, `src-tauri/src/pty_manager.rs`, `src-tauri/tests/daemon_e2e.rs`, `src-tauri/tests/pty_echo.rs`.
**Estimated scope:** Medium: 4 files.

## Checkpoint: After Tasks 4–6
- [x] Stress tests and real socket smoke demonstrate bounded resources and healthy-client progress.
- [x] CLI automation detects failures and confirms shutdown.
- [ ] Windows headless execution is verified natively; Unix behavior still passes.
- [ ] Human reviews before changing frontend lifecycle contracts.

## Task 7: Make terminal attachment sequence-safe

**Description:** Fix A04 with an atomic snapshot/output watermark and exit reconciliation, migrating the existing GUI callers together.

**Acceptance criteria:**
- [x] A moved pane replays only output newer than its snapshot, with no lost or duplicated terminal operations.
- [x] Buffered exit during snapshot/attachment produces the exited/respawn UI and drops stale ownership.
- [x] Snapshot regressions preserve cursor, primary/alternate buffers, supported modes, and unfinished escape input; full native continuous-output movement remains in Task 12's packaged gate.

**Verification:**
- [x] Tests: full Rust suite including `terminal_state` and `pty_echo`; `node --test tests/terminalLifecycle.test.ts`.
- [x] Build: frontend check/build, debug application, and macOS Tauri bundles.
- [x] Browser runtime with instrumented IPC: moved pane replayed only newer output and displayed Respawn after buffered exit. Native full-screen movement remains a release gate.

**Dependencies:** Tasks 1–2.
**Files likely touched:** `src-tauri/src/pty_manager.rs`, `src-tauri/src/lib.rs`, `src/lib/TerminalPane.svelte`, `src/lib/ptySessions.ts` for pure replay-state logic, `tests/terminalLifecycle.test.ts` (new).
**Estimated scope:** Medium: 5 files. Keep native sequencing tests colocated in the manager; if the sink contract forces changes to independent daemon/test callers, split that shared event-contract migration into a prerequisite rather than expanding this task.

## Task 8: Share pending-spawn ownership across remounts

**Description:** Fix A05 by making the registry own a pending/live session lease independent of TerminalPane mounting.

**Acceptance criteria:**
- [x] Moving a node while spawn is unresolved adopts one pending spawn and does not kill the adopted session.
- [x] True close before resolution kills the eventual session exactly once; stale callbacks cannot replace newer ownership.
- [x] Failure clears the pending lease and permits a deliberate retry without duplicate processes.

**Verification:**
- [x] Tests: `node --test tests/ptySessions.test.ts tests/terminalLifecycle.test.ts` with deferred promises and rapid moves.
- [x] Build: `npm run check && npm run build`.
- [x] Browser runtime with instrumented IPC: delayed spawn moved successfully; one original spawn, no adopted-session kill, original pane rendered in its new location. Empty-source-tab replacement was accounted separately.

**Dependencies:** Task 7.
**Files likely touched:** `src/lib/ptySessions.ts`, `src/lib/TerminalPane.svelte`, `tests/ptySessions.test.ts`, `tests/terminalLifecycle.test.ts`.
**Estimated scope:** Medium: 4 files.

## Task 9: Preserve failed or unsupported saved layouts

**Description:** Fix A06 with explicit recovery state and preservation of the original before autosave can replace it.

**Acceptance criteria:**
- [x] Missing layout triggers first run, but unreadable/corrupt/unsupported layouts trigger recovery without destructive autosave.
- [x] Retry/export/reset choices clearly report errors and preserve the original until explicit reset/repair consent.
- [x] After recovery, normal persistence resumes and round-trips the new layout correctly.

**Verification:**
- [x] Tests: Rust `layout_store` and frontend layout regression suites.
- [x] Build: frontend check/build, debug application, and macOS Tauri bundles.
- [x] Browser Retry/Export/Reset success/error flows passed; final packaged app preserved corrupt/future-version originals byte-for-byte and spawned no terminals.

**Dependencies:** None; recovery UX decision.
**Files likely touched:** `src/lib/store.svelte.ts`, `src/lib/layout.ts`, `src/routes/+page.svelte`, `src-tauri/src/layout_store.rs`, `tests/layout.test.ts`.
**Estimated scope:** Medium: 5 files; keep Rust regressions colocated in the existing module.

## Checkpoint: After Tasks 7–9
- [x] Move/spawn/exit races are covered and preserve one adopted session.
- [ ] Printing/full-screen panes survive moves correctly in the desktop app.
- [x] Invalid saved files remain recoverable without silent replacement.
- [ ] Full suites/build and human review.

## Task 10: Validate saved-layout identities and geometry

**Description:** Fix A07 with safe schema limits and globally unique stable identities before rendering or session registration.

**Acceptance criteria:**
- [x] Duplicate/empty IDs across the full layout are rejected deterministically; new IDs retain full UUID entropy.
- [x] Ratios are finite, nondegenerate, and normalized without overflow; depth/count/document limits are enforced.
- [x] Valid existing layouts and active/zoom references round-trip, and unsafe documents enter recovery without overwriting the original.

**Verification:**
- [x] Tests: frontend layout suite with cross-workspace duplicate IDs, extreme values, and excessive depth.
- [x] Build: `npm run check && npm run build`.
- [x] Browser malformed-layout recovery and final packaged corrupt/future-version preservation smoke passed; no default PTY was spawned during native recovery.

**Dependencies:** Task 9.
**Files likely touched:** `src/lib/layout.ts`, `tests/layout.test.ts`, `src/lib/store.svelte.ts` only if recovery wiring needs it.
**Estimated scope:** Medium: up to 3 files.

## Task 11: Fall back safely when tray creation fails

**Description:** Fix A11 with an explicit reachable close/quit policy when the tray cannot be created.

**Acceptance criteria:**
- [ ] Hide-on-close is enabled only when the app can expose a usable recovery surface.
- [ ] Tray/hide failure follows the approved fallback policy with visible user feedback.
- [x] Quit remains reachable independently of tray availability and terminates owned sessions. Forced tray-off startup and native application Quit exited zero; Settings exposes its own Quit command.

**Verification:**
- [x] Checks: full Rust suite and Clippy passed. No source/wiring-only close-policy unit test was added; native behavior is the required proof.
- [x] Build: debug application and final macOS Tauri release bundles.
- [ ] Native manual: force tray failure and close the window on Linux/macOS/Windows; verify fallback warnings and recovery/exit. Debug builds support `UBRA_DISABLE_TRAY=1`; release builds ignore it.

**Dependencies:** Task 2; approved tray-fallback policy.
**Files likely touched:** `src-tauri/src/lib.rs`, `src/lib/SettingsModal.svelte` if an explicit Quit control is chosen.
**Estimated scope:** Small: up to 2 files.

## Task 12: Enable a compatible production CSP

**Description:** Implement R01 as defense in depth around the native command boundary without claiming a demonstrated injection exploit.

**Acceptance criteria:**
- [ ] Production CSP blocks unneeded remote scripts/connections while permitting required Tauri IPC and application assets/styles.
- [ ] Terminal rendering, clipboard, settings, links, notifications, and attachment work in a packaged build.
- [x] Policy exceptions are narrowly scoped and documented; no native capability expansion was needed.

**Verification:**
- [x] Checks: frontend check/unit tests and full native suite passed.
- [x] Build: final macOS `npm run tauri build` passed (`.app` and `.dmg`); Linux/Windows builds remain native gates.
- [ ] Full packaged smoke/console/visual check. macOS bootstrap/IPC/real command/Quit passed, but this does not cover clipboard, links, notifications, settings, or movement.

**Dependencies:** Task 7 for attachment smoke coverage.
**Files likely touched:** `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json` only if needed, `README.md`.
**Estimated scope:** Medium: up to 3 files.

## Checkpoint: After Tasks 10–12
- [x] Layout/schema invariants prevent crashes and ownership confusion in available regression and recovery checks.
- [ ] Tray-unavailable fallback works natively.
- [ ] Packaged CSP checks pass; no unnecessary native permissions added.
- [ ] Full suites/build and human review.

## Task 13: Honor selected chime in every playback path

**Description:** Fix A12 through one pure playback-payload helper used by previews, test notifications, and real agent notifications.

**Acceptance criteria:**
- [x] A custom path is passed only when `soundStyle` is custom; built-in styles ignore a retained path.
- [x] Preview/test/real playback use identical selection semantics and existing mute/delivery rules.
- [x] Returning to custom restores the retained path; invalid-file fallback still works.

**Verification:**
- [x] Tests: frontend notification suite with custom-to-built-in payload cases.
- [x] Build: `npm run check && npm run build`.
- [x] Live browser: select custom, retain its path, select Bright; preview and a real agent-transition callback both invoked Bright with `file:null`. Native audible playback remains a release check.

**Dependencies:** None.
**Files likely touched:** `src/lib/notify.ts`, `src/lib/SettingsModal.svelte`, `src/lib/agent.svelte.ts`, `tests/notify.test.ts`.
**Estimated scope:** Medium: 4 files.

## Task 14: Match real shortcut events and respect keyboard ownership

**Description:** Fix A13 and the global-dispatch portion of A14 without intercepting unrelated terminal keys.

**Acceptance criteria:**
- [x] Shifted plus/braces and documented shortcuts match intended actions with realistic event values.
- [x] Mod means Cmd on macOS and Ctrl elsewhere; ordinary terminal Ctrl keys remain available.
- [x] Settings/onboarding/confirmation/menu overlays own the keyboard, preventing background layout actions unless explicitly allowed.

**Verification:**
- [x] Tests: frontend shortcut suite covering actual key/code/modifier combinations and overlay eligibility.
- [x] Build: `npm run check && npm run build`.
- [x] Browser keyboard smoke on macOS: shifted plus resized font, Ctrl+D reached the terminal input path, and Mod+T did not add a hidden tab behind Settings. Native desktop interaction remains a release gate.

**Dependencies:** None; coordinate overlay contract with Task 15.
**Files likely touched:** `src/lib/shortcuts.ts`, `src/routes/+page.svelte`, `tests/shortcuts.test.ts`, `src/lib/store.svelte.ts` if shared overlay state is necessary.
**Estimated scope:** Medium: up to 4 files.

## Task 15: Contain and restore overlay focus

**Description:** Fix the remaining A14 with usable keyboard menus, focus containment/restoration, and visible keyboard-focused controls.

**Acceptance criteria:**
- [x] Reopened onboarding and dialogs prevent keyboard focus reaching obscured background controls, restoring useful focus on dismissal.
- [x] Context menus take focus, support expected directional/Escape navigation, and return focus to their opener.
- [x] Pane action buttons are visibly usable when keyboard-focused, not just hovered.

**Verification:**
- [x] Checks: `npm run check && npm run test:unit`; no component runner added. Live browser interactions and screenshots supplied UI evidence.
- [x] Build: `npm run build`.
- [x] Browser keyboard-only onboarding, settings, close-cancel, and menu flows: focus remained inside overlays and restored to terminal/opener; pane controls were visible when focused.

**Dependencies:** Task 14; component-testing decision. If introducing a runner requires extra setup files, make that a separately approved prerequisite.
**Files likely touched:** `src/lib/FirstRun.svelte`, `src/lib/ContextMenu.svelte`, `src/lib/ConfirmDialog.svelte`, `src/lib/PaneView.svelte`, a focused component regression test file (new).
**Estimated scope:** Medium: 5 files; retain the existing Settings focus trap and shared dispatch fix rather than expanding this task.

## Checkpoint: After Tasks 13–15
- [x] Sound selection payloads work across real and preview paths; native audibility remains unchecked.
- [x] Real browser keyboard events match and terminal Ctrl combinations remain forwarded.
- [x] Browser focus containment/restoration and visible keyboard controls verified.
- [ ] Full frontend tests/build and human review.

## Task 16: Keep large navigation layouts usable

**Description:** Fix A15 through bounded scrolling regions and stable navigation controls without remounting live terminals.

**Acceptance criteria:**
- [x] Many workspaces/agents can be scrolled while Settings and add controls remain reachable.
- [x] Many tabs keep readable/clickable controls and reveal the active tab through scrolling.
- [x] Long names/small windows do not hide controls or terminate/remount adopted sessions.

**Verification:**
- [x] Checks: `npm run check && npm run test:unit`; no component runner dependency added.
- [x] Build: `npm run build`.
- [x] Browser runtime: 30 workspaces, 30 tabs, and 59 agent rows at 900×600; Settings remained visible, active tabs stayed readable/revealed, and instrumented session counts remained stable.

**Dependencies:** Tasks 10, 15.
**Files likely touched:** `src/lib/Sidebar.svelte`, `src/lib/TabBar.svelte`, a focused navigation component test file if available.
**Estimated scope:** Medium: up to 3 files.

## Task 17: Make platform and release verification reproducible

**Description:** Fix A16/A18 by explicitly declaring Linux audio prerequisites, enforcing release verification, and aligning formatting/documented toolchain expectations.

**Acceptance criteria:**
- [x] CI/release explicitly install Linux ALSA development prerequisites and run verification before publishing tag artifacts.
- [x] Rust formatting passes and documentation accurately states Node 24+, workflow triggers, and implemented features.
- [x] Native evidence is a publication blocker: missing/stale per-platform smoke SHAs fail the actual gate script. Clean Linux/Windows runs and the complete macOS native checklist remain outstanding.

**Verification:**
- [x] Full available frontend/Rust commands, including Clippy and formatting, passed.
- [x] Final macOS Tauri `.app`/`.dmg` build passed; clean supported-platform certification remains unclaimed.
- [x] Inspected Linux ALSA prerequisites, Node 24 configuration, verify-before-build/publish dependencies, and commit-bound native smoke evidence; executed the actual publication script for missing, stale, and matching SHAs.

**Dependencies:** Earlier runtime/UI fixes for final release verification; prerequisite preparation may run earlier.
**Files likely touched:** `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `src-tauri/src/lib.rs`, `README.md`, `package.json` for an engines declaration if chosen.
**Estimated scope:** Medium: 5 files. The scaffold HTML title is a separate trivial follow-up; signing/notarization remains a separate scope decision.

## Task 18: Resolve the frontend advisory safely

**Description:** Fix A17 through a compatible targeted dependency change or a documented applicability decision; avoid forced historical downgrades.

**Acceptance criteria:**
- [x] Underlying cookie advisory resolved by the targeted `cookie@0.7.2` override; `npm audit` reports zero vulnerabilities.
- [x] Lockfile reflects the compatible targeted resolution without forced framework downgrades; parse/serialize and invalid-field rejection were smoke-tested.
- [x] Frontend tests/checks/build and basic packaged macOS startup/terminal/recovery/Quit smoke passed. Full native settings/CSP interaction remains gated by Task 12; `cargo audit` is not installed.

**Verification:**
- [x] Frontend check/unit tests and `npm audit` passed (108 tests, zero vulnerabilities).
- [x] Frontend production build and final macOS Tauri `.app`/`.dmg` build passed.
- [x] Packaged macOS startup/terminal command and Quit passed; browser Settings passed. Native Settings and the full packaged checklist remain outstanding; no Rust advisory scan result is claimed.

**Dependencies:** Task 17 for the reproducible verification baseline.
**Files likely touched:** `package.json`, `package-lock.json`, `tasks/audit.md` for updated advisory applicability/results.
**Estimated scope:** Medium: 3 files.

## Checkpoint: After Tasks 16–18 / Complete
- [ ] All acceptance criteria met or deviations explicitly approved.
- [ ] Full frontend/Rust checks pass and platform bundles are verified.
- [x] Large-layout browser navigation remains usable with stable instrumented session ownership.
- [x] npm advisory results are understood; release-tag quality and native-evidence gates are enforced. Rust advisory scan remains unavailable.
- [ ] Human approves release readiness and decides deferred hardening/signing work.

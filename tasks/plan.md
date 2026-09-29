# Implementation Plan: Whole-app audit remediation

## Overview

Remediate the correctness, security, recovery, and usability issues identified in `tasks/audit.md` without redesigning Ubra or migrating the GUI to the experimental daemon. Implementation has been applied for all 18 tasks on `audit-remediation`, preserving the existing working-tree work. Available macOS checks pass: 108 frontend tests, 107 Rust tests, Svelte check, frontend/macOS bundle builds, Clippy, formatting, and zero npm advisories. Native QA for Tasks 6, 11, and 12 remains blocked; this is not release approval. See the verification record in `tasks/todo.md`.

Tasks are tracked in **`tasks/todo.md`**. The user's instruction to execute this plan authorized implementation, not publication, signing, or approval of unperformed platform checks. Remaining native/manual checks stay unchecked and publication is gated on matching per-platform smoke evidence.

## Architecture Decisions

- Keep the desktop's in-process PTY backend and the headless daemon separate. Shared invariants belong in `PtyManager`; do not duplicate safety checks only in callers.
- Treat a PTY session as a lifecycle resource owned by a stable pane identity, not by a transient Svelte component. Pending spawns need the same ownership as live sessions.
- Make reattachment a sequence-based protocol: snapshot and output events require a shared watermark, with buffered exits reconciled. Do not paper over duplicate output with delays.
- Define owned-process shutdown explicitly across platforms. An ordinary pane close must not leave owned children running or kill unrelated panes. Detached processes need a documented product policy.
- Authenticate daemon clients before exposing snapshots or output. Keep binding loopback-only even after authentication; secure runtime files and verify the server identity.
- Bound connection/output/request resources. Disconnect lagging terminal consumers rather than dropping arbitrary terminal bytes and pretending the screen remains consistent.
- Preserve invalid/unsupported saved documents and require an explicit recovery choice before destructive saves. Keep first-run onboarding separate from recovery.
- Continue extracting pure lifecycle/payload/shortcut helpers for Node tests. Where behavior is inherently DOM/native, require focused component/native tests or a documented manual check; pure helper tests alone are insufficient.
- Preserve the centralized backend status service and conservative rule semantics. Do not replace explicit evidence with silence/typing heuristics during remediation.
- Apply CSP as defense in depth, not as proof that an exploitable injection currently exists.
- Use existing conventions and small vertical fixes. Do not introduce a second tracker, an unrelated UI framework, or a monolithic refactor.

## Dependency Graph

```text
1 PTY dimensions ──> 2 process ownership ──> 7 attachment sequencing
                           │                        │
                           └────────────────────────> 8 pending-spawn ownership

3 daemon authentication/startup ──> 4 resource limits ──> 5 CLI result contract
                     │                                      │
                     └─────────────────────────────────────> 6 Windows headless queries

9 safe layout recovery ──> 10 schema/identity validation
11 tray fallback                         (independent)
12 CSP                                   (after attachment smoke coverage)
13 sound payload                         (independent)
14 shortcut ownership ──> 15 overlay focus
16 navigation overflow                   (after schema and overlay decisions)
17 clean-platform/release verification ──> 18 dependency advisory resolution
```

The graph constrains implementation order; the numbered task list is one valid sequence. Task 17 can be brought forward to unblock Linux verification once its scope is agreed.

## Task List

### Phase 1: PTY and daemon safety
- [ ] Task 1: Reject unsafe terminal dimensions (A02).
- [ ] Task 2: Terminate owned pane process trees (A03).
- [ ] Task 3: Authenticate daemon sessions and secure startup (A01).

### Checkpoint: Safety
- [ ] Invalid dimensions leave healthy sessions intact.
- [ ] Ordinary pane close terminates owned jobs without affecting other panes.
- [ ] Unauthenticated clients receive neither output nor command access.
- [ ] Full tests, frontend build, Clippy, formatting pass; human review.

### Phase 2: Headless reliability
- [ ] Task 4: Bound daemon connection and output resources (A09).
- [ ] Task 5: Enforce CLI failure and shutdown semantics (A08).
- [ ] Task 6: Answer headless Windows terminal queries (A10).

### Checkpoint: Headless runtime
- [ ] Slow clients cannot cause unbounded growth.
- [ ] Scripts reliably observe nonzero exit codes and finite timeouts.
- [ ] Windows commands actually execute in headless PTYs.
- [ ] Cross-platform focused verification and human review.

### Phase 3: Session and recovery correctness
- [ ] Task 7: Make terminal attachment sequence-safe (A04).
- [ ] Task 8: Share pending-spawn ownership across remounts (A05).
- [ ] Task 9: Preserve failed/unsupported saved layouts (A06).

### Checkpoint: Lifecycle and recovery
- [ ] Moves preserve one session and a correct screen, including in-flight spawn/exit.
- [ ] Recovery never overwrites the original without consent.
- [ ] Full tests/build/native lifecycle checks and human review.

### Phase 4: Invariants and desktop hardening
- [ ] Task 10: Validate saved-layout identities and geometry (A07).
- [ ] Task 11: Fall back safely when tray creation fails (A11).
- [ ] Task 12: Enable a compatible production CSP (R01).

### Checkpoint: Desktop resilience
- [ ] Invalid layouts cannot break keyed rendering or cross-wire PTYs.
- [ ] Users can recover/quit without a tray.
- [ ] Packaged frontend works under CSP without unnecessary permissions.
- [ ] Full tests/build and human review.

### Phase 5: Settings and keyboard interactions
- [ ] Task 13: Honor selected chime in every playback path (A12).
- [ ] Task 14: Match real shortcut events and respect overlay/platform ownership (A13, part of A14).
- [ ] Task 15: Contain and restore overlay focus (remaining A14).

### Checkpoint: Interaction quality
- [ ] Built-in/custom sound switching, shifted shortcuts, and terminal Ctrl input work.
- [ ] Keyboard users cannot accidentally act on an obscured background app.
- [ ] Component/manual accessibility checks and human review.

### Phase 6: Scale and release hygiene
- [ ] Task 16: Keep large navigation layouts usable (A15).
- [ ] Task 17: Make platform/release verification reproducible (A16, A18).
- [ ] Task 18: Resolve the frontend dependency advisory safely (A17).

### Checkpoint: Complete
- [ ] All task acceptance criteria met.
- [ ] All tests/checks/builds pass on supported platforms, or limitations explicitly accepted.
- [ ] Release verification runs on tags; advisories have fixes or documented applicability.
- [ ] Packaged manual smoke checklist passes; human sign-off before release.

## Verification Strategy

Baseline commands:

```sh
npm run check
npm run test:unit
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
npm audit
```

Use focused filters in `tasks/todo.md` during each task and the full suite at checkpoints. `cargo test` is not equivalent to a packaged desktop smoke test. Use `npm run tauri build` for bundle verification on macOS, Linux, and Windows. Native/manual tests must use scratch `--state-dir` / `UBRA_DATA_DIR` values; never connect destructive probes to the user's existing daemon. Capture Windows ConPTY execution and Linux tray-unavailable behavior explicitly.

Frontend lifecycle tests should exercise deferred invoke resolution and reordered events. Add a small component-test harness only if needed; its introduction and dependencies require review rather than silently replacing the current runner. Performance hardening should be checked with bounded, reproducible workloads, not an arbitrary source-code cleanliness threshold.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Existing source changes are active work | High | Preserve working tree; agree task ownership before editing shared files |
| Process groups do not encompass every descendant/job-control arrangement | High | Explicit ownership policy, platform primitives, adversarial shutdown tests, isolated fixtures |
| Authentication breaks experimental CLI clients | Medium | Version the protocol; clear errors; coordinate daemon/CLI rollout; GUI remains independent |
| Snapshot watermark does not cover terminal modes or exits | High | Atomic snapshot/event contract and full lifecycle tests rather than timestamp heuristics |
| Recovery writes destroy user data | High | Original-file preservation and explicit consent tests, including unsupported version |
| Windows/Linux behavior differs from macOS tests | High | Require native CI and manual platform checks; do not infer platform success |
| CSP breaks framework/xterm styling or IPC | Medium | Packaged smoke tests; narrowly documented exceptions rather than disabling all policy |
| Advisory automation recommends incompatible downgrades | Medium | Targeted compatible dependency resolution; no blind force-fix |
| Broad UI fix turns into redesign | Medium | Limit changes to focus/keyboard/overflow behavior and existing tokens/components |

## Execution Decisions and Outstanding Release Inputs

Execution used the conservative defaults announced before implementation:

1. Daemon remains experimental and separate from the GUI, with mutually authenticated per-user loopback TCP (protocol 2).
2. Ordinary owned jobs terminate on close and terminal EOF, including children that outlive their root after closing inherited terminal handles; intentionally detached Unix sessions are outside the ownership set.
3. Tray failure means normal close quits; native warning dialogs explain tray/hide failures, and Settings provides independent Quit.
4. Node 24+ is the declared baseline; CI uses Node 24.
5. No component-test dependency was added. Real Chromium interactions with instrumented IPC exercised lifecycle, focus, keyboard, sound selection, and overflow; native interaction remains a separate release gate.
6. Signing/notarization, Linux/Windows execution, and human release approval remain outstanding.

## Parallelization Opportunities

- Tasks 1–2 must coordinate because both touch `pty_manager.rs`; Task 3 can proceed independently with an agreed daemon/CLI contract.
- Tasks 7–8 are sequential and share terminal lifecycle files. Task 9 may proceed independently after a recovery API decision.
- Task 13 is independent of the runtime fixes. Tasks 14–15 must coordinate keyboard/overlay ownership.
- Task 17 workflow preparation can run in parallel with feature work, but final release verification follows all fixes.
- Never parallelize edits to the same existing user-modified files without explicit ownership.

## Deferred Follow-up

R02–R07 in the audit remain follow-up recommendations: measured output batching, bounded status/audio history, asynchronous preference revisions, crash-durable saves/shutdown flush, and broader accessibility polish. They are not silently included in the 18-task scope. Signing/notarization is a separate decision and should become a separate plan if requested.

## Planning Verification

- [x] Findings and verification limitations recorded.
- [x] Every task has acceptance criteria, verification, dependencies, likely files, and scope.
- [x] No planned task exceeds approximately five source/test/config files.
- [x] Checkpoints occur after every three tasks.
- [x] Tasks recorded only in `tasks/todo.md`; plan remains an ordered overview.
- [x] Human authorized execution with “Execute the plan”; release readiness is not approved.

The supplied planning skill references a project-wide Definition of Done at `/Users/nemoryoliver/.agents/references/definition-of-done.md`; that file was unavailable. `CONTRIBUTING.md` and the repository checks serve as the standing verification bar for this plan.

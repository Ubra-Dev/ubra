# Headless daemon + CLI (experimental)

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

# Agent runtime daemon + GUI + CLI

`ubra-daemon` owns PTYs and the agent watcher, serving a JSON-lines protocol
over loopback TCP. By default Unix runtime files live in
`<tmp>/ubra-<effective-uid>/`; Windows uses the per-user app data directory.
`ubra-cli` drives it — all output is pretty-printed JSON, and the CLI starts the
daemon on demand. The desktop GUI is also a daemon client: it owns no PTYs
itself, so quitting the app detaches without stopping any agent (Herdr-style
persistence); only an explicit stop ends the agents.

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

Protocol 2
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

## GUI runtime

On startup the GUI probes the daemon port file and spawns a detached daemon
(own session/process group, stdio closed) when none answers, then connects
over the same authenticated protocol. A supervisor thread forwards `pty_*`
and `agent-state-update` events to the frontend and retries across
disconnects; every `pty_*` Tauri command is a daemon round trip.

- `Quit Ubra` detaches: the daemon keeps every agent running and reopening
  readopts the same sessions by pane key.
- `Stop Agents and Quit` (menus, tray) shuts the daemon down first. It
  never spawns a daemon to stop one.
- While disconnected the GUI shows a reconnecting banner; on reconnect it
  remounts every terminal (adopt live, else fresh spawn).
- GUI panes spawn keyed (`pane-<uuid>`) with the query responder off so
  xterm answers device queries alone. `pty_list`/`pty_snapshot` serve
  adoption; orphan sweeps still apply.

## Screen history

The daemon persists per-key screen snapshots to
`<app-data>/terminal-history/<key>.json` (0600/0700 on Unix, pruned past
30 days): every 30s, on every pane exit, and synchronously on shutdown.
`pty_history` serves the last snapshot for a key (null when absent).
Fresh mounts repaint it beneath the live screen so terminals feel
continuous across daemon restarts; corrupt, oversize, or foreign files
read as absent. `ubra-daemon --history-dir DIR` (`UBRA_HISTORY_DIR`)
overrides the location.

## Agent resume

After a daemon or machine restart the original processes are gone; panes
restore via agent-native resume (gated by the auto-launch setting; the
respawn button always resumes explicitly):

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

## Manual QA

1. Launch, start agents in two panes, Quit, relaunch: same live sessions.
2. `Stop Agents and Quit`: processes end (`ps` shows no agents).
3. Kill the daemon process: banner appears; GUI respawns it; panes show
   history prelude and resumed agents (claude/codex) or fresh shells.
4. `ubra-cli snapshot` during all of the above: consistent pane list.

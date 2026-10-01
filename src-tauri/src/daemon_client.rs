//! GUI-side daemon client: ensure a daemon answers, then drive it.
//!
//! The desktop app owns no PTYs itself: every `pty_*` command is forwarded
//! to the background daemon over its authenticated loopback protocol, and
//! pushed daemon events are demuxed from request responses on one long-lived
//! connection. When this process exits, the daemon (and its agents) survive;
//! quitting is detach, not shutdown.

use crate::daemon::{
    connect_authenticated, default_state_dir, open_private_file, read_frame, write_frame,
    MAX_FRAME_BYTES, PROTOCOL_VERSION,
};
use crate::pty_manager::{PaneId, PtySessionInfo, PtySnapshot};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::BufReader;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);
const ENSURE_TIMEOUT: Duration = Duration::from_secs(15);
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Daemon state dir, honoring `UBRA_STATE_DIR` for tests and scripted runs.
pub fn daemon_state_dir() -> PathBuf {
    if let Ok(path) = std::env::var("UBRA_STATE_DIR") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    default_state_dir()
}

/// Daemon binary: explicit override, sibling of this executable (dev target
/// dir and bundled sidecar layouts), else `PATH` lookup.
pub fn daemon_binary() -> PathBuf {
    if let Ok(path) = std::env::var("UBRA_DAEMON_BIN") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(format!("ubra-daemon{}", std::env::consts::EXE_SUFFIX));
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from("ubra-daemon")
}

/// Pushed daemon events the GUI bridge forwards to the frontend. Raw agent
/// payloads pass through untouched; the frontend already parses them.
#[derive(Debug, Clone)]
pub enum DaemonEvent {
    PtyOutput {
        id: PaneId,
        data: String,
        sequence: u64,
    },
    PtyExit {
        id: PaneId,
        success: bool,
        code: Option<i32>,
    },
    AgentUpdate {
        revision: u64,
        states: serde_json::Value,
        transitions: serde_json::Value,
    },
}

fn pane_id(value: &serde_json::Value) -> Option<PaneId> {
    value
        .as_u64()
        .and_then(|id| PaneId::try_from(id).ok())
        .filter(|id| *id != 0)
}

impl DaemonEvent {
    fn parse(value: &serde_json::Value) -> Option<Self> {
        match value.get("event").and_then(|e| e.as_str())? {
            "pty_output" => Some(DaemonEvent::PtyOutput {
                id: pane_id(value.get("pane")?)?,
                data: value.get("data")?.as_str()?.to_string(),
                sequence: value.get("sequence")?.as_u64()?,
            }),
            "pty_exit" => Some(DaemonEvent::PtyExit {
                id: pane_id(value.get("pane")?)?,
                success: value.get("success")?.as_bool()?,
                code: value.get("code").and_then(|c| c.as_i64()).map(|c| c as i32),
            }),
            "agent-state-update" => {
                let states = value.get("states")?.clone();
                let transitions = value.get("transitions")?.clone();
                if !states.is_object() || !transitions.is_array() {
                    return None;
                }
                Some(DaemonEvent::AgentUpdate {
                    revision: value.get("revision")?.as_u64()?,
                    states,
                    transitions,
                })
            }
            // `agent-states` (connect greeting) and future events are not
            // consumed by the bridge; the snapshot command covers the former.
            _ => None,
        }
    }
}

/// Authenticated daemon connection with one reader thread demuxing pushed
/// events (by `event`) from request responses (by `id`).
pub struct DaemonClient {
    writer: Mutex<TcpStream>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::SyncSender<serde_json::Value>>>,
    dead: AtomicBool,
}

impl DaemonClient {
    /// Authenticate and start the reader thread. Responses with an unknown
    /// or timed-out `id` are dropped; malformed frames are skipped.
    pub fn connect(
        state_dir: &Path,
        events: mpsc::SyncSender<DaemonEvent>,
    ) -> std::io::Result<Arc<Self>> {
        let stream = connect_authenticated(state_dir, CONNECT_TIMEOUT)?;
        let reader = BufReader::new(stream.try_clone()?);
        let client = Arc::new(Self {
            writer: Mutex::new(stream),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            dead: AtomicBool::new(false),
        });
        let pump = Arc::clone(&client);
        if let Err(e) = std::thread::Builder::new()
            .name("ubra-daemon-client".to_string())
            .spawn(move || pump.read_loop(reader, events))
        {
            client.note_dead();
            return Err(e);
        }
        Ok(client)
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Acquire)
    }

    fn note_dead(&self) {
        if self.dead.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = self.writer.lock().shutdown(std::net::Shutdown::Both);
        // Wake every waiter; dropping the senders unblocks `recv`.
        self.pending.lock().clear();
    }

    fn read_loop(&self, mut reader: BufReader<TcpStream>, events: mpsc::SyncSender<DaemonEvent>) {
        loop {
            match read_frame(&mut reader, Instant::now() + READ_IDLE_TIMEOUT) {
                Ok(line) => {
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                        continue;
                    };
                    if let Some(event) = DaemonEvent::parse(&value) {
                        // A lagging bridge drops rather than backpressuring
                        // the reader; the daemon disconnects slow consumers.
                        let _ = events.try_send(event);
                        continue;
                    }
                    let id = value.get("id").and_then(|id| id.as_u64());
                    let ok = value.get("ok").and_then(|ok| ok.as_bool());
                    if let (Some(id), Some(_)) = (id, ok) {
                        if let Some(tx) = self.pending.lock().remove(&id) {
                            let _ = tx.try_send(value);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_) => break,
            }
        }
        self.note_dead();
    }

    /// One request/response round trip. Daemon `{"ok": false}` replies map
    /// to `Err`; pushed events interleaved on the wire never surface here.
    pub fn request(
        &self,
        op: &str,
        mut body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        if self.is_dead() {
            return Err("daemon disconnected".to_string());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        body["op"] = op.into();
        body["id"] = id.into();
        let encoded = body.to_string();
        if encoded.len() + 1 > MAX_FRAME_BYTES {
            return Err("request exceeds frame byte limit".to_string());
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending.lock().insert(id, tx);
        let wrote = write_frame(
            &mut self.writer.lock(),
            &encoded,
            Instant::now() + WRITE_TIMEOUT,
        );
        if wrote.is_err() {
            self.pending.lock().remove(&id);
            self.note_dead();
            return Err("lost daemon connection".to_string());
        }
        match rx.recv_timeout(REQUEST_TIMEOUT) {
            Ok(value) => {
                if value.get("ok").and_then(|ok| ok.as_bool()) == Some(true) {
                    Ok(value)
                } else {
                    Err(value
                        .get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("daemon operation failed")
                        .to_string())
                }
            }
            Err(_) => {
                self.pending.lock().remove(&id);
                Err(format!("daemon {op} timed out"))
            }
        }
    }

    fn get(body: &serde_json::Value, key: &str) -> Result<serde_json::Value, String> {
        body.get(key)
            .cloned()
            .ok_or_else(|| format!("malformed daemon reply: missing \"{key}\""))
    }

    /// Ping and fail closed on protocol skew between GUI and daemon builds.
    pub fn ping(&self) -> Result<String, String> {
        let reply = self.request("ping", serde_json::json!({}))?;
        let protocol = Self::get(&reply, "protocol")?
            .as_u64()
            .ok_or("malformed daemon reply: \"protocol\" must be a number")?;
        if protocol != u64::from(PROTOCOL_VERSION) {
            return Err(format!(
                "daemon protocol mismatch (want {PROTOCOL_VERSION}, got {protocol}); restart the app to update"
            ));
        }
        Self::get(&reply, "version")?
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "malformed daemon reply: \"version\" must be a string".to_string())
    }

    /// Spawn a GUI-attached pane: keyed for reattach, with the terminal
    /// query responder disabled so xterm answers device queries alone.
    pub fn spawn(
        &self,
        shell: Option<String>,
        cwd: Option<String>,
        args: Vec<String>,
        cols: u16,
        rows: u16,
        key: Option<String>,
    ) -> Result<PaneId, String> {
        let reply = self.request(
            "pty_spawn",
            serde_json::json!({
                "shell": shell, "cwd": cwd, "args": args,
                "cols": cols, "rows": rows, "key": key, "headless": false,
            }),
        )?;
        pane_id(&Self::get(&reply, "pane")?).ok_or_else(|| "malformed pty_spawn reply".to_string())
    }

    pub fn write(&self, id: PaneId, data: &str) -> Result<(), String> {
        self.request("pty_write", serde_json::json!({"pane": id, "data": data}))?;
        Ok(())
    }

    pub fn resize(&self, id: PaneId, cols: u16, rows: u16) -> Result<(), String> {
        self.request(
            "pty_resize",
            serde_json::json!({"pane": id, "cols": cols, "rows": rows}),
        )?;
        Ok(())
    }

    pub fn kill(&self, id: PaneId) -> Result<(), String> {
        self.request("pty_kill", serde_json::json!({"pane": id}))?;
        Ok(())
    }

    pub fn snapshot(&self, id: PaneId) -> Result<PtySnapshot, String> {
        let reply = self.request("pty_snapshot", serde_json::json!({"pane": id}))?;
        serde_json::from_value(Self::get(&reply, "snapshot")?)
            .map_err(|e| format!("malformed pty_snapshot reply: {e}"))
    }

    pub fn list(&self) -> Result<Vec<PtySessionInfo>, String> {
        let reply = self.request("pty_list", serde_json::json!({}))?;
        serde_json::from_value(Self::get(&reply, "sessions")?)
            .map_err(|e| format!("malformed pty_list reply: {e}"))
    }

    /// Last saved screen for `key`, if any. Absent history is normal (new
    /// panes, disabled persistence) and reads as `None`, never an error.
    pub fn history(&self, key: &str) -> Result<Option<PtySnapshot>, String> {
        let reply = self.request("pty_history", serde_json::json!({"key": key}))?;
        let history = Self::get(&reply, "history")?;
        if history.is_null() {
            return Ok(None);
        }
        serde_json::from_value(history)
            .map(Some)
            .map_err(|e| format!("malformed pty_history reply: {e}"))
    }

    /// Agent states snapshot; the bridge adapts it to the frontend update.
    pub fn agent_states(&self) -> Result<serde_json::Value, String> {
        let reply = self.request("agent_states", serde_json::json!({}))?;
        let states = Self::get(&reply, "states")?;
        let revision = Self::get(&reply, "revision")?;
        if !states.is_object() || revision.as_u64().is_none() {
            return Err("malformed agent_states reply".to_string());
        }
        Ok(serde_json::json!({"revision": revision, "states": states}))
    }

    /// Ask the daemon to stop all panes and exit. Best-effort from the
    /// GUI's explicit "stop agents" path only.
    pub fn shutdown_daemon(&self) -> Result<(), String> {
        self.request("shutdown", serde_json::json!({}))?;
        Ok(())
    }
}

/// Connect, spawning a detached daemon first when none answers. The daemon
/// runs in its own session (Unix) or process group (Windows) with stdio
/// detached, so it survives this process and terminal signals.
pub fn connect_or_ensure(
    state_dir: &Path,
    rules_dir: Option<&Path>,
    events: mpsc::SyncSender<DaemonEvent>,
) -> Result<Arc<DaemonClient>, String> {
    if let Ok(client) = DaemonClient::connect(state_dir, events.clone()) {
        return Ok(client);
    }
    ensure_daemon(state_dir, rules_dir)?;
    DaemonClient::connect(state_dir, events).map_err(|e| format!("daemon did not answer: {e}"))
}

fn ensure_daemon(state_dir: &Path, rules_dir: Option<&Path>) -> Result<(), String> {
    if connect_authenticated(state_dir, PROBE_TIMEOUT).is_ok() {
        return Ok(());
    }
    spawn_daemon(state_dir, rules_dir)?;
    let deadline = Instant::now() + ENSURE_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(format!(
                "ubra-daemon did not answer (state dir {}; log at daemon.log)",
                state_dir.display()
            ));
        }
        if connect_authenticated(state_dir, remaining.min(Duration::from_millis(250))).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn spawn_daemon(state_dir: &Path, rules_dir: Option<&Path>) -> Result<(), String> {
    let log = open_private_file(&state_dir.join("daemon.log"), true, true)
        .map_err(|e| format!("cannot open private daemon log: {e}"))?;
    let mut command = Command::new(daemon_binary());
    command
        .arg("--state-dir")
        .arg(state_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    if let Some(rules) = rules_dir {
        command.arg("--rules-dir").arg(rules);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start ubra-daemon: {e}"))?;
    // Reap the exit status without supervising: a detached daemon that is
    // still alive when we exit is reparented and keeps running.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::{new_auth_token, serve, write_auth_token, write_port_file, DaemonCore};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ubra-daemon-client-test-{}-{id}",
            std::process::id()
        ))
    }

    /// A live daemon on a scratch state dir, served on a background thread.
    fn live_daemon() -> PathBuf {
        let dir = scratch_dir();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = new_auth_token().unwrap();
        write_port_file(&dir, port).unwrap();
        write_auth_token(&dir, &token).unwrap();
        let core = DaemonCore::new(None, dir.clone());
        core.set_history_dir(dir.join("history"));
        std::thread::spawn(move || serve(core, listener, token));
        dir
    }

    fn client_for(dir: &Path) -> (Arc<DaemonClient>, mpsc::Receiver<DaemonEvent>) {
        let (tx, rx) = mpsc::sync_channel(256);
        let client = DaemonClient::connect(dir, tx).expect("client connects");
        (client, rx)
    }

    fn next_output(
        client: &DaemonClient,
        rx: &mpsc::Receiver<DaemonEvent>,
        pane: PaneId,
        needle: &str,
    ) -> (PaneId, u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut transcript = String::new();
        let mut handshake_answered = false;
        loop {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(DaemonEvent::PtyOutput { id, data, sequence }) => {
                    // ConPTY startup query (Windows only): answer it or the
                    // pane withholds all further output. In production xterm.js
                    // answers; here the test plays the terminal itself.
                    // Silent no-op on Unix, where the query never arrives.
                    transcript.push_str(&data);
                    if !handshake_answered && transcript.contains("\u{1b}[6n") {
                        handshake_answered = true;
                        let _ = client.write(pane, "\u{1b}[1;1R");
                    }
                    if data.contains(needle) {
                        return (id, sequence);
                    }
                }
                Ok(_) => continue,
                Err(_) => panic!("timed out waiting for output containing {needle:?}"),
            }
        }
    }

    fn next_exit(rx: &mpsc::Receiver<DaemonEvent>, pane: PaneId) -> (bool, Option<i32>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(DaemonEvent::PtyExit { id, success, code }) if id == pane => {
                    return (success, code);
                }
                Ok(_) => continue,
                Err(_) => panic!("timed out waiting for exit of pane {pane}"),
            }
        }
    }

    #[test]
    fn ping_reports_version_and_protocol() {
        let dir = live_daemon();
        let (client, _rx) = client_for(&dir);
        let version = client.ping().unwrap();
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keyed_spawn_lists_snapshots_streams_and_kills() {
        let dir = live_daemon();
        let (client, rx) = client_for(&dir);
        let pane = client
            .spawn(
                None,
                None,
                Vec::new(),
                80,
                24,
                Some("pane-client-1".to_string()),
            )
            .unwrap();
        assert!(pane != 0);

        let sessions = client.list().unwrap();
        assert_eq!(
            sessions,
            vec![PtySessionInfo {
                id: pane,
                key: Some("pane-client-1".to_string()),
            }]
        );

        client.write(pane, "echo marker-client-7\n").unwrap();
        let (got, _) = next_output(&client, &rx, pane, "marker-client-7");
        assert_eq!(got, pane);

        let snap = client.snapshot(pane).unwrap();
        assert_eq!((snap.cols, snap.rows), (80, 24));
        assert!(
            snap.data.contains("marker-client-7"),
            "got: {:?}",
            snap.data
        );

        let states = client.agent_states().unwrap();
        assert!(states["revision"].as_u64().is_some());
        assert!(states["states"].is_object());

        client.kill(pane).unwrap();
        next_exit(&rx, pane);
        assert!(client.list().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_survives_pane_death() {
        let dir = live_daemon();
        let (client, rx) = client_for(&dir);
        assert!(client.history("pane-nope").unwrap().is_none());
        let pane = client
            .spawn(
                None,
                None,
                Vec::new(),
                80,
                24,
                Some("pane-hist-1".to_string()),
            )
            .unwrap();
        client.write(pane, "echo marker-hist-client\n").unwrap();
        next_output(&client, &rx, pane, "marker-hist-client");
        client.kill(pane).unwrap();
        next_exit(&rx, pane);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match client.history("pane-hist-1").unwrap() {
                Some(snap) if snap.data.contains("marker-hist-client") => break,
                _ if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                other => panic!("history never landed: {other:?}"),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dead_pane_operations_error() {
        let dir = live_daemon();
        let (client, _rx) = client_for(&dir);
        assert!(client.snapshot(424242).is_err());
        assert!(client.kill(424242).is_err());
        assert!(client.write(424242, "x").is_err());
        assert!(client.resize(424242, 80, 24).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn event_parse_accepts_known_shapes_and_ignores_the_rest() {
        let output = serde_json::json!({"event":"pty_output","pane":3,"data":"hi","sequence":9});
        assert!(matches!(
            DaemonEvent::parse(&output),
            Some(DaemonEvent::PtyOutput { id: 3, .. })
        ));
        let exit = serde_json::json!({"event":"pty_exit","pane":3,"success":true,"code":0});
        assert!(matches!(
            DaemonEvent::parse(&exit),
            Some(DaemonEvent::PtyExit {
                id: 3,
                success: true,
                code: Some(0)
            })
        ));
        let update = serde_json::json!({
            "event":"agent-state-update","revision":1,"states":{},"transitions":[]
        });
        assert!(matches!(
            DaemonEvent::parse(&update),
            Some(DaemonEvent::AgentUpdate { revision: 1, .. })
        ));
        // Greeting, future events, and malformed shapes never reach the bridge.
        for raw in [
            serde_json::json!({"event":"agent-states","states":{}}),
            serde_json::json!({"event":"pty_output","pane":0,"data":"x","sequence":1}),
            serde_json::json!({"event":"pty_output","pane":3,"sequence":1}),
            serde_json::json!({"event":"pty_exit","pane":3}),
            serde_json::json!({"event":"agent-state-update","revision":1,"states":[]}),
            serde_json::json!({"ok": true}),
            serde_json::json!({"event":42}),
        ] {
            assert!(DaemonEvent::parse(&raw).is_none(), "got: {raw}");
        }
    }
}

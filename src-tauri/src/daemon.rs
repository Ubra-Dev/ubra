//! Headless agent-runtime daemon (Phase 5a): owns PTYs and the agent
//! watcher, serving a JSON-lines protocol over loopback TCP.
//!
//! Wire format: one JSON object per line, both directions. Requests carry
//! `op` plus arguments; responses carry `ok` (echoing `id` when present).
//! The daemon also pushes `event` objects (pty output/exit, agent-state
//! changes) to every connection; clients filter what they need.
//!
//! ```text
//! -> {"op":"pty_spawn","cols":80,"rows":24}
//! <- {"event":"agent-states","states":{}}
//! <- {"ok":true,"pane":1}
//! <- {"event":"pty_output","pane":1,"data":"$ "}
//! ```
//!
//! SECURITY: no authentication yet — any local process can drive panes.
//! The socket binds 127.0.0.1 only (never LAN-reachable); a token file is
//! planned before the GUI migrates onto this protocol.

use crate::agent_watch::Watcher;
use crate::pty_manager::{PaneId, PtyEventSink, PtyManager};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const PROTOCOL_VERSION: u32 = 1;
pub const AGENT_POLL_SECS: u64 = 2;
pub const PORT_FILE: &str = "daemon.json";
pub const APP_IDENTIFIER: &str = "com.nemoryoliver.ubra";

/// Request envelope: flat object, `op` plus per-op arguments.
#[derive(Debug, serde::Deserialize)]
pub struct Request {
    pub op: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub pane: Option<PaneId>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    #[serde(default)]
    pub keys: Option<Vec<String>>,
    #[serde(default)]
    pub want: Option<Vec<String>>,
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

fn respond(id: Option<u64>, mut value: serde_json::Value) -> String {
    if let Some(id) = id {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("id".to_string(), id.into());
        }
    }
    value.to_string()
}

fn ok(id: Option<u64>, value: serde_json::Value) -> String {
    respond(id, value)
}

fn err(id: Option<u64>, msg: impl std::fmt::Display) -> String {
    respond(
        id,
        serde_json::json!({"ok": false, "error": msg.to_string()}),
    )
}

/// Runtime state dir: `--state-dir` or `<tmp>/ubra-<user>/`.
pub fn default_state_dir() -> PathBuf {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string());
    std::env::temp_dir().join(format!("ubra-{user}"))
}

/// Contents of `<state-dir>/daemon.json`: where the daemon listens.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PortFile {
    pub port: u16,
    pub pid: u32,
}

pub fn read_port_file(dir: &Path) -> Option<PortFile> {
    let text = std::fs::read_to_string(dir.join(PORT_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write_port_file(dir: &Path, port: u16) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        dir.join(PORT_FILE),
        serde_json::json!({"port": port, "pid": std::process::id()}).to_string(),
    )
}

/// App data dir, mirroring Tauri's `appDataDir` for our identifier so the
/// daemon and GUI share paths (`<platform-data-dir>/<identifier>`).
pub fn app_data_dir() -> PathBuf {
    app_data_dir_with(
        &home_dir(),
        std::env::var("XDG_DATA_HOME").ok(),
        std::env::var("APPDATA").ok(),
    )
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn app_data_dir_with(
    home: &Path,
    xdg_data_home: Option<String>,
    appdata: Option<String>,
) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let _ = (xdg_data_home, appdata);
        home.join("Library")
            .join("Application Support")
            .join(APP_IDENTIFIER)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = xdg_data_home;
        appdata
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
            .join(APP_IDENTIFIER)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = appdata;
        xdg_data_home
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join(APP_IDENTIFIER)
    }
}

/// Daemon state shared by connection threads and the agent poll thread.
pub struct DaemonCore {
    manager: PtyManager,
    watcher: Mutex<Watcher>,
    peers: Arc<Mutex<HashMap<u64, mpsc::Sender<String>>>>,
    next_peer: AtomicU64,
    last_states: Mutex<String>,
    state_dir: PathBuf,
}

impl DaemonCore {
    pub fn new(rules_dir: Option<PathBuf>, state_dir: PathBuf) -> Arc<Self> {
        let peers = Arc::new(Mutex::new(HashMap::new()));
        let manager = PtyManager::new(Arc::new(DaemonSink {
            peers: peers.clone(),
        }));
        let watcher = match rules_dir {
            Some(dir) => Watcher::with_dir(dir),
            None => Watcher::bundled(),
        };
        Arc::new(Self {
            manager,
            watcher: Mutex::new(watcher),
            peers,
            next_peer: AtomicU64::new(1),
            last_states: Mutex::new(String::new()),
            state_dir,
        })
    }

    fn broadcast(&self, msg: String) {
        let mut peers = self.peers.lock().unwrap();
        let mut dead = Vec::new();
        for (id, tx) in peers.iter() {
            if tx.send(msg.clone()).is_err() {
                dead.push(*id);
            }
        }
        for id in dead {
            peers.remove(&id);
        }
    }
}

struct DaemonSink {
    peers: Arc<Mutex<HashMap<u64, mpsc::Sender<String>>>>,
}

impl PtyEventSink for DaemonSink {
    fn output(&self, id: PaneId, data: String) {
        let msg = serde_json::json!({"event": "pty_output", "pane": id, "data": data}).to_string();
        let mut peers = self.peers.lock().unwrap();
        let mut dead = Vec::new();
        for (peer, tx) in peers.iter() {
            if tx.send(msg.clone()).is_err() {
                dead.push(*peer);
            }
        }
        for peer in dead {
            peers.remove(&peer);
        }
    }

    fn exited(&self, id: PaneId, success: bool, code: Option<i32>) {
        let msg =
            serde_json::json!({"event": "pty_exit", "pane": id, "success": success, "code": code})
                .to_string();
        let mut peers = self.peers.lock().unwrap();
        let mut dead = Vec::new();
        for (peer, tx) in peers.iter() {
            if tx.send(msg.clone()).is_err() {
                dead.push(*peer);
            }
        }
        for peer in dead {
            peers.remove(&peer);
        }
    }
}

pub(crate) enum Action {
    Respond(String),
    Shutdown(String),
}

fn need_pane(req: &Request) -> Result<PaneId, String> {
    req.pane.ok_or_else(|| "missing \"pane\"".to_string())
}

/// Encode one `send-keys` key: single characters pass through literally,
/// anything longer must be a known key name.
fn key_to_bytes(key: &str) -> Result<String, String> {
    if key.chars().count() == 1 {
        return Ok(key.to_string());
    }
    let named = match key.to_lowercase().as_str() {
        "enter" => "\r",
        "escape" | "esc" => "\u{1b}",
        "tab" => "\t",
        "backspace" => "\u{7f}",
        "space" => " ",
        "up" => "\u{1b}[A",
        "down" => "\u{1b}[B",
        "right" => "\u{1b}[C",
        "left" => "\u{1b}[D",
        "home" => "\u{1b}[H",
        "end" => "\u{1b}[F",
        "pageup" => "\u{1b}[5~",
        "pagedown" => "\u{1b}[6~",
        "f1" => "\u{1b}OP",
        "f2" => "\u{1b}OQ",
        "f3" => "\u{1b}OR",
        "f4" => "\u{1b}OS",
        "f5" => "\u{1b}[15~",
        "f6" => "\u{1b}[17~",
        "f7" => "\u{1b}[18~",
        "f8" => "\u{1b}[19~",
        "f9" => "\u{1b}[20~",
        "f10" => "\u{1b}[21~",
        "f11" => "\u{1b}[23~",
        "f12" => "\u{1b}[24~",
        name => {
            if let Some(letter) = name.strip_prefix("ctrl-") {
                let mut chars = letter.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    if c.is_ascii_alphabetic() {
                        let byte = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                        return Ok((byte as char).to_string());
                    }
                }
            }
            return Err(format!("unknown key: {key}"));
        }
    };
    Ok(named.to_string())
}

fn pane_alive(core: &DaemonCore, pane: PaneId) -> bool {
    core.manager.pane_roots().iter().any(|p| p.id == pane)
}

fn wait_timeout(req: &Request) -> Duration {
    Duration::from_secs(req.timeout_secs.unwrap_or(30).clamp(1, 600))
}

fn state_key(state: &crate::agent_watch::PaneAgent) -> &'static str {
    match state {
        crate::agent_watch::PaneAgent::Working { .. } => "working",
        crate::agent_watch::PaneAgent::Blocked { .. } => "blocked",
        crate::agent_watch::PaneAgent::Unknown { .. } => "unknown",
        crate::agent_watch::PaneAgent::Done { .. } => "done",
        crate::agent_watch::PaneAgent::Idle => "idle",
    }
}

fn dispatch(core: &DaemonCore, req: &Request) -> Action {
    let id = req.id;
    let respond = |value: serde_json::Value| Action::Respond(ok(id, value));
    match req.op.as_str() {
        "ping" => respond(serde_json::json!({
            "ok": true,
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": PROTOCOL_VERSION,
        })),
        "pty_spawn" => match core.manager.spawn(
            req.shell.clone(),
            req.cwd.clone(),
            req.args.clone().unwrap_or_default(),
            req.cols.unwrap_or(80),
            req.rows.unwrap_or(24),
        ) {
            Ok(pane) => respond(serde_json::json!({"ok": true, "pane": pane})),
            Err(e) => Action::Respond(err(id, e)),
        },
        "pty_write" => {
            let (Some(pane), Some(data)) = (req.pane, req.data.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"data\""));
            };
            match core.manager.write(pane, &data) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            }
        }
        "pty_resize" => match (req.pane, req.cols, req.rows) {
            (Some(pane), Some(cols), Some(rows)) => match core.manager.resize(pane, cols, rows) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            },
            _ => Action::Respond(err(id, "missing \"pane\", \"cols\" or \"rows\"")),
        },
        "pty_kill" => match need_pane(req)
            .and_then(|pane| core.manager.kill(pane).map_err(|e| e.to_string()))
        {
            Ok(()) => respond(serde_json::json!({"ok": true})),
            Err(e) => Action::Respond(err(id, e)),
        },
        "pty_read" => match need_pane(req) {
            Ok(pane) => match core.manager.screen_text(pane) {
                Some(screen) => respond(serde_json::json!({"ok": true, "screen": screen})),
                None => Action::Respond(err(id, format!("no such pane: {pane}"))),
            },
            Err(e) => Action::Respond(err(id, e)),
        },
        "panes" => respond(serde_json::json!({"ok": true, "panes": core.manager.pane_roots()})),
        "agent_states" => {
            let states = core.watcher.lock().unwrap().poll(&core.manager);
            respond(serde_json::json!({"ok": true, "states": states}))
        }
        "rules_reload" => {
            core.watcher.lock().unwrap().reload_rules();
            respond(serde_json::json!({"ok": true}))
        }
        "snapshot" => {
            let states: BTreeMap<PaneId, crate::agent_watch::PaneAgent> =
                core.watcher.lock().unwrap().poll(&core.manager);
            respond(serde_json::json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "protocol": PROTOCOL_VERSION,
                "panes": core.manager.pane_roots(),
                "states": states,
            }))
        }
        "pty_send_text" => {
            let (Some(pane), Some(data)) = (req.pane, req.data.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"data\""));
            };
            // Bracketed paste: the app treats this as one pasted input.
            let pasted = format!("\u{1b}[200~{data}\u{1b}[201~");
            match core.manager.write(pane, &pasted) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            }
        }
        "pty_send_keys" => {
            let (Some(pane), Some(keys)) = (req.pane, req.keys.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"keys\""));
            };
            let mut bytes = String::new();
            for key in &keys {
                match key_to_bytes(key) {
                    Ok(seq) => bytes.push_str(&seq),
                    Err(e) => return Action::Respond(err(id, e)),
                }
            }
            match core.manager.write(pane, &bytes) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            }
        }
        "agent_prompt" => {
            let (Some(pane), Some(data)) = (req.pane, req.data.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"data\""));
            };
            let submitted = format!("\u{1b}[200~{data}\u{1b}[201~\r");
            match core.manager.write(pane, &submitted) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            }
        }
        "wait_state" => {
            let (Some(pane), Some(want)) = (req.pane, req.want.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"want\""));
            };
            if want.is_empty() {
                return Action::Respond(err(id, "\"want\" must not be empty"));
            }
            if !pane_alive(core, pane) {
                return Action::Respond(err(id, format!("no such pane: {pane}")));
            }
            let deadline = std::time::Instant::now() + wait_timeout(req);
            loop {
                if !pane_alive(core, pane) {
                    return Action::Respond(err(id, format!("pane exited: {pane}")));
                }
                let states = core.watcher.lock().unwrap().poll(&core.manager);
                if let Some(state) = states.get(&pane) {
                    if want.iter().any(|w| w == state_key(state)) {
                        let (agent, cli) = match state {
                            crate::agent_watch::PaneAgent::Working { agent, cli, .. }
                            | crate::agent_watch::PaneAgent::Blocked { agent, cli, .. }
                            | crate::agent_watch::PaneAgent::Unknown { agent, cli, .. }
                            | crate::agent_watch::PaneAgent::Done { agent, cli, .. } => {
                                (Some(agent), Some(cli))
                            }
                            crate::agent_watch::PaneAgent::Idle => (None, None),
                        };
                        return respond(serde_json::json!({
                            "ok": true, "pane": pane, "state": state_key(state),
                            "agent": agent, "cli": cli,
                        }));
                    }
                }
                if std::time::Instant::now() >= deadline {
                    return Action::Respond(err(id, "timed out waiting for state"));
                }
                thread::sleep(Duration::from_millis(200));
            }
        }
        "wait_output" => {
            let (Some(pane), Some(needle)) = (req.pane, req.contains.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"contains\""));
            };
            let deadline = std::time::Instant::now() + wait_timeout(req);
            loop {
                match core.manager.screen_text(pane) {
                    Some(screen) if screen.contains(&needle) => {
                        return respond(serde_json::json!({"ok": true, "pane": pane}));
                    }
                    Some(_) => {}
                    None => return Action::Respond(err(id, format!("no such pane: {pane}"))),
                }
                if std::time::Instant::now() >= deadline {
                    return Action::Respond(err(id, "timed out waiting for output"));
                }
                thread::sleep(Duration::from_millis(200));
            }
        }
        "shutdown" => Action::Shutdown(ok(id, serde_json::json!({"ok": true}))),
        op => Action::Respond(err(id, format!("unknown op: {op}"))),
    }
}

/// Serve forever: agent poll thread plus a thread per connection.
pub fn serve(core: Arc<DaemonCore>, listener: TcpListener) {
    let poll_core = core.clone();
    thread::Builder::new()
        .name("ubra-daemon-agent-watch".to_string())
        .spawn(move || loop {
            thread::sleep(Duration::from_secs(AGENT_POLL_SECS));
            let states = poll_core.watcher.lock().unwrap().poll(&poll_core.manager);
            let json = serde_json::to_string(&states).unwrap_or_default();
            let mut last = poll_core.last_states.lock().unwrap();
            if json != *last {
                *last = json;
                let msg =
                    serde_json::json!({"event": "agent-states", "states": states}).to_string();
                drop(last);
                poll_core.broadcast(msg);
            }
        })
        .expect("spawn agent poll thread");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let core = core.clone();
                let id = core.next_peer.fetch_add(1, Ordering::Relaxed);
                thread::Builder::new()
                    .name(format!("ubra-daemon-conn-{id}"))
                    .spawn(move || handle_conn(core, stream, id))
                    .ok();
            }
            Err(e) => eprintln!("ubra-daemon: accept failed: {e}"),
        }
    }
}

fn handle_conn(core: Arc<DaemonCore>, stream: TcpStream, id: u64) {
    let (tx, rx) = mpsc::channel::<String>();
    core.peers.lock().unwrap().insert(id, tx.clone());
    // Greet with the current agent snapshot so watchers need no round trip.
    {
        let states = core.watcher.lock().unwrap().poll(&core.manager);
        let _ = tx.send(serde_json::json!({"event": "agent-states", "states": states}).to_string());
    }
    let writer_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => {
            core.peers.lock().unwrap().remove(&id);
            return;
        }
    };
    thread::spawn(move || {
        let mut writer = writer_stream;
        for msg in rx {
            if writeln!(writer, "{msg}").is_err() || writer.flush().is_err() {
                break;
            }
        }
    });
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let _ = tx.send(
                    serde_json::json!({"ok": false, "error": format!("invalid request: {e}")})
                        .to_string(),
                );
                continue;
            }
        };
        match dispatch(&core, &req) {
            Action::Respond(msg) => {
                if tx.send(msg).is_err() {
                    break;
                }
            }
            Action::Shutdown(msg) => {
                let _ = tx.send(msg);
                // Let the writer flush the goodbye before exiting.
                thread::sleep(Duration::from_millis(200));
                shutdown_now(&core);
            }
        }
    }
    core.peers.lock().unwrap().remove(&id);
}

fn shutdown_now(core: &DaemonCore) -> ! {
    for pane in core.manager.pane_roots() {
        let _ = core.manager.kill(pane.id);
    }
    let _ = std::fs::remove_file(core.state_dir.join(PORT_FILE));
    std::process::exit(0);
}

/// Daemon CLI arguments (thin bin parses into this; kept here for tests).
#[derive(Debug, PartialEq, Eq)]
pub struct DaemonArgs {
    pub state_dir: Option<PathBuf>,
    pub rules_dir: Option<PathBuf>,
    pub port: u16,
}

pub fn parse_daemon_args(args: &[String]) -> Result<DaemonArgs, String> {
    let mut out = DaemonArgs {
        state_dir: None,
        rules_dir: None,
        port: 0,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--state-dir" => {
                i += 1;
                out.state_dir = Some(PathBuf::from(
                    args.get(i).ok_or("missing value for --state-dir")?,
                ));
            }
            "--rules-dir" => {
                i += 1;
                out.rules_dir = Some(PathBuf::from(
                    args.get(i).ok_or("missing value for --rules-dir")?,
                ));
            }
            "--port" => {
                i += 1;
                out.port = args
                    .get(i)
                    .ok_or("missing value for --port")?
                    .parse()
                    .map_err(|_| "invalid --port (want 0-65535)")?;
            }
            "--help" | "-h" => return Err("help".to_string()),
            flag => return Err(format!("unknown flag: {flag}")),
        }
        i += 1;
    }
    Ok(out)
}

pub const DAEMON_USAGE: &str =
    "usage: ubra-daemon [--state-dir DIR] [--rules-dir DIR] [--port PORT]";

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ubra-daemon-test-{}-{id}", std::process::id()))
    }

    fn core() -> Arc<DaemonCore> {
        DaemonCore::new(None, scratch_dir())
    }

    fn req(op: &str) -> Request {
        serde_json::from_str(&format!("{{\"op\":\"{op}\"}}")).unwrap()
    }

    fn responded(action: Action) -> serde_json::Value {
        match action {
            Action::Respond(line) => serde_json::from_str(&line).unwrap(),
            Action::Shutdown(_) => panic!("unexpected shutdown"),
        }
    }

    #[test]
    fn request_defaults_are_empty() {
        let req: Request = serde_json::from_str("{\"op\":\"ping\"}").unwrap();
        assert_eq!(req.op, "ping");
        assert_eq!(req.id, None);
        assert_eq!(req.pane, None);
        assert_eq!(req.cols, None);
    }

    #[test]
    fn ping_echoes_id() {
        let req: Request = serde_json::from_str("{\"op\":\"ping\",\"id\":7}").unwrap();
        let v = responded(dispatch(&core(), &req));
        assert_eq!(v["ok"], true);
        assert_eq!(v["id"], 7);
        assert_eq!(v["protocol"], PROTOCOL_VERSION);
    }

    #[test]
    fn unknown_op_errors() {
        let v = responded(dispatch(&core(), &req("frobnicate")));
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("frobnicate"));
    }

    #[test]
    fn missing_args_error() {
        let v = responded(dispatch(&core(), &req("pty_write")));
        assert_eq!(v["ok"], false);
        let v = responded(dispatch(&core(), &req("pty_read")));
        assert_eq!(v["ok"], false);
        let v = responded(dispatch(&core(), &req("pty_kill")));
        assert_eq!(v["ok"], false);
    }

    #[test]
    fn spawn_write_read_kill_roundtrip() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        assert_eq!(spawn["ok"], true);
        let pane = spawn["pane"].as_u64().unwrap() as PaneId;

        let mut write = req("pty_write");
        write.pane = Some(pane);
        write.data = Some("echo marker-abc123\n".to_string());
        assert_eq!(responded(dispatch(&core, &write))["ok"], true);

        let mut read = req("pty_read");
        read.pane = Some(pane);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let screen = responded(dispatch(&core, &read))["screen"]
                .as_str()
                .unwrap()
                .to_string();
            if screen.contains("marker-abc123") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "screen never showed output: {screen:?}"
            );
            thread::sleep(Duration::from_millis(100));
        }

        let mut kill = req("pty_kill");
        kill.pane = Some(pane);
        assert_eq!(responded(dispatch(&core, &kill))["ok"], true);
        assert_eq!(responded(dispatch(&core, &read))["ok"], false);
    }

    #[test]
    fn panes_and_snapshot_list_spawned_panes() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        let pane = spawn["pane"].as_u64().unwrap();
        let panes = responded(dispatch(&core, &req("panes")));
        assert!(panes["panes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == pane));
        let snap = responded(dispatch(&core, &req("snapshot")));
        assert_eq!(snap["ok"], true);
        assert!(snap["panes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == pane));
        assert!(snap.get("states").is_some());
    }

    #[test]
    fn shutdown_action_carries_ok() {
        match dispatch(&core(), &req("shutdown")) {
            Action::Shutdown(line) => {
                let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(v["ok"], true);
            }
            Action::Respond(_) => panic!("shutdown must not merely respond"),
        }
    }

    #[test]
    fn port_file_roundtrip() {
        let dir = scratch_dir();
        assert_eq!(read_port_file(&dir), None);
        write_port_file(&dir, 4567).unwrap();
        assert_eq!(
            read_port_file(&dir),
            Some(PortFile {
                port: 4567,
                pid: std::process::id(),
            })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_data_dir_uses_identifier() {
        let home = Path::new("/home/test");
        let dir = app_data_dir_with(home, None, None);
        assert!(dir.ends_with(APP_IDENTIFIER), "got: {}", dir.display());
        assert!(dir.starts_with(home), "got: {}", dir.display());
        let xdg = app_data_dir_with(home, Some("/xdg/data".to_string()), None);
        assert!(xdg.ends_with(APP_IDENTIFIER), "got: {}", xdg.display());
    }

    #[test]
    fn parse_daemon_args_flags() {
        let args: Vec<String> = [
            "--state-dir",
            "/tmp/s",
            "--rules-dir",
            "/tmp/r",
            "--port",
            "1234",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            parse_daemon_args(&args).unwrap(),
            DaemonArgs {
                state_dir: Some(PathBuf::from("/tmp/s")),
                rules_dir: Some(PathBuf::from("/tmp/r")),
                port: 1234,
            }
        );
        assert_eq!(
            parse_daemon_args(&[]).unwrap(),
            DaemonArgs {
                state_dir: None,
                rules_dir: None,
                port: 0,
            }
        );
        assert!(parse_daemon_args(&["--port".to_string()]).is_err());
        assert!(parse_daemon_args(&["--port".to_string(), "x".to_string()]).is_err());
        assert!(parse_daemon_args(&["--bogus".to_string()]).is_err());
    }

    #[test]
    fn key_encoding_covers_names_ctrl_and_literals() {
        assert_eq!(key_to_bytes("enter").unwrap(), "\r");
        assert_eq!(key_to_bytes("Enter").unwrap(), "\r");
        assert_eq!(key_to_bytes("esc").unwrap(), "\u{1b}");
        assert_eq!(key_to_bytes("tab").unwrap(), "\t");
        assert_eq!(key_to_bytes("up").unwrap(), "\u{1b}[A");
        assert_eq!(key_to_bytes("f5").unwrap(), "\u{1b}[15~");
        assert_eq!(key_to_bytes("ctrl-c").unwrap(), "\u{3}");
        assert_eq!(key_to_bytes("ctrl-Z").unwrap(), "\u{1a}");
        assert_eq!(key_to_bytes("a").unwrap(), "a");
        assert_eq!(key_to_bytes("?").unwrap(), "?");
        assert!(key_to_bytes("f13").is_err());
        assert!(key_to_bytes("ctrl-").is_err());
        assert!(key_to_bytes("ctrl-ab").is_err());
        assert!(key_to_bytes("splat").is_err());
    }

    /// Bracketed-paste markers need a terminal that implements them.
    #[cfg(unix)]
    #[test]
    fn prompt_submits_and_output_arrives() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        let pane = spawn["pane"].as_u64().unwrap() as PaneId;

        let mut prompt: Request =
            serde_json::from_str("{\"op\":\"agent_prompt\",\"data\":\"echo marker-p6a\"}").unwrap();
        prompt.pane = Some(pane);
        assert_eq!(responded(dispatch(&core, &prompt))["ok"], true);

        let mut wait: Request =
            serde_json::from_str("{\"op\":\"wait_output\",\"contains\":\"marker-p6a\"}").unwrap();
        wait.pane = Some(pane);
        wait.timeout_secs = Some(10);
        let got = responded(dispatch(&core, &wait));
        assert_eq!(got["ok"], true);
        assert_eq!(got["pane"], pane);
    }

    #[test]
    fn send_keys_submits_pending_line() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        let pane = spawn["pane"].as_u64().unwrap() as PaneId;

        let mut write: Request =
            serde_json::from_str("{\"op\":\"pty_write\",\"data\":\"echo marker-p6b\"}").unwrap();
        write.pane = Some(pane);
        assert_eq!(responded(dispatch(&core, &write))["ok"], true);

        let mut keys: Request =
            serde_json::from_str("{\"op\":\"pty_send_keys\",\"keys\":[\"enter\"]}").unwrap();
        keys.pane = Some(pane);
        assert_eq!(responded(dispatch(&core, &keys))["ok"], true);

        let mut wait: Request =
            serde_json::from_str("{\"op\":\"wait_output\",\"contains\":\"marker-p6b\"}").unwrap();
        wait.pane = Some(pane);
        wait.timeout_secs = Some(10);
        assert_eq!(responded(dispatch(&core, &wait))["ok"], true);
    }

    #[test]
    fn send_keys_rejects_unknown_keys() {
        let core = core();
        let mut keys: Request =
            serde_json::from_str("{\"op\":\"pty_send_keys\",\"keys\":[\"splat\"]}").unwrap();
        keys.pane = Some(1);
        let v = responded(dispatch(&core, &keys));
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("splat"));
    }

    #[test]
    fn wait_state_matches_idle_and_times_out() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        let pane = spawn["pane"].as_u64().unwrap() as PaneId;

        let mut wait: Request =
            serde_json::from_str("{\"op\":\"wait_state\",\"want\":[\"idle\"]}").unwrap();
        wait.pane = Some(pane);
        wait.timeout_secs = Some(10);
        let got = responded(dispatch(&core, &wait));
        assert_eq!(got["ok"], true);
        assert_eq!(got["state"], "idle");

        let mut miss: Request =
            serde_json::from_str("{\"op\":\"wait_state\",\"want\":[\"working\"]}").unwrap();
        miss.pane = Some(pane);
        miss.timeout_secs = Some(1);
        let v = responded(dispatch(&core, &miss));
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("timed out"));
    }

    #[test]
    fn waits_reject_dead_panes() {
        let core = core();
        let mut wait: Request =
            serde_json::from_str("{\"op\":\"wait_state\",\"want\":[\"idle\"]}").unwrap();
        wait.pane = Some(424242);
        assert_eq!(responded(dispatch(&core, &wait))["ok"], false);
        let mut out: Request =
            serde_json::from_str("{\"op\":\"wait_output\",\"contains\":\"x\"}").unwrap();
        out.pane = Some(424242);
        assert_eq!(responded(dispatch(&core, &out))["ok"], false);
    }
}

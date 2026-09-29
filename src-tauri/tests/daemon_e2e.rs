//! Daemon end-to-end: boot the real `ubra-daemon` binary on a scratch
//! state dir, drive the protocol over TCP (directly and via `ubra-cli`),
//! and shut it down.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn scratch_dir() -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ubra-daemon-e2e-{}-{id}", std::process::id()))
}

/// Locate a sibling binary under target/<profile>/. Full `cargo test`
/// builds bins first; filtered runs need `cargo build` beforehand.
fn bin(name: &str) -> PathBuf {
    let mut dir = std::env::current_exe().expect("test exe path");
    dir.pop();
    if dir.ends_with("deps") {
        dir.pop();
    }
    let path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "missing binary {} (run full `cargo test` or `cargo build` first)",
        path.display()
    );
    path
}

struct Daemon {
    child: Option<Child>,
    state_dir: PathBuf,
}

impl Daemon {
    // All paths wait: success moves the child into `Self` (whose `Drop`
    // kills + waits), timeout kills + waits before panicking. The lint
    // cannot see through the struct move.
    #[allow(clippy::zombie_processes)]
    fn start() -> Self {
        let state_dir = scratch_dir();
        let mut child = Command::new(bin("ubra-daemon"))
            .arg("--state-dir")
            .arg(&state_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn ubra-daemon");
        let port_file = state_dir.join("daemon.json");
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(text) = std::fs::read_to_string(&port_file) {
                if let Ok(port) = serde_json::from_str::<serde_json::Value>(&text) {
                    let addr = format!("127.0.0.1:{}", port["port"]);
                    if let Ok(addr) = addr.parse() {
                        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                            return Self {
                                child: Some(child),
                                state_dir,
                            };
                        }
                    }
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("daemon never came up (state dir {})", state_dir.display());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn wait_exit(mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.child.as_mut().unwrap().try_wait().unwrap() {
                Some(status) => {
                    assert!(status.success(), "daemon exit status: {status}");
                    break;
                }
                None => assert!(
                    Instant::now() < deadline,
                    "daemon did not exit after shutdown"
                ),
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.child.take().unwrap().wait().unwrap();
        assert!(
            !self.state_dir.join("daemon.json").exists(),
            "shutdown must remove the port file"
        );
        let _ = std::fs::remove_dir_all(&self.state_dir);
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.state_dir);
    }
}

struct Client {
    stream: TcpStream,
}

impl Client {
    fn connect(state_dir: &std::path::Path) -> Self {
        let text = std::fs::read_to_string(state_dir.join("daemon.json")).unwrap();
        let port: serde_json::Value = serde_json::from_str(&text).unwrap();
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", port["port"]).parse().unwrap();
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap();
        Self { stream }
    }

    /// Send a request; skip pushed events until the response arrives.
    fn request(&mut self, value: serde_json::Value) -> serde_json::Value {
        writeln!(self.stream, "{value}").unwrap();
        self.stream.flush().unwrap();
        let mut reader = BufReader::new(self.stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(!line.is_empty(), "daemon closed the connection");
            let value: serde_json::Value = serde_json::from_str(&line).unwrap();
            if value.get("ok").is_some() {
                return value;
            }
        }
    }
}

fn cli(state_dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(bin("ubra-cli"))
        .arg("--state-dir")
        .arg(state_dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run ubra-cli")
}

#[test]
fn daemon_serves_protocol_end_to_end() {
    let daemon = Daemon::start();
    let mut client = Client::connect(&daemon.state_dir);

    let pong = client.request(serde_json::json!({"op": "ping", "id": 7}));
    assert_eq!(pong["ok"], true);
    assert_eq!(pong["id"], 7);
    assert_eq!(pong["protocol"], 1);

    let spawn = client.request(serde_json::json!({"op": "pty_spawn"}));
    assert_eq!(spawn["ok"], true);
    let pane = spawn["pane"].as_u64().unwrap();

    let write = client.request(
        serde_json::json!({"op": "pty_write", "pane": pane, "data": "echo marker-e2e789\n"}),
    );
    assert_eq!(write["ok"], true);

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let read = client.request(serde_json::json!({"op": "pty_read", "pane": pane}));
        let screen = read["screen"].as_str().unwrap().to_string();
        if screen.contains("marker-e2e789") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "screen never showed output: {screen:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    let panes = client.request(serde_json::json!({"op": "panes"}));
    assert!(panes["panes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == pane));

    let agents = client.request(serde_json::json!({"op": "agent_states"}));
    assert!(agents["states"].get(pane.to_string()).is_some());

    let snap = client.request(serde_json::json!({"op": "snapshot"}));
    assert_eq!(snap["ok"], true);
    assert!(snap.get("version").is_some());
    assert!(snap["panes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == pane));

    let reload = client.request(serde_json::json!({"op": "rules_reload"}));
    assert_eq!(reload["ok"], true);

    let bad = client.request(serde_json::json!({"op": "bogus"}));
    assert_eq!(bad["ok"], false);

    let down = client.request(serde_json::json!({"op": "shutdown"}));
    assert_eq!(down["ok"], true);
    daemon.wait_exit();
}

#[test]
fn cli_drives_running_daemon() {
    let daemon = Daemon::start();

    let ping = cli(&daemon.state_dir, &["ping"]);
    assert!(
        ping.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ping.stderr)
    );
    let pong: serde_json::Value =
        serde_json::from_str(&String::from_utf8(ping.stdout).unwrap()).unwrap();
    assert_eq!(pong["ok"], true);

    let spawn = cli(&daemon.state_dir, &["spawn"]);
    assert!(spawn.status.success());
    let spawned: serde_json::Value =
        serde_json::from_str(&String::from_utf8(spawn.stdout).unwrap()).unwrap();
    let pane = spawned["pane"].as_u64().unwrap();

    let panes = cli(&daemon.state_dir, &["panes"]);
    let list: serde_json::Value =
        serde_json::from_str(&String::from_utf8(panes.stdout).unwrap()).unwrap();
    assert!(list["panes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == pane));

    let waiting = cli(
        &daemon.state_dir,
        &["wait-state", &pane.to_string(), "idle", "--timeout", "10"],
    );
    assert!(waiting.status.success());
    let waited: serde_json::Value =
        serde_json::from_str(&String::from_utf8(waiting.stdout).unwrap()).unwrap();
    assert_eq!(waited["state"], "idle");

    let down = cli(&daemon.state_dir, &["shutdown"]);
    assert!(down.status.success());
    assert!(String::from_utf8(down.stdout)
        .unwrap()
        .contains("daemon stopped"));
    daemon.wait_exit();
}

#[test]
fn automation_ops_drive_pane_over_protocol() {
    let daemon = Daemon::start();
    let mut client = Client::connect(&daemon.state_dir);

    let spawn = client.request(serde_json::json!({"op": "pty_spawn"}));
    let pane = spawn["pane"].as_u64().unwrap();

    // send-keys submits a pending line; wait-output observes the result.
    let write = client
        .request(serde_json::json!({"op": "pty_write", "pane": pane, "data": "echo marker-p6e"}));
    assert_eq!(write["ok"], true);
    let keys =
        client.request(serde_json::json!({"op": "pty_send_keys", "pane": pane, "keys": ["enter"]}));
    assert_eq!(keys["ok"], true);
    let found = client.request(serde_json::json!({
        "op": "wait_output", "pane": pane, "contains": "marker-p6e", "timeout_secs": 10,
    }));
    assert_eq!(found["ok"], true);

    // wait-state observes the idle shell.
    let idle = client.request(serde_json::json!({
        "op": "wait_state", "pane": pane, "want": ["idle"], "timeout_secs": 10,
    }));
    assert_eq!(idle["state"], "idle");

    // prompt pastes and submits in one step (unix: bracketed paste).
    #[cfg(unix)]
    {
        let prompted = client.request(serde_json::json!({
            "op": "agent_prompt", "pane": pane, "data": "echo marker-p6f",
        }));
        assert_eq!(prompted["ok"], true);
        let found = client.request(serde_json::json!({
            "op": "wait_output", "pane": pane, "contains": "marker-p6f", "timeout_secs": 10,
        }));
        assert_eq!(found["ok"], true);
    }

    let down = client.request(serde_json::json!({"op": "shutdown"}));
    assert_eq!(down["ok"], true);
    daemon.wait_exit();
}

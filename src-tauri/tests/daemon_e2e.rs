//! Daemon end-to-end: boot the real `ubra-daemon` binary on a scratch
//! state dir, drive the protocol over TCP (directly and via `ubra-cli`),
//! and shut it down.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use ubra_lib::daemon::{
    connect_authenticated, new_auth_token, secure_state_dir, write_auth_token, write_port_file,
    MAX_CLIENTS, MAX_FRAME_BYTES, PROTOCOL_VERSION,
};

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

fn daemon_addr(state_dir: &Path) -> SocketAddr {
    let text = std::fs::read_to_string(state_dir.join("daemon.json")).unwrap();
    let port: serde_json::Value = serde_json::from_str(&text).unwrap();
    format!("127.0.0.1:{}", port["port"]).parse().unwrap()
}

fn auth_token(state_dir: &Path) -> String {
    std::fs::read_to_string(state_dir.join("daemon.auth"))
        .unwrap()
        .trim()
        .to_string()
}

fn authenticated_stream(state_dir: &Path) -> std::io::Result<TcpStream> {
    connect_authenticated(state_dir, Duration::from_secs(5))
}

fn proof(token: &str, role: &str, client: &str, server: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(token.as_bytes()).unwrap();
    mac.update(format!("ubra-v{PROTOCOL_VERSION}:{role}:{client}:{server}").as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Daemon {
    child: Option<Child>,
    state_dir: PathBuf,
}

impl Daemon {
    // All paths wait: success moves the child into `Self` (whose `Drop`
    // kills + waits), timeout kills + waits before panicking. The lint
    // cannot see through the struct move.
    fn start() -> Self {
        Self::start_in(scratch_dir())
    }

    #[allow(clippy::zombie_processes)]
    fn start_in(state_dir: PathBuf) -> Self {
        let mut child = Command::new(bin("ubra-daemon"))
            .arg("--state-dir")
            .arg(&state_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn ubra-daemon");
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if state_dir.join("daemon.json").exists()
                && state_dir.join("daemon.auth").exists()
                && authenticated_stream(&state_dir).is_ok()
            {
                return Self {
                    child: Some(child),
                    state_dir,
                };
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
        assert!(
            !self.state_dir.join("daemon.auth").exists(),
            "shutdown must remove the auth file"
        );
        assert!(
            self.state_dir.join("daemon.lock").is_file(),
            "persistent lock inode must remain safe for restart"
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
    reader: BufReader<TcpStream>,
    next_id: u64,
}

impl Client {
    fn connect(state_dir: &Path) -> Self {
        let stream = authenticated_stream(state_dir).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        Self {
            stream,
            reader,
            next_id: 1,
        }
    }

    fn request(&mut self, mut value: serde_json::Value) -> serde_json::Value {
        let id = value["id"].as_u64().unwrap_or_else(|| {
            let id = self.next_id;
            self.next_id += 1;
            id
        });
        value["id"] = id.into();
        writeln!(self.stream, "{value}").unwrap();
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).unwrap();
            assert!(!line.is_empty(), "daemon closed the connection");
            let value: serde_json::Value = serde_json::from_str(&line).unwrap();
            if value["id"] == id && value["ok"].is_boolean() {
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
fn unauthenticated_clients_receive_no_snapshot_or_greeting() {
    let daemon = Daemon::start();
    let mut stream =
        TcpStream::connect_timeout(&daemon_addr(&daemon.state_dir), Duration::from_secs(5))
            .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
        other => panic!("unauthenticated connection received data: {other:?} {line:?}"),
    }

    writeln!(stream, "{}", serde_json::json!({"op": "snapshot"})).unwrap();
    stream.flush().unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap();
    let denied: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(denied["ok"], false);
    assert!(denied.get("panes").is_none());
    assert!(denied.get("states").is_none());

    let mut client = Client::connect(&daemon.state_dir);
    assert_eq!(
        client.request(serde_json::json!({"op": "shutdown"}))["ok"],
        true
    );
    daemon.wait_exit();
}

#[test]
fn wrong_auth_proof_is_rejected() {
    let daemon = Daemon::start();
    let mut stream =
        TcpStream::connect_timeout(&daemon_addr(&daemon.state_dir), Duration::from_secs(5))
            .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let nonce = new_auth_token().unwrap();
    writeln!(
        stream,
        "{}",
        serde_json::json!({"op":"hello","id":0,"nonce":nonce})
    )
    .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let challenge: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(challenge.get("states").is_none());
    assert!(challenge.get("panes").is_none());
    let wrong = proof(
        &"0".repeat(64),
        "client",
        &nonce,
        challenge["nonce"].as_str().unwrap(),
    );
    writeln!(
        stream,
        "{}",
        serde_json::json!({"op":"auth","id":0,"proof":wrong})
    )
    .unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap();
    let denied: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(denied["ok"], false);
    line.clear();
    assert_eq!(reader.read_line(&mut line).unwrap(), 0);
    let mut client = Client::connect(&daemon.state_dir);
    assert_eq!(
        client.request(serde_json::json!({"op": "shutdown"}))["ok"],
        true
    );
    daemon.wait_exit();
}

#[test]
fn fake_port_endpoint_does_not_satisfy_cli() {
    let state_dir = scratch_dir();
    secure_state_dir(&state_dir).unwrap();
    write_auth_token(&state_dir, &"a".repeat(64)).unwrap();
    let fake = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let fake_port = fake.local_addr().unwrap().port();
    write_port_file(&state_dir, fake_port).unwrap();
    let fake_thread = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = fake.accept() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            reader.read_line(&mut line).unwrap();
            let hello: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(hello["op"], "hello");
            assert!(hello.get("token").is_none());
            assert!(hello.get("proof").is_none());
            let _ = writeln!(
                stream,
                "{}",
                serde_json::json!({"id":0,"protocol":PROTOCOL_VERSION,"nonce":"b".repeat(64),"proof":"c".repeat(64)})
            );
            line.clear();
            assert_eq!(
                reader.read_line(&mut line).unwrap(),
                0,
                "client must not prove identity to an unverified endpoint"
            );
        }
    });

    let ping = cli(&state_dir, &["ping"]);
    assert!(
        ping.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ping.stderr)
    );
    let pong: serde_json::Value =
        serde_json::from_str(&String::from_utf8(ping.stdout).unwrap()).unwrap();
    assert_eq!(pong["ok"], true);
    assert_eq!(pong["protocol"], PROTOCOL_VERSION);
    fake_thread.join().unwrap();

    let down = cli(&state_dir, &["shutdown"]);
    assert!(down.status.success());
    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn concurrent_starts_leave_one_daemon_owner() {
    let state_dir = scratch_dir();
    let mut children: Vec<Child> = (0..4)
        .map(|_| {
            Command::new(bin("ubra-daemon"))
                .arg("--state-dir")
                .arg(&state_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn ubra-daemon")
        })
        .collect();

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if state_dir.join("daemon.json").exists()
            && state_dir.join("daemon.auth").exists()
            && authenticated_stream(&state_dir).is_ok()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "daemon never became authenticatable"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let running = children
            .iter_mut()
            .map(|child| usize::from(child.try_wait().unwrap().is_none()))
            .sum::<usize>();
        if running == 1 {
            break;
        }
        assert!(Instant::now() < deadline, "expected one daemon owner");
        std::thread::sleep(Duration::from_millis(100));
    }

    for child in &mut children {
        if child.try_wait().unwrap().is_none() {
            child.kill().unwrap();
        }
        let _ = child.wait();
    }
    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn daemon_serves_protocol_end_to_end() {
    let daemon = Daemon::start();
    let mut client = Client::connect(&daemon.state_dir);

    let pong = client.request(serde_json::json!({"op": "ping", "id": 7}));
    assert_eq!(pong["ok"], true);
    assert_eq!(pong["id"], 7);
    assert_eq!(pong["protocol"], PROTOCOL_VERSION);

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

    // The worker discovers new panes asynchronously; a status query must
    // not run classification itself to force a fresh observation.
    let observed = client.request(serde_json::json!({
        "op": "wait_state", "pane": pane, "want": ["idle"], "timeout_secs": 10,
    }));
    assert_eq!(observed["ok"], true);
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

#[test]
fn cli_failures_preserve_daemon_error_json() {
    let daemon = Daemon::start();
    for args in [
        vec!["read", "4294967295"],
        vec!["write", "4294967295", "hello"],
        vec!["resize", "4294967295", "80", "24"],
        vec!["spawn", "--shell", "ubra-executable-that-does-not-exist"],
        vec!["wait-output", "4294967295", "never", "--timeout", "1"],
    ] {
        let output = cli(&daemon.state_dir, &args);
        assert!(
            !output.status.success(),
            "operation falsely succeeded: {args:?}"
        );
        let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["ok"], false);
        assert!(response["error"].is_string());
        assert!(!output.stderr.is_empty());
    }
    let spawned = cli(&daemon.state_dir, &["spawn"]);
    assert!(spawned.status.success());
    let pane = serde_json::from_slice::<serde_json::Value>(&spawned.stdout).unwrap()["pane"]
        .as_u64()
        .unwrap()
        .to_string();
    let started = Instant::now();
    let timed_out = cli(
        &daemon.state_dir,
        &["wait-output", &pane, "UNREACHABLE_MARKER", "--timeout", "1"],
    );
    assert!(!timed_out.status.success());
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(
        serde_json::from_slice::<serde_json::Value>(&timed_out.stdout).unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("timed out")
    );
    assert!(cli(&daemon.state_dir, &["shutdown"]).status.success());
    daemon.wait_exit();
}

#[test]
fn stale_crashed_owner_is_recovered_and_credentials_rotate() {
    let mut old = Daemon::start();
    let token = auth_token(&old.state_dir);
    let mut child = old.child.take().unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    let recovered = Daemon::start_in(old.state_dir.clone());
    assert_ne!(token, auth_token(&recovered.state_dir));
    let mut client = Client::connect(&recovered.state_dir);
    assert_eq!(client.request(serde_json::json!({"op":"ping"}))["ok"], true);
    assert_eq!(
        client.request(serde_json::json!({"op":"shutdown"}))["shutdown"],
        true
    );
    recovered.wait_exit();
}

#[cfg(unix)]
#[test]
fn unsafe_runtime_files_do_not_modify_symlink_or_hardlink_targets() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = scratch_dir();
    secure_state_dir(&dir).unwrap();
    let target = dir.join("original");
    std::fs::write(&target, "preserve me").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&target, dir.join("daemon.log")).unwrap();
    let output = cli(&dir, &["ping"]);
    assert!(!output.status.success());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "preserve me");
    std::fs::remove_file(dir.join("daemon.log")).unwrap();
    std::fs::hard_link(&target, dir.join("daemon.log")).unwrap();
    assert!(!cli(&dir, &["ping"]).status.success());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "preserve me");
    std::fs::remove_file(dir.join("daemon.log")).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        secure_state_dir(&dir).is_err(),
        "unsafe existing directory must not be trusted"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn oversized_authenticated_frames_disconnect_only_the_offender() {
    let daemon = Daemon::start();
    let mut stream = authenticated_stream(&daemon.state_dir).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let oversized =
        serde_json::json!({"op":"pty_write","pane":1,"data":"x".repeat(MAX_FRAME_BYTES)});
    let _ = writeln!(stream, "{oversized}");
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => assert!(
                serde_json::from_str::<serde_json::Value>(&line).unwrap()["event"].is_string()
            ),
            Err(error) if error.kind() == ErrorKind::ConnectionReset => break,
            other => panic!("oversized sender not disconnected: {other:?}"),
        }
    }
    let mut healthy = Client::connect(&daemon.state_dir);
    assert_eq!(
        healthy.request(serde_json::json!({"op":"ping"}))["ok"],
        true
    );
    assert_eq!(
        healthy.request(serde_json::json!({"op":"shutdown"}))["shutdown"],
        true
    );
    daemon.wait_exit();
}

#[test]
fn unauthenticated_connection_budget_is_released_on_disconnect() {
    let daemon = Daemon::start();
    std::thread::sleep(Duration::from_millis(100));
    let pending: Vec<_> = (0..MAX_CLIENTS)
        .map(|_| TcpStream::connect(daemon_addr(&daemon.state_dir)).unwrap())
        .collect();
    let mut extra = TcpStream::connect(daemon_addr(&daemon.state_dir)).unwrap();
    extra
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    match extra.read(&mut [0_u8; 1]) {
        Ok(0) => {}
        Err(error) if error.kind() == ErrorKind::ConnectionReset => {}
        other => panic!("over-budget connection was not rejected: {other:?}"),
    }
    drop(pending);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(stream) = authenticated_stream(&daemon.state_dir) {
            drop(stream);
            break;
        }
        assert!(Instant::now() < deadline, "connection slots not released");
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut healthy = Client::connect(&daemon.state_dir);
    assert_eq!(
        healthy.request(serde_json::json!({"op":"shutdown"}))["ok"],
        true
    );
    daemon.wait_exit();
}

/// A real authenticated test server lets the CLI contract be exercised
/// independently of correct daemon responses.
fn fake_daemon_cli(
    args: &[&str],
    respond: impl FnOnce(TcpStream, serde_json::Value) + Send + 'static,
) -> std::process::Output {
    let dir = scratch_dir();
    secure_state_dir(&dir).unwrap();
    let token = new_auth_token().unwrap();
    write_auth_token(&dir, &token).unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    write_port_file(&dir, listener.local_addr().unwrap().port()).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let hello: serde_json::Value = serde_json::from_str(&line).unwrap();
        let nonce = new_auth_token().unwrap();
        let client_nonce = hello["nonce"].as_str().unwrap();
        writeln!(
            stream,
            "{}",
            serde_json::json!({
                "id":0,"protocol":PROTOCOL_VERSION,"nonce":nonce,
                "proof":proof(&token,"server",client_nonce,&nonce)
            })
        )
        .unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        let auth: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(auth["proof"], proof(&token, "client", client_nonce, &nonce));
        writeln!(
            stream,
            "{}",
            serde_json::json!({"id":0,"ok":true,"protocol":PROTOCOL_VERSION})
        )
        .unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        let request = serde_json::from_str(&line).unwrap();
        respond(stream, request);
    });
    let output = cli(&dir, args);
    server.join().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    output
}

#[test]
fn cli_shutdown_skips_event_bursts_and_unrelated_ids_until_matching_ack() {
    let output = fake_daemon_cli(&["shutdown"], |mut stream, request| {
        for _ in 0..1500 {
            writeln!(
                stream,
                "{}",
                serde_json::json!({"event":"agent-states","states":{}})
            )
            .unwrap();
        }
        writeln!(
            stream,
            "{}",
            serde_json::json!({"id":999999,"ok":true,"shutdown":true})
        )
        .unwrap();
        writeln!(
            stream,
            "{}",
            serde_json::json!({"id":request["id"],"ok":true,"shutdown":true})
        )
        .unwrap();
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["shutdown"], true);
    assert_ne!(value["id"], 999999);
}

#[test]
fn cli_rejects_invalid_response_shapes_and_false_shutdown_success() {
    for (args, payload) in [
        (
            vec!["read", "1"],
            serde_json::json!({"ok":true,"screen":42}),
        ),
        (vec!["spawn"], serde_json::json!({"ok":true,"pane":"wrong"})),
        (vec!["shutdown"], serde_json::json!({"ok":true})),
        (
            vec!["ping"],
            serde_json::json!({"ok":"true","protocol":PROTOCOL_VERSION,"version":"fake"}),
        ),
        (
            vec!["shutdown"],
            serde_json::json!({"ok":false,"error":"refused shutdown"}),
        ),
    ] {
        let output = fake_daemon_cli(&args, move |mut stream, request| {
            let mut response = payload;
            response["id"] = request["id"].clone();
            writeln!(stream, "{response}").unwrap();
        });
        assert!(!output.status.success(), "{args:?} falsely succeeded");
        assert!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).is_ok(),
            "useful malformed/error JSON lost"
        );
    }
}

#[test]
fn cli_silent_authenticated_operation_has_finite_deadline() {
    let started = Instant::now();
    let output = fake_daemon_cli(&["ping"], |mut stream, _| {
        // Wait for the CLI's own operation deadline to close the connection.
        assert_eq!(stream.read(&mut [0_u8; 1]).unwrap(), 0);
    });
    assert!(!output.status.success());
    assert!(started.elapsed() < Duration::from_secs(14));
    assert!(String::from_utf8_lossy(&output.stderr).contains("connection"));
}

#[cfg(unix)]
#[test]
fn slow_output_consumer_is_disconnected_while_healthy_client_progresses() {
    let daemon = Daemon::start();
    let mut healthy = Client::connect(&daemon.state_dir);
    let spawned = healthy.request(serde_json::json!({"op":"pty_spawn","shell":"/bin/sh"}));
    let pane = spawned["pane"].as_u64().unwrap();
    let mut slow = authenticated_stream(&daemon.state_dir).unwrap();
    let command = "i=0; while [ \"$i\" -lt 131072 ]; do printf '%0128d\\n' 0; i=$((i+1)); done; printf 'FLOOD_%s\\n' COMPLETE\r";
    assert_eq!(
        healthy.request(serde_json::json!({"op":"pty_write","pane":pane,"data":command}))["ok"],
        true
    );
    assert_eq!(
        healthy.request(serde_json::json!({"op":"ping"}))["ok"],
        true
    );
    let found = healthy.request(serde_json::json!({
        "op":"wait_output","pane":pane,"contains":"FLOOD_COMPLETE","timeout_secs":30
    }));
    assert_eq!(found["ok"], true);
    slow.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut buffer = [0_u8; 65536];
    loop {
        assert!(Instant::now() < deadline, "slow peer never disconnected");
        match slow.read(&mut buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::ConnectionReset => break,
            Err(error) => panic!("slow peer survived output budget: {error}"),
        }
    }
    assert_eq!(
        healthy.request(serde_json::json!({"op":"pty_read","pane":pane}))["ok"],
        true
    );
    assert_eq!(
        healthy.request(serde_json::json!({"op":"shutdown"}))["shutdown"],
        true
    );
    daemon.wait_exit();
}

#[cfg(windows)]
#[test]
fn headless_windows_command_executes_side_effect_not_just_input_echo() {
    let daemon = Daemon::start();
    let marker = daemon.state_dir.join("executed.txt");
    let mut client = Client::connect(&daemon.state_dir);
    let pane = client.request(serde_json::json!({"op":"pty_spawn","shell":"powershell.exe"}))
        ["pane"]
        .as_u64()
        .unwrap();
    let quoted = marker.to_string_lossy().replace('\'', "''");
    let command = format!("Set-Content -LiteralPath '{quoted}' -Value 'EXECUTED'; Write-Output ('HEADLESS_' + 'COMPLETE')\r");
    assert_eq!(
        client.request(serde_json::json!({"op":"pty_write","pane":pane,"data":command}))["ok"],
        true
    );
    assert_eq!(
        client.request(serde_json::json!({
            "op":"wait_output","pane":pane,"contains":"HEADLESS_COMPLETE","timeout_secs":10
        }))["ok"],
        true
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap().trim(), "EXECUTED");
    assert_eq!(
        client.request(serde_json::json!({"op":"shutdown"}))["shutdown"],
        true
    );
    daemon.wait_exit();
}

#[test]
fn silent_endpoint_shutdown_fails_without_starting_or_deleting_runtime_state() {
    let dir = scratch_dir();
    secure_state_dir(&dir).unwrap();
    let token = new_auth_token().unwrap();
    write_auth_token(&dir, &token).unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    write_port_file(&dir, listener.local_addr().unwrap().port()).unwrap();
    let port_before = std::fs::read(dir.join("daemon.json")).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut hello = String::new();
        reader.read_line(&mut hello).unwrap();
        assert_eq!(stream.read(&mut [0_u8; 1]).unwrap(), 0);
    });
    let started = Instant::now();
    let output = cli(&dir, &["shutdown"]);
    assert!(!output.status.success());
    assert!(started.elapsed() < Duration::from_secs(4));
    server.join().unwrap();
    assert_eq!(auth_token(&dir), token);
    assert_eq!(std::fs::read(dir.join("daemon.json")).unwrap(), port_before);
    assert!(
        !dir.join("daemon.log").exists(),
        "shutdown must not start another daemon"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn pane_budget_rejects_excess_and_releases_capacity_after_kill() {
    use ubra_lib::daemon::MAX_PANES;
    let daemon = Daemon::start();
    let mut client = Client::connect(&daemon.state_dir);
    let mut panes = Vec::new();
    for _ in 0..MAX_PANES {
        let response = client.request(serde_json::json!({
            "op":"pty_spawn","shell":"/bin/sh","cols":2,"rows":1
        }));
        assert_eq!(response["ok"], true);
        panes.push(response["pane"].as_u64().unwrap());
    }
    let rejected = client.request(serde_json::json!({"op":"pty_spawn","shell":"/bin/sh"}));
    assert_eq!(rejected["ok"], false);
    assert_eq!(
        client.request(serde_json::json!({"op":"panes"}))["panes"]
            .as_array()
            .unwrap()
            .len(),
        MAX_PANES
    );
    assert_eq!(
        client.request(serde_json::json!({"op":"pty_kill","pane":panes[0]}))["ok"],
        true
    );
    assert_eq!(
        client.request(serde_json::json!({"op":"pty_spawn","shell":"/bin/sh","cols":2,"rows":1}))
            ["ok"],
        true
    );
    assert_eq!(
        client.request(serde_json::json!({"op":"shutdown"}))["shutdown"],
        true
    );
    daemon.wait_exit();
}

#[cfg(windows)]
#[test]
fn windows_runtime_acl_rejects_credentials_readable_by_everyone() {
    let dir = scratch_dir();
    secure_state_dir(&dir).unwrap();
    let token = new_auth_token().unwrap();
    write_auth_token(&dir, &token).unwrap();
    assert_eq!(
        ubra_lib::daemon::read_auth_token(&dir).as_deref(),
        Some(token.as_str())
    );
    let output = Command::new("icacls.exe")
        .arg(dir.join("daemon.auth"))
        .args(["/grant", "*S-1-1-0:R"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(ubra_lib::daemon::read_auth_token(&dir).is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

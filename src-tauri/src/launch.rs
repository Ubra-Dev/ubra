//! Shared daemon launch/connect for the desktop app and the CLI.
//!
//! Both frontends use [`launch_or_connect`]: connect to the running daemon,
//! or start one detached from the launcher's lifetime (Unix `setsid` with
//! detached stdio, detached Windows process creation) and wait for it to
//! answer. A daemon speaking another protocol version is reported with
//! restart instructions; existing sessions are never stopped automatically.

use crate::daemon::{connect_authenticated, read_port_file, PROTOCOL_VERSION};
use std::io;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn sibling_binary(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    // Bundled sidecars sit next to the app binary; on macOS also check the
    // app bundle's Resources directory.
    let candidates = [
        dir.join(name),
        dir.join("Resources").join(name),
        dir.parent()?.join("Resources").join(name),
    ];
    candidates.into_iter().find(|path| path.is_file())
}

fn env_override(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

/// Resolve the daemon executable: explicit override, bundled sibling,
/// or PATH lookup.
pub fn daemon_binary() -> PathBuf {
    if let Some(path) = env_override("UBRA_DAEMON") {
        return path;
    }
    let name = format!("ubra-daemon{}", std::env::consts::EXE_SUFFIX);
    sibling_binary(&name).unwrap_or(PathBuf::from(name))
}

/// Resolve the CLI executable (passed to panes as `UBRA_CLI` so hooks can
/// report sessions without PATH lookups).
pub fn cli_binary() -> PathBuf {
    if let Some(path) = env_override("UBRA_CLI") {
        return path;
    }
    let name = format!("ubra-cli{}", std::env::consts::EXE_SUFFIX);
    sibling_binary(&name).unwrap_or(PathBuf::from(name))
}

/// Spawn a daemon detached from the launcher's lifetime. Stdio is
/// disconnected (stderr goes to `log_file`); the child is reaped by a
/// waiter thread while the launcher lives and by init after it exits.
pub fn spawn_detached(program: &PathBuf, args: &[&str], log_file: std::fs::File) -> io::Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log_file);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            // SAFETY: setsid is async-signal-safe; runs in the child after
            // fork, where the child is never a process group leader.
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS (0x8) | CREATE_NEW_PROCESS_GROUP (0x200): no
        // console, survives the launching terminal.
        cmd.creation_flags(0x8 | 0x200);
    }
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// What, if anything, answers in `state_dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonStatus {
    /// No port file or nothing listening.
    Absent,
    /// A daemon answers with our protocol.
    Current,
    /// A daemon answers, but speaks another protocol version.
    Incompatible { protocol: u32 },
}

/// Probe for a daemon without authenticating: the hello challenge carries
/// the protocol version before any credential is used.
pub fn probe_daemon(state_dir: &std::path::Path) -> DaemonStatus {
    let Some(port) = read_port_file(state_dir) else {
        return DaemonStatus::Absent;
    };
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port.port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return DaemonStatus::Absent;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    use io::{BufRead, BufReader, Write};
    // The nonce must be well-formed: daemons reject malformed hellos with a
    // bare `unauthorized` reply carrying no protocol version, which would
    // make a live-but-incompatible daemon look absent.
    let Ok(nonce) = crate::daemon::new_auth_token() else {
        return DaemonStatus::Absent;
    };
    let hello = serde_json::json!({"op":"hello","id":0,"nonce":nonce}).to_string();
    if writeln!(stream, "{hello}").is_err() {
        return DaemonStatus::Absent;
    }
    let mut line = String::new();
    let mut reader = BufReader::new(stream.try_clone().unwrap_or(stream));
    if reader.read_line(&mut line).is_err() {
        return DaemonStatus::Absent;
    }
    let protocol = serde_json::from_str::<serde_json::Value>(&line)
        .ok()
        .and_then(|v| v.get("protocol").and_then(serde_json::Value::as_u64))
        .map(|v| v as u32);
    match protocol {
        None => DaemonStatus::Absent,
        Some(p) if p == PROTOCOL_VERSION => DaemonStatus::Current,
        Some(p) => DaemonStatus::Incompatible { protocol: p },
    }
}

/// Human guidance for an incompatible daemon. Neither the app nor the
/// current CLI can authenticate to it, so stopping it is necessarily manual.
/// Never stop sessions implicitly.
pub fn incompatible_message(running: u32, state_dir: &std::path::Path) -> String {
    format!(
        "A Ubra daemon speaking protocol {running} is already running, but this app needs protocol {PROTOCOL_VERSION}. \
        Stop the old daemon explicitly: its PID is in {}, e.g. `kill <pid>`, then relaunch this app. \
        Your saved layouts are preserved; terminals owned by the old daemon cannot be reattached by this version.",
        state_dir.join(crate::daemon::PORT_FILE).display()
    )
}

/// Whether a fresh daemon should be spawned for a probe outcome. A daemon
/// that already answers holds the startup lock, so spawning would only add
/// an "already running" sibling that exits immediately.
fn should_spawn(status: &DaemonStatus) -> bool {
    matches!(status, DaemonStatus::Absent)
}

/// Connect, starting a detached daemon first when none answers. Fails with
/// restart instructions when an incompatible daemon holds the state dir.
pub fn launch_or_connect(state_dir: &std::path::Path) -> Result<TcpStream, String> {
    if let Ok(stream) = connect_authenticated(state_dir, Duration::from_millis(500)) {
        return Ok(stream);
    }
    let probe = probe_daemon(state_dir);
    if let DaemonStatus::Incompatible { protocol } = probe {
        return Err(incompatible_message(protocol, state_dir));
    }
    // A current daemon answers but the first authentication missed (e.g. it
    // was mid-startup): poll it instead of spawning a redundant sibling that
    // would only exit "already running".
    if should_spawn(&probe) {
        let log = crate::daemon::open_private_file(&state_dir.join("daemon.log"), true, true)
            .map_err(|e| format!("cannot open private daemon log: {e}"))?;
        spawn_detached(
            &daemon_binary(),
            &["--state-dir", &state_dir.to_string_lossy()],
            log,
        )
        .map_err(|e| format!("cannot start ubra-daemon: {e}"))?;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        if let Ok(stream) = connect_authenticated(
            state_dir,
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(250)),
        ) {
            return Ok(stream);
        }
        if let DaemonStatus::Incompatible { protocol } = probe_daemon(state_dir) {
            return Err(incompatible_message(protocol, state_dir));
        }
    }
    Err(format!(
        "ubra-daemon did not answer (state dir {}; log at daemon.log)",
        state_dir.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incompatible_message_names_both_protocols() {
        let dir = std::path::Path::new("/tmp/ubra-test-state");
        let message = incompatible_message(2, dir);
        assert!(message.contains('2'));
        assert!(message.contains(&PROTOCOL_VERSION.to_string()));
        // The current CLI cannot authenticate across the skew either, so the
        // only working recovery is terminating the old daemon by PID.
        assert!(message.contains("daemon.json"));
        assert!(message.contains("kill <pid>"));
        assert!(!message.contains("ubra-cli shutdown"));
    }

    #[test]
    fn spawns_only_when_absent() {
        assert!(should_spawn(&DaemonStatus::Absent));
        assert!(!should_spawn(&DaemonStatus::Current));
        assert!(!should_spawn(&DaemonStatus::Incompatible { protocol: 2 }));
    }

    #[test]
    fn probe_reports_absent_without_port_file() {
        let dir = std::env::temp_dir().join(format!(
            "ubra-launch-probe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(probe_daemon(&dir), DaemonStatus::Absent);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fake daemon: answers one hello with `reply`, captures the hello line.
    fn serve_once(
        reply: String,
    ) -> (
        std::net::SocketAddr,
        std::sync::mpsc::Receiver<String>,
        std::thread::JoinHandle<()>,
    ) {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut hello = String::new();
            reader.read_line(&mut hello).unwrap();
            let _ = tx.send(hello);
            let mut stream = stream;
            let _ = writeln!(stream, "{reply}");
        });
        (addr, rx, handle)
    }

    fn scratch_state_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ubra-launch-probe-{}-{tag}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        crate::daemon::secure_state_dir(&dir).unwrap();
        dir
    }

    fn valid_nonce(nonce: &str) -> bool {
        nonce.len() == 64 && nonce.bytes().all(|b| b.is_ascii_hexdigit())
    }

    #[test]
    fn probe_classifies_live_daemon_versions() {
        for (reply, expected) in [
            (
                serde_json::json!({"protocol": 99, "nonce": "a".repeat(64)}).to_string(),
                DaemonStatus::Incompatible { protocol: 99 },
            ),
            (
                serde_json::json!({"protocol": PROTOCOL_VERSION, "nonce": "b".repeat(64)})
                    .to_string(),
                DaemonStatus::Current,
            ),
            (
                serde_json::json!({"ok": false, "error": "unauthorized"}).to_string(),
                DaemonStatus::Absent,
            ),
        ] {
            let (addr, hello_rx, handle) = serve_once(reply);
            let dir = scratch_state_dir("version");
            crate::daemon::write_port_file(&dir, addr.port()).unwrap();
            assert_eq!(probe_daemon(&dir), expected);
            // The probe hello must carry a well-formed nonce, or real daemons
            // answer `unauthorized` with no protocol and skew goes undetected.
            let hello: serde_json::Value =
                serde_json::from_str(&hello_rx.recv_timeout(Duration::from_secs(5)).unwrap())
                    .unwrap();
            assert_eq!(hello["op"], "hello");
            assert!(
                hello["nonce"].as_str().is_some_and(valid_nonce),
                "probe nonce must be 64 hex chars: {hello}"
            );
            handle.join().unwrap();
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn binaries_honor_env_overrides() {
        std::env::set_var("UBRA_DAEMON", "/tmp/test-ubra-daemon");
        std::env::set_var("UBRA_CLI", "/tmp/test-ubra-cli");
        assert_eq!(daemon_binary(), PathBuf::from("/tmp/test-ubra-daemon"));
        assert_eq!(cli_binary(), PathBuf::from("/tmp/test-ubra-cli"));
        std::env::remove_var("UBRA_DAEMON");
        std::env::remove_var("UBRA_CLI");
    }
}

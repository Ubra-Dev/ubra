//! GUI bridge end-to-end: `connect_or_ensure` boots the real
//! `ubra-daemon` binary on a scratch state dir (detached, as in production),
//! drives keyed GUI-style panes through `DaemonClient`, and shuts it down.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use ubra_lib::daemon_client::{connect_or_ensure, DaemonEvent};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn scratch_dir() -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ubra-bridge-e2e-{}-{id}", std::process::id()))
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

/// Best-effort daemon stop so a failed test never leaks a live daemon.
struct DaemonGuard {
    state_dir: PathBuf,
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let (tx, _rx) = mpsc::sync_channel(8);
        if let Ok(client) = ubra_lib::daemon_client::DaemonClient::connect(&self.state_dir, tx) {
            let _ = client.shutdown_daemon();
        }
        let _ = std::fs::remove_dir_all(&self.state_dir);
    }
}

#[test]
fn ensure_spawns_detached_daemon_and_gui_roundtrips() {
    // All tests in this binary resolve the same daemon path.
    unsafe {
        std::env::set_var("UBRA_DAEMON_BIN", bin("ubra-daemon"));
    }
    let state_dir = scratch_dir();
    let _guard = DaemonGuard {
        state_dir: state_dir.clone(),
    };
    // The spawned daemon inherits this: history lands on scratch, never in
    // the real app data dir.
    unsafe {
        std::env::set_var("UBRA_HISTORY_DIR", state_dir.join("history"));
    }

    // No daemon answers on a scratch dir: ensure must spawn one.
    let (tx, rx) = mpsc::sync_channel(256);
    let client = connect_or_ensure(&state_dir, None, tx).expect("ensure connects");
    client.ping().expect("ping answers");

    // A second ensure attaches to the same daemon instead of spawning.
    let (tx2, _rx2) = mpsc::sync_channel(8);
    let again = connect_or_ensure(&state_dir, None, tx2).expect("second ensure attaches");
    again.ping().expect("second client pings");

    // GUI-style keyed pane round trip over the real binary.
    let pane = client
        .spawn(
            None,
            None,
            Vec::new(),
            80,
            24,
            Some("pane-e2e-1".to_string()),
        )
        .expect("spawn");
    let sessions = client.list().expect("list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].key.as_deref(), Some("pane-e2e-1"));

    client.write(pane, "echo marker-bridge-9\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut transcript = String::new();
    let mut handshake_answered = false;
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(DaemonEvent::PtyOutput { data, .. }) => {
                // ConPTY startup query (Windows only): answer it or the pane
                // withholds all further output. In production xterm.js answers;
                // here the test plays the terminal itself. Silent no-op on
                // Unix, where the query never arrives.
                transcript.push_str(&data);
                if !handshake_answered && transcript.contains("\u{1b}[6n") {
                    handshake_answered = true;
                    let _ = client.write(pane, "\u{1b}[1;1R");
                }
                if data.contains("marker-bridge-9") {
                    break;
                }
            }
            Ok(_) => continue,
            Err(_) => panic!("timed out waiting for daemon output event"),
        }
    }
    let snap = client.snapshot(pane).expect("snapshot");
    assert!(snap.data.contains("marker-bridge-9"));

    // Killing the pane persists its screen; a fresh spawn under a new key
    // starts with no history.
    assert!(client
        .history("pane-e2e-absent")
        .expect("history")
        .is_none());
    client.kill(pane).expect("kill");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match client.history("pane-e2e-1").expect("history") {
            Some(saved) if saved.data.contains("marker-bridge-9") => break,
            _ if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            other => panic!("history never landed: {other:?}"),
        }
    }

    // Explicit shutdown stops the daemon; the client observes disconnect.
    client.shutdown_daemon().expect("shutdown accepted");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !client.is_dead() {
        assert!(
            Instant::now() < deadline,
            "client never observed daemon shutdown"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

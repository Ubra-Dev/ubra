//! Real-PTY integration tests for [`ubra_lib::pty_manager`]: spawn a process,
//! capture its output, and kill a live pane. No Tauri runtime involved.

use std::sync::mpsc;
use std::time::{Duration, Instant};
use ubra_lib::pty_manager::{PaneId, PtyEventSink, PtyManager};

enum Event {
    Output(PaneId, String),
    Exit(PaneId, bool),
}

struct ChannelSink {
    tx: mpsc::Sender<Event>,
}

impl PtyEventSink for ChannelSink {
    fn output(&self, id: PaneId, data: String) {
        let _ = self.tx.send(Event::Output(id, data));
    }

    fn exited(&self, id: PaneId, success: bool, _code: Option<i32>) {
        let _ = self.tx.send(Event::Exit(id, success));
    }
}

fn echo_command() -> (Option<String>, Vec<String>) {
    #[cfg(windows)]
    return (
        Some("cmd.exe".to_string()),
        vec!["/C".to_string(), "echo hello-pty".to_string()],
    );
    #[cfg(not(windows))]
    return (
        Some("sh".to_string()),
        vec!["-c".to_string(), "echo hello-pty".to_string()],
    );
}

#[test]
fn pty_spawns_and_captures_output() {
    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    let (shell, args) = echo_command();
    let id = manager.spawn(shell, None, args, 80, 24).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut transcript = String::new();
    let exited_success = loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(got, data)) => {
                assert_eq!(got, id);
                transcript.push_str(&data);
            }
            Ok(Event::Exit(got, success)) => {
                assert_eq!(got, id);
                break success;
            }
            Err(_) => panic!("timed out waiting for pane exit; got: {transcript:?}"),
        }
    };
    assert!(exited_success, "pane should exit 0");
    assert!(
        transcript.contains("hello-pty"),
        "transcript should contain echo output, got: {transcript:?}"
    );
}

#[test]
fn pty_kill_terminates_live_pane() {
    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    // Bare interactive shell blocks on input until killed.
    let id = manager.spawn(None, None, Vec::new(), 80, 24).unwrap();
    // Give the child a moment to start so kill targets a live process.
    std::thread::sleep(Duration::from_millis(500));
    manager.kill(id).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(_, _)) => continue,
            Ok(Event::Exit(got, _)) => {
                assert_eq!(got, id);
                return;
            }
            Err(_) => panic!("timed out waiting for killed pane to exit"),
        }
    }
}

#[test]
fn spawn_failure_returns_error_and_leaks_no_session() {
    let (tx, _rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    let result = manager.spawn(
        Some("ubra-no-such-binary-xyz".to_string()),
        None,
        Vec::new(),
        80,
        24,
    );
    assert!(result.is_err(), "bogus shell must fail to spawn");
    assert!(
        manager.pane_roots().is_empty(),
        "failed spawn must not leave a session behind"
    );
}

#[test]
fn unknown_pane_operations_fail() {
    let (tx, _rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    assert!(manager.write(999, "x").is_err());
    assert!(manager.resize(999, 80, 24).is_err());
    assert!(manager.kill(999).is_err());
}

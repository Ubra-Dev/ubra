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

/// Answers ConPTY's startup cursor-position query (Windows only).
///
/// ConPTY — and shells like PSReadLine — emit `ESC[6n` at startup and withhold
/// all further output until the terminal replies with a cursor position
/// report. In production xterm.js answers via `pty_write`; these headless
/// tests must play the terminal themselves, or the pane looks permanently
/// stuck (no output, no exit). Tracking the whole transcript also covers the
/// query arriving split across output chunks. On Unix the query never
/// arrives, so this is a silent no-op there.
struct Handshake {
    answered: bool,
}

impl Handshake {
    fn new() -> Self {
        Self { answered: false }
    }

    fn note_output(&mut self, manager: &PtyManager, id: PaneId, transcript: &str) {
        if !self.answered && transcript.contains("\u{1b}[6n") {
            self.answered = true;
            // Best-effort: if the pane already exited, no handshake is needed.
            let _ = manager.write(id, "\u{1b}[1;1R");
        }
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
    let mut handshake = Handshake::new();
    let exited_success = loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(got, data)) => {
                assert_eq!(got, id);
                transcript.push_str(&data);
                handshake.note_output(&manager, id, &transcript);
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
    // Drain startup output (answering the ConPTY handshake on Windows) so the
    // child is actually running — not blocked on an unanswered cursor query —
    // when killed. Must happen before kill: kill drops the session, and with
    // it the ability to write the handshake reply.
    let mut handshake = Handshake::new();
    let mut transcript = String::new();
    let warmup_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let timeout = warmup_deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            break;
        }
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(_, data)) => {
                transcript.push_str(&data);
                handshake.note_output(&manager, id, &transcript);
            }
            Ok(Event::Exit(got, _)) => panic!("shell {got} exited before kill"),
            Err(_) => break, // Quiet: warmed up.
        }
    }
    manager.kill(id).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(_, data)) => {
                transcript.push_str(&data);
                handshake.note_output(&manager, id, &transcript);
            }
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
    assert!(manager.snapshot(999).is_err());
}

#[test]
fn snapshot_repaints_live_pane_and_fails_after_kill() {
    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    // Bare interactive shell stays alive so the snapshot has a live screen.
    let id = manager.spawn(None, None, Vec::new(), 80, 24).unwrap();
    let mut handshake = Handshake::new();
    let mut transcript = String::new();
    // Warm up (answering the ConPTY handshake on Windows), then run echo.
    let warmup_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let timeout = warmup_deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            break;
        }
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(_, data)) => {
                transcript.push_str(&data);
                handshake.note_output(&manager, id, &transcript);
            }
            Ok(Event::Exit(got, _)) => panic!("shell {got} exited before snapshot"),
            Err(_) => break, // Quiet: warmed up.
        }
    }
    manager.write(id, "echo hello-snapshot\n").unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(_, data)) => {
                transcript.push_str(&data);
                handshake.note_output(&manager, id, &transcript);
                if transcript.contains("hello-snapshot") {
                    break;
                }
            }
            Ok(Event::Exit(got, _)) => panic!("shell {got} exited before echo"),
            Err(_) => panic!("timed out waiting for echo; got: {transcript:?}"),
        }
    }
    let snap = manager.snapshot(id).unwrap();
    assert!(
        snap.contains("hello-snapshot"),
        "snapshot should repaint visible output, got: {snap:?}"
    );

    manager.kill(id).unwrap();
    assert!(
        manager.snapshot(id).is_err(),
        "snapshot of a killed pane must fail"
    );
    // Drain until the reader thread reports the exit (no leaked session).
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Event::Exit(got, _)) => {
                assert_eq!(got, id);
                return;
            }
            Ok(Event::Output(_, _)) => {}
            Err(_) => panic!("timed out waiting for killed pane to exit"),
        }
    }
}

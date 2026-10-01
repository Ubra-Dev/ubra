//! Real-PTY integration tests for [`ubra_lib::pty_manager`]: spawn a process,
//! capture its output, and kill a live pane. No Tauri runtime involved.

use std::sync::mpsc;
use std::time::{Duration, Instant};
use ubra_lib::pty_manager::{PaneId, PtyEventSink, PtyManager, PtySessionInfo, SpawnOptions};

enum Event {
    Output(PaneId, String),
    Exit(PaneId, bool),
}

struct ChannelSink {
    tx: mpsc::Sender<Event>,
}

impl PtyEventSink for ChannelSink {
    fn output(&self, id: PaneId, data: String, _sequence: u64) {
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
/// report. In production xterm.js answers via `pty_write`; these tests
/// must play the terminal themselves, or the pane looks permanently
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
    let id = manager
        .spawn(SpawnOptions {
            shell,
            args,
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();

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

/// Panes advertise a color-capable terminal even when the spawner
/// carries a stale colorless environment.
#[test]
fn pty_spawn_advertises_color_capable_terminal() {
    fn env_command() -> (Option<String>, Vec<String>) {
        #[cfg(windows)]
        return (
            Some("cmd.exe".to_string()),
            vec![
                "/C".to_string(),
                "echo TERM=%TERM% COLORTERM=%COLORTERM% NO_COLOR=%NO_COLOR%".to_string(),
            ],
        );
        #[cfg(not(windows))]
        return (
            Some("sh".to_string()),
            vec![
                "-c".to_string(),
                "echo TERM=$TERM COLORTERM=$COLORTERM NO_COLOR=${NO_COLOR-unset}".to_string(),
            ],
        );
    }

    // Simulate a spawner launched from a colorless session: the pane
    // must not inherit any of this. Restored immediately after spawn,
    // which captures the child environment.
    let saved_term = std::env::var_os("TERM");
    let saved_colorterm = std::env::var_os("COLORTERM");
    let saved_no_color = std::env::var_os("NO_COLOR");
    std::env::set_var("TERM", "dumb");
    std::env::remove_var("COLORTERM");
    std::env::set_var("NO_COLOR", "1");

    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    let (shell, args) = env_command();
    let id = manager
        .spawn(SpawnOptions {
            shell,
            args,
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();

    match saved_term {
        Some(v) => std::env::set_var("TERM", v),
        None => std::env::remove_var("TERM"),
    }
    match saved_colorterm {
        Some(v) => std::env::set_var("COLORTERM", v),
        None => std::env::remove_var("COLORTERM"),
    }
    match saved_no_color {
        Some(v) => std::env::set_var("NO_COLOR", v),
        None => std::env::remove_var("NO_COLOR"),
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut transcript = String::new();
    let mut handshake = Handshake::new();
    loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(got, data)) => {
                assert_eq!(got, id);
                transcript.push_str(&data);
                handshake.note_output(&manager, id, &transcript);
            }
            Ok(Event::Exit(got, success)) => {
                assert_eq!(got, id);
                assert!(success, "env probe should exit 0");
                break;
            }
            Err(_) => panic!("timed out waiting for env probe; got: {transcript:?}"),
        }
    }
    assert!(
        transcript.contains("TERM=xterm-256color"),
        "pane should advertise xterm-256color, got: {transcript:?}"
    );
    assert!(
        transcript.contains("COLORTERM=truecolor"),
        "pane should advertise truecolor, got: {transcript:?}"
    );
    #[cfg(not(windows))]
    assert!(
        transcript.contains("NO_COLOR=unset"),
        "pane must not inherit NO_COLOR, got: {transcript:?}"
    );
    #[cfg(windows)]
    assert!(
        transcript.contains("NO_COLOR=%NO_COLOR%"),
        "pane must not inherit NO_COLOR, got: {transcript:?}"
    );
}

#[test]
fn pty_kill_terminates_live_pane() {
    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    // Bare interactive shell blocks on input until killed.
    let id = manager
        .spawn(SpawnOptions {
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
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
    let result = manager.spawn(SpawnOptions {
        shell: Some("ubra-no-such-binary-xyz".to_string()),
        cols: 80,
        rows: 24,
        ..Default::default()
    });
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
    let id = manager
        .spawn(SpawnOptions {
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
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
        snap.data.contains("hello-snapshot"),
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

/// A restarted frontend discovers its surviving sessions by stable key
/// and adopts them via snapshot instead of spawning replacements.
#[test]
fn keyed_sessions_list_and_reattach_after_frontend_restart() {
    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    let id = manager
        .spawn(SpawnOptions {
            cols: 80,
            rows: 24,
            key: Some("pane-aaa".to_string()),
            ..Default::default()
        })
        .unwrap();
    // Warm up, then produce output the reattach must recover.
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
            Ok(Event::Exit(got, _)) => panic!("shell {got} exited before reattach"),
            Err(_) => break, // Quiet: warmed up.
        }
    }
    manager.write(id, "echo hello-reattach\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(_, data)) => {
                transcript.push_str(&data);
                handshake.note_output(&manager, id, &transcript);
                if transcript.contains("hello-reattach") {
                    break;
                }
            }
            Ok(Event::Exit(got, _)) => panic!("shell {got} exited before echo"),
            Err(_) => panic!("timed out waiting for echo; got: {transcript:?}"),
        }
    }

    // The "restarted frontend" knows only the stable key, not the numeric id.
    let listed = manager.list();
    assert_eq!(
        listed,
        vec![PtySessionInfo {
            id,
            key: Some("pane-aaa".to_string()),
        }]
    );
    let adopted = listed
        .iter()
        .find(|s| s.key.as_deref() == Some("pane-aaa"))
        .unwrap()
        .id;
    let snap = manager.snapshot(adopted).unwrap();
    assert!(
        snap.data.contains("hello-reattach"),
        "adopted snapshot must repaint surviving output, got: {snap:?}"
    );

    // Keyed and unkeyed sessions coexist in one listing.
    let id2 = manager
        .spawn(SpawnOptions {
            cols: 80,
            rows: 24,
            key: Some("pane-bbb".into()),
            ..Default::default()
        })
        .unwrap();
    let id3 = manager
        .spawn(SpawnOptions {
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        manager.list(),
        vec![
            PtySessionInfo {
                id,
                key: Some("pane-aaa".into())
            },
            PtySessionInfo {
                id: id2,
                key: Some("pane-bbb".into())
            },
            PtySessionInfo { id: id3, key: None },
        ]
    );

    // Kills disappear from the list, so orphan sweeps observe the truth.
    manager.kill(id).unwrap();
    assert!(
        manager
            .list()
            .iter()
            .all(|s| s.key.as_deref() != Some("pane-aaa")),
        "killed key must leave the list"
    );
    manager.kill(id2).unwrap();
    manager.kill(id3).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut exits = 0;
    while exits < 3 {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Event::Exit(_, _)) => exits += 1,
            Ok(Event::Output(_, _)) => {}
            Err(_) => panic!("timed out waiting for killed panes to exit"),
        }
    }
}

#[test]
fn invalid_geometry_preserves_a_usable_session() {
    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    for (cols, rows) in [
        (0, 24),
        (1, 24),
        (80, 0),
        (1001, 24),
        (1000, 1000),
        (u16::MAX, u16::MAX),
    ] {
        assert!(manager
            .spawn(SpawnOptions {
                cols,
                rows,
                ..Default::default()
            })
            .is_err());
    }
    assert!(manager.pane_roots().is_empty());
    let id = manager
        .spawn(SpawnOptions {
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    for (cols, rows) in [(0, 24), (1, 24), (80, 0), (1000, 1000)] {
        assert!(manager.resize(id, cols, rows).is_err());
        let snapshot = manager.snapshot(id).unwrap();
        assert_eq!((snapshot.cols, snapshot.rows), (80, 24));
    }
    manager.resize(id, 2, 1).unwrap();
    manager.resize(id, 80, 24).unwrap();

    let mut handshake = Handshake::new();
    let mut output = String::new();
    let warmup_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let timeout = warmup_deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            break;
        }
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(got, data)) => {
                if got == id {
                    output.push_str(&data);
                    handshake.note_output(&manager, id, &output);
                }
            }
            Ok(Event::Exit(got, _)) => panic!("shell {got} exited before write"),
            Err(_) => break, // Quiet: warmed up.
        }
    }

    #[cfg(unix)]
    manager.write(id, "printf 'healthy-%s\\n' pane\n").unwrap();
    #[cfg(windows)]
    manager
        .write(id, "Write-Output ('healthy-' + 'pane')\r\n")
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !output.contains("healthy-pane") {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Event::Output(got, data)) => {
                if got == id {
                    output.push_str(&data);
                    handshake.note_output(&manager, got, &output);
                }
            }
            _ => panic!("rejected resize damaged session: {output:?}"),
        }
    }
    manager.kill(id).unwrap();
}

#[test]
fn close_terminates_resistant_child_but_preserves_sibling_pane() {
    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    #[cfg(unix)]
    let sibling = manager
        .spawn(SpawnOptions {
            shell: Some("sh".into()),
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    #[cfg(windows)]
    let sibling = manager
        .spawn(SpawnOptions {
            shell: Some("powershell.exe".into()),
            args: vec!["-NoProfile".into()],
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    #[cfg(unix)]
    let parent = manager
        .spawn(SpawnOptions {
            shell: Some("sh".into()),
            args: vec![
                "-c".into(),
                "trap '' HUP TERM; sh -c 'trap \"\" HUP TERM; echo CHILD:$$; while :; do sleep 1; done' & wait".into(),
            ],
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    #[cfg(windows)]
    let parent = manager
        .spawn(SpawnOptions {
            shell: Some("powershell.exe".into()),
            args: vec![
                "-NoProfile".into(),
                "-Command".into(),
                "$p = Start-Process powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep 300' -PassThru; [Console]::WriteLine('CHILD:' + $p.Id); Start-Sleep 300".into(),
            ],
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut output = String::new();
    let mut parent_handshake = Handshake::new();
    let mut sibling_handshake = Handshake::new();
    let mut sibling_output = String::new();
    let child = loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Event::Output(id, data)) if id == parent => {
                output.push_str(&data);
                parent_handshake.note_output(&manager, id, &output);
                if let Some(idx) = output.find("CHILD:") {
                    let rest = &output[idx + "CHILD:".len()..];
                    if let Some(num_str) = rest
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().next())
                    {
                        if let Ok(pid) = num_str.trim().parse::<i32>() {
                            break pid;
                        }
                    }
                }
            }
            Ok(Event::Output(id, data)) if id == sibling => {
                sibling_output.push_str(&data);
                sibling_handshake.note_output(&manager, id, &sibling_output);
            }
            Ok(_) => {}
            Err(_) => panic!("child did not start: {output:?}"),
        }
    };
    #[cfg(unix)]
    assert_eq!(
        unsafe { libc::getsid(child) },
        manager
            .pane_roots()
            .iter()
            .find(|p| p.id == parent)
            .unwrap()
            .root_pid as i32
    );
    manager.kill(parent).unwrap();
    let mut processes = sysinfo::System::new();
    let child_pid = sysinfo::Pid::from_u32(child as u32);
    let termination_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        processes.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        if processes
            .process(child_pid)
            .is_none_or(|p| p.status() == sysinfo::ProcessStatus::Zombie)
        {
            break;
        }
        assert!(
            Instant::now() < termination_deadline,
            "child process {child} remained alive after closing its pane"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let warmup_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let timeout = warmup_deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            break;
        }
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(id, data)) if id == sibling => {
                sibling_output.push_str(&data);
                sibling_handshake.note_output(&manager, id, &sibling_output);
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    #[cfg(unix)]
    manager
        .write(sibling, "printf 'sibling-%s\\n' alive\n")
        .unwrap();
    #[cfg(windows)]
    manager
        .write(sibling, "Write-Output ('sibling-' + 'alive')\r\n")
        .unwrap();
    let mut output = String::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !output.contains("sibling-alive") {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Event::Output(id, data)) if id == sibling => {
                output.push_str(&data);
                sibling_handshake.note_output(&manager, id, &output);
            }
            Ok(_) => {}
            Err(_) => panic!("closing another pane damaged sibling: {output:?}"),
        }
    }
    manager.kill(sibling).unwrap();
}

#[test]
fn shutdown_terminates_live_sessions_and_refuses_late_spawns() {
    let (tx, _rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    manager
        .spawn(SpawnOptions {
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    manager
        .spawn(SpawnOptions {
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    manager.shutdown().unwrap();
    assert!(
        manager.pane_roots().is_empty(),
        "live sessions escaped shutdown"
    );
    assert!(
        manager
            .spawn(SpawnOptions {
                cols: 80,
                rows: 24,
                ..Default::default()
            })
            .is_err(),
        "closed manager admitted another process"
    );
}

#[cfg(unix)]
#[test]
fn root_exit_releases_resistant_children_even_after_they_close_terminal_handles() {
    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ubra-exit-child-{:016x}",
        u64::from_ne_bytes(nonce)
    ));
    let script = format!(
        "trap '' HUP; sh -c 'trap \"\" HUP TERM; echo $$ > \"$1\"; exec sleep 300' sh '{}' </dev/null >/dev/null 2>&1 & \
         while [ ! -s '{}' ]; do sleep 0.01; done; printf 'CHILD:'; cat '{}'; exit 0",
        path.display(), path.display(), path.display()
    );
    let id = manager
        .spawn(SpawnOptions {
            shell: Some("sh".into()),
            args: vec!["-c".into(), script],
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut output = String::new();
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Event::Output(_, data)) => output.push_str(&data),
            Ok(Event::Exit(got, success)) => {
                assert_eq!(got, id);
                assert!(success);
                break;
            }
            Err(_) => panic!("root exit was not delivered: {output:?}"),
        }
    }
    let child = std::fs::read_to_string(&path)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let mut processes = sysinfo::System::new();
    processes.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    assert!(
        processes
            .process(sysinfo::Pid::from_u32(child))
            .is_none_or(|p| p.status() == sysinfo::ProcessStatus::Zombie),
        "orphaned child survived root exit"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn invalid_cwd_fails_without_a_session_while_a_valid_sibling_runs() {
    let (tx, rx) = mpsc::channel();
    let manager = PtyManager::new(std::sync::Arc::new(ChannelSink { tx }));
    let missing = std::env::temp_dir().join(format!(
        "ubra-missing-cwd-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let error = manager
        .spawn(SpawnOptions {
            shell: Some("sh".to_string()),
            cwd: Some(missing.to_string_lossy().into_owned()),
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("Working directory is unavailable:"),
        "unexpected error: {error}"
    );
    let file_cwd = std::env::temp_dir().join(format!("ubra-file-cwd-{}", std::process::id()));
    std::fs::write(&file_cwd, b"not a directory").unwrap();
    let error = manager
        .spawn(SpawnOptions {
            shell: Some("sh".to_string()),
            cwd: Some(file_cwd.to_string_lossy().into_owned()),
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("Working directory is not a directory:"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_file(&file_cwd);
    assert!(manager.pane_roots().is_empty());
    let (shell, args) = echo_command();
    let id = manager
        .spawn(SpawnOptions {
            shell,
            args,
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut transcript = String::new();
    let mut handshake = Handshake::new();
    loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(Event::Output(got, data)) => {
                assert_eq!(got, id);
                transcript.push_str(&data);
                handshake.note_output(&manager, id, &transcript);
            }
            Ok(Event::Exit(got, success)) => {
                assert_eq!(got, id);
                assert!(success, "sibling pane should exit 0");
                break;
            }
            Err(_) => panic!("timed out waiting for sibling pane; got: {transcript:?}"),
        }
    }
    assert!(
        transcript.contains("hello-pty"),
        "sibling should run, got: {transcript:?}"
    );
}

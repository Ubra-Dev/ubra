//! Agent detection against real processes: a fake agent (an executable named
//! like a known CLI) running in a real PTY must scan as unknown, while a
//! plain shell scans as idle.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use ubra_lib::agent_watch::{poll_once, poll_once_with_grace, PaneAgent};
use ubra_lib::pty_manager::{PaneId, PtyEventSink, PtyManager};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Sink;
impl PtyEventSink for Sink {
    fn output(&self, _id: PaneId, _data: String, _sequence: u64) {}
    fn exited(&self, _id: PaneId, _success: bool, _code: Option<i32>) {}
}

fn scratch_dir() -> std::path::PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ubra-agent-test-{}-{id}", std::process::id()))
}

/// Create a long-running executable named like a known agent CLI. Returns
/// (program, args) to spawn it in a pane.
#[cfg(unix)]
fn fake_agent(dir: &std::path::Path, name: &str) -> (String, Vec<String>) {
    fake_agent_with_output(dir, name, "")
}

/// Fake agent that prints `output` (no single quotes) before idling, so
/// screen-based detection has something to match.
#[cfg(unix)]
fn fake_agent_with_output(
    dir: &std::path::Path,
    name: &str,
    output: &str,
) -> (String, Vec<String>) {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(
        &path,
        format!("#!/bin/sh\nprintf '%s' '{output}'\nsleep 60\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (path.to_string_lossy().into_owned(), Vec::new())
}

/// Windows cannot run extensionless scripts; copy a real exe under an
/// agent-like name instead.
#[cfg(windows)]
fn fake_agent(dir: &std::path::Path, name: &str) -> (String, Vec<String>) {
    let dest = dir.join(format!("{name}.exe"));
    let system32 = std::env::var("SystemRoot")
        .map(|r| format!("{r}\\System32\\timeout.exe"))
        .unwrap();
    std::fs::copy(&system32, &dest).unwrap();
    (
        dest.to_string_lossy().into_owned(),
        vec!["/t".to_string(), "60".to_string()],
    )
}

/// Equality ignoring the reported cwd (which depends on the test runner).
fn same_state_ignoring_cwd(a: &PaneAgent, b: &PaneAgent) -> bool {
    use PaneAgent::*;
    match (a, b) {
        (Idle, Idle) => true,
        (
            Working {
                agent: a1, cli: c1, ..
            },
            Working {
                agent: a2, cli: c2, ..
            },
        )
        | (
            Blocked {
                agent: a1, cli: c1, ..
            },
            Blocked {
                agent: a2, cli: c2, ..
            },
        )
        | (
            Unknown {
                agent: a1, cli: c1, ..
            },
            Unknown {
                agent: a2, cli: c2, ..
            },
        ) => a1 == a2 && c1 == c2,
        _ => false,
    }
}

/// Poll with `poll` until the pane scans as `expected`, or panic.
fn expect_with(
    pane: PaneId,
    expected: PaneAgent,
    mut poll: impl FnMut() -> BTreeMap<PaneId, PaneAgent>,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let states = poll();
        if states
            .get(&pane)
            .is_some_and(|s| same_state_ignoring_cwd(s, &expected))
        {
            return;
        }
        if Instant::now() > deadline {
            panic!("pane {pane:?} never scanned as {expected:?} (states: {states:?})");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Poll until the pane scans as `expected`, or panic.
fn expect_state(manager: &PtyManager, pane: PaneId, expected: PaneAgent, grace_secs: u64) {
    expect_with(pane, expected, || poll_once_with_grace(manager, grace_secs));
}

/// Poll until the pane scans as unknown with the expected label + CLI, or
/// panic. Uses zero starting grace so detection itself is deterministic;
/// the default grace path is covered by `detects_agent_process_in_pane`.
fn expect_unknown(manager: &PtyManager, pane: PaneId, agent: &str, cli: &str) {
    expect_state(
        manager,
        pane,
        PaneAgent::Unknown {
            agent: agent.to_string(),
            cli: cli.to_string(),
            cwd: None,
        },
        0,
    );
}

#[test]
fn detects_agent_process_in_pane() {
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let (program, args) = fake_agent(&dir, "codex");

    let manager = PtyManager::new(std::sync::Arc::new(Sink));
    let agent_pane = manager.spawn(Some(program), None, args, 80, 24).unwrap();
    let shell_pane = manager.spawn(None, None, Vec::new(), 80, 24).unwrap();
    assert_ne!(manager.pane_roots().len(), 0);

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let states = poll_once(&manager);
        let agent_state = states.get(&agent_pane);
        let shell_state = states.get(&shell_pane);
        let working = PaneAgent::Unknown {
            agent: "Codex".to_string(),
            cli: "codex".to_string(),
            cwd: None,
        };
        if agent_state.is_some_and(|s| same_state_ignoring_cwd(s, &working))
            && shell_state == Some(&PaneAgent::Idle)
        {
            break;
        }
        if Instant::now() > deadline {
            panic!("agent pane never scanned as unknown (states: {states:?})");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    // The unknown state must carry the agent process's cwd.
    let states = poll_once(&manager);
    match states.get(&agent_pane) {
        Some(PaneAgent::Unknown { cwd: Some(_), .. }) => {}
        other => panic!("unknown state must carry agent cwd, got: {other:?}"),
    }

    manager.kill(agent_pane).unwrap();
    manager.kill(shell_pane).unwrap();
    let states = poll_once(&manager);
    assert!(
        !states.contains_key(&agent_pane) && !states.contains_key(&shell_pane),
        "killed panes must leave the scan, got: {states:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_versioned_muse_binary_in_pane() {
    // The `muse` launcher execs a versioned `muse-bin-<version>-<build>`
    // binary; a pane running it must scan as unknown "Muse".
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let (program, args) = fake_agent(&dir, "muse-bin-1.4.1-R4503.1");

    let manager = PtyManager::new(std::sync::Arc::new(Sink));
    let agent_pane = manager.spawn(Some(program), None, args, 80, 24).unwrap();

    expect_unknown(&manager, agent_pane, "Muse", "muse");

    manager.kill(agent_pane).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn process_age_never_implies_working() {
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let (program, args) = fake_agent(&dir, "codex");

    let manager = PtyManager::new(std::sync::Arc::new(Sink));
    let agent_pane = manager.spawn(Some(program), None, args, 80, 24).unwrap();

    // A huge grace keeps the fresh process unknown; zero grace still reads unknown.
    expect_state(
        &manager,
        agent_pane,
        PaneAgent::Unknown {
            agent: "Codex".to_string(),
            cli: "codex".to_string(),
            cwd: None,
        },
        u64::MAX,
    );
    expect_state(
        &manager,
        agent_pane,
        PaneAgent::Unknown {
            agent: "Codex".to_string(),
            cli: "codex".to_string(),
            cwd: None,
        },
        0,
    );

    manager.kill(agent_pane).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Send a signal to a pid via the `kill` command (unix-only tests).
#[cfg(unix)]
fn signal(pid: u32, sig: &str) {
    let status = std::process::Command::new("kill")
        .arg(format!("-{sig}"))
        .arg(pid.to_string())
        .status()
        .unwrap();
    assert!(status.success(), "kill -{sig} {pid} failed");
}

/// A suspended (SIGSTOP) agent process scans as blocked until resumed.
#[cfg(unix)]
#[test]
fn stopped_agent_reports_blocked_until_resumed() {
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let (program, args) = fake_agent(&dir, "codex");

    let manager = PtyManager::new(std::sync::Arc::new(Sink));
    let agent_pane = manager.spawn(Some(program), None, args, 80, 24).unwrap();
    let root_pid = manager
        .pane_roots()
        .iter()
        .find(|p| p.id == agent_pane)
        .unwrap()
        .root_pid;

    let working = PaneAgent::Unknown {
        agent: "Codex".to_string(),
        cli: "codex".to_string(),
        cwd: None,
    };
    let blocked = PaneAgent::Blocked {
        agent: "Codex".to_string(),
        cli: "codex".to_string(),
        cwd: None,
    };
    expect_state(&manager, agent_pane, working.clone(), 0);
    signal(root_pid, "STOP");
    expect_state(&manager, agent_pane, blocked, 0);
    signal(root_pid, "CONT");
    expect_state(&manager, agent_pane, working, 0);

    manager.kill(agent_pane).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A fake agent whose first output is an approval prompt scans as blocked
/// via the bundled screen rule (unix: needs a script preamble).
#[cfg(unix)]
#[test]
fn approval_screen_reports_blocked() {
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let (program, args) = fake_agent_with_output(
        &dir,
        "claude",
        "Do you want to proceed?\n❯ 1. Yes\n  2. No\nEsc to cancel\n",
    );

    let manager = PtyManager::new(std::sync::Arc::new(Sink));
    let agent_pane = manager.spawn(Some(program), None, args, 80, 24).unwrap();

    let mut watcher = ubra_lib::agent_watch::Watcher::bundled();
    expect_with(
        agent_pane,
        PaneAgent::Blocked {
            agent: "Claude Code".to_string(),
            cli: "claude".to_string(),
            cwd: None,
        },
        || watcher.poll_with_grace(&manager, 0),
    );

    manager.kill(agent_pane).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `<cli>.toml` override adds rules for agents without bundled ones, and
/// `reload_rules` picks up edits (unix: needs a script preamble).
#[cfg(unix)]
#[test]
fn detection_override_file_adds_rules() {
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let (program, args) = fake_agent_with_output(&dir, "codex", "Shall I run this?\n> yes / no\n");
    let rules_dir = dir.join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join("codex.toml"),
        "[[blocked]]\nid = \"custom\"\ncontains = [\"shall i run this\", \"> yes\"]\n",
    )
    .unwrap();

    let manager = PtyManager::new(std::sync::Arc::new(Sink));
    let agent_pane = manager.spawn(Some(program), None, args, 80, 24).unwrap();

    let mut watcher = ubra_lib::agent_watch::Watcher::with_dir(rules_dir.clone());
    expect_with(
        agent_pane,
        PaneAgent::Blocked {
            agent: "Codex".to_string(),
            cli: "codex".to_string(),
            cwd: None,
        },
        || watcher.poll_with_grace(&manager, 0),
    );

    // Rewrite with a non-matching rule and reload: back to unknown.
    std::fs::write(
        rules_dir.join("codex.toml"),
        "[[blocked]]\nid = \"custom\"\ncontains = [\"nothing like this\"]\n",
    )
    .unwrap();
    watcher.reload_rules();
    expect_with(
        agent_pane,
        PaneAgent::Unknown {
            agent: "Codex".to_string(),
            cli: "codex".to_string(),
            cwd: None,
        },
        || watcher.poll_with_grace(&manager, 0),
    );

    manager.kill(agent_pane).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

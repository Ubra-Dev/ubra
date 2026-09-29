//! Agent detection against real processes: a fake agent (an executable named
//! like a known CLI) running in a real PTY must scan as working, while a
//! plain shell scans as idle.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use ubra_lib::agent_watch::{poll_once, PaneAgent};
use ubra_lib::pty_manager::{PaneId, PtyEventSink, PtyManager};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Sink;
impl PtyEventSink for Sink {
    fn output(&self, _id: PaneId, _data: String) {}
    fn exited(&self, _id: PaneId, _success: bool, _code: Option<i32>) {}
}

fn scratch_dir() -> std::path::PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ubra-agent-test-{}-{id}", std::process::id()))
}

/// Create a long-running executable named like a known agent CLI. Returns
/// (program, args) to spawn it in a pane.
#[cfg(unix)]
fn fake_agent(dir: &std::path::Path) -> (String, Vec<String>) {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("codex");
    std::fs::write(&path, "#!/bin/sh\nsleep 60\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (path.to_string_lossy().into_owned(), Vec::new())
}

/// Windows cannot run extensionless scripts; copy a real exe under an
/// agent-like name instead.
#[cfg(windows)]
fn fake_agent(dir: &std::path::Path) -> (String, Vec<String>) {
    let dest = dir.join("codex.exe");
    let system32 = std::env::var("SystemRoot")
        .map(|r| format!("{r}\\System32\\timeout.exe"))
        .unwrap();
    std::fs::copy(&system32, &dest).unwrap();
    (
        dest.to_string_lossy().into_owned(),
        vec!["/t".to_string(), "60".to_string()],
    )
}

#[test]
fn detects_agent_process_in_pane() {
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let (program, args) = fake_agent(&dir);

    let manager = PtyManager::new(std::sync::Arc::new(Sink));
    let agent_pane = manager.spawn(Some(program), None, args, 80, 24).unwrap();
    let shell_pane = manager.spawn(None, None, Vec::new(), 80, 24).unwrap();
    assert_ne!(manager.pane_roots().len(), 0);

    let mut sys = sysinfo::System::new_all();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let states = poll_once(&manager, &mut sys);
        let agent_state = states.get(&agent_pane);
        let shell_state = states.get(&shell_pane);
        if agent_state
            == Some(&PaneAgent::Working {
                agent: "Codex".to_string(),
            })
            && shell_state == Some(&PaneAgent::Idle)
        {
            break;
        }
        if Instant::now() > deadline {
            panic!("agent pane never scanned as working (states: {states:?})");
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    manager.kill(agent_pane).unwrap();
    manager.kill(shell_pane).unwrap();
    let states = poll_once(&manager, &mut sys);
    assert!(
        !states.contains_key(&agent_pane) && !states.contains_key(&shell_pane),
        "killed panes must leave the scan, got: {states:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

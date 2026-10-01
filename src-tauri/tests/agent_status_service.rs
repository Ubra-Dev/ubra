//! Real PTYs and the production worker; scripts only simulate renderer markers.
//! These are not captured CLI sessions and do not invoke any model service.
#![cfg(unix)]
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use ubra_lib::agent_status::{AgentStatusService, AgentUpdate};
use ubra_lib::pty_manager::{PtyEventSink, PtyManager, SpawnOptions};

struct Sink;
impl PtyEventSink for Sink {
    fn output(&self, _: u32, _: String, _: u64) {}
    fn exited(&self, _: u32, _: bool, _: Option<i32>) {}
}
struct Fixture {
    dir: std::path::PathBuf,
    manager: Arc<PtyManager>,
    service: AgentStatusService,
    updates: mpsc::Receiver<AgentUpdate>,
    pane: u32,
}
impl Fixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "ubra-status-service-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("codex");
        std::fs::write(
            &program,
            r#"#!/bin/sh
stty -echo
printf '\033[2J\033[HREADY_MARKER\n'
while IFS= read -r command; do
 case "$command" in
 busy) printf '\033[2J\033[HBUSY_MARKER\n' ;;
 ready) printf '\033[2J\033[HREADY_MARKER\n' ;;
 block) printf '\033[2J\033[HAPPROVAL_MARKER\n' ;;
 exit) exit 0 ;;
 fail) exit 1 ;;
 esac
done
"#,
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.join("codex.toml"), "[[working]]\nid='fixture-busy'\ncontains=['BUSY_MARKER']\n[[idle]]\nid='fixture-ready'\ncontains=['READY_MARKER']\n[[blocked]]\nid='fixture-approval'\ncontains=['APPROVAL_MARKER']\n").unwrap();
        let manager = Arc::new(PtyManager::new(Arc::new(Sink)));
        let (tx, updates) = mpsc::channel();
        let service = AgentStatusService::start(&manager, Some(dir.clone()), move |update| {
            let _ = tx.send(update);
        });
        let pane = manager
            .spawn(SpawnOptions {
                shell: Some(program.to_string_lossy().into_owned()),
                cwd: Some(dir.to_string_lossy().into_owned()),
                cols: 80,
                rows: 24,
                ..Default::default()
            })
            .unwrap();
        Self {
            dir,
            manager,
            service,
            updates,
            pane,
        }
    }
    fn state(&self, wanted: &str) -> AgentUpdate {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let snapshot = self.service.snapshot();
            if snapshot
                .states
                .get(&self.pane)
                .is_some_and(|s| s.state.state_key() == wanted)
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "expected {wanted}, got {snapshot:?}"
            );
            self.service
                .wait_for_revision(snapshot.revision, Duration::from_millis(100));
        }
    }
    fn send(&self, command: &str) {
        self.manager
            .write(self.pane, &format!("{command}\n"))
            .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.manager.kill(self.pane);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn queries_resize_and_redraws_do_not_restart_completed_tasks() {
    let fixture = Fixture::new();
    let initial = fixture.state("idle");
    assert!(initial.states[&fixture.pane].state.identity().is_some());
    assert!(initial.transitions.is_empty());
    fixture.send("busy");
    fixture.state("working");
    fixture.send("block");
    fixture.state("blocked");
    fixture.send("busy");
    fixture.state("working");
    fixture.send("ready");
    let completed = fixture.state("done");
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..1000 {
                    let cached = fixture.service.snapshot();
                    assert_eq!(cached.revision, completed.revision);
                    assert!(cached.transitions.is_empty());
                    assert_eq!(cached.states[&fixture.pane].state.state_key(), "done");
                }
            });
        }
    });
    fixture.manager.resize(fixture.pane, 80, 24).unwrap();
    fixture.manager.resize(fixture.pane, 100, 30).unwrap();
    fixture.send("ready");
    std::thread::sleep(Duration::from_millis(350));
    assert_eq!(
        fixture.service.snapshot().states[&fixture.pane]
            .state
            .state_key(),
        "done"
    );
    let transitions: Vec<_> = fixture
        .updates
        .try_iter()
        .flat_map(|update| update.transitions)
        .collect();
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].kind, "task-completed");
    fixture.manager.kill(fixture.pane).unwrap();
    std::thread::sleep(Duration::from_millis(250));
    assert!(!fixture
        .service
        .snapshot()
        .states
        .contains_key(&fixture.pane));
    assert!(fixture
        .updates
        .try_iter()
        .all(|update| update.transitions.is_empty()));
}

#[test]
fn direct_successful_exit_finishes_once_and_failure_needs_attention() {
    for (command, kind) in [("exit", "task-completed"), ("fail", "agent-stopped")] {
        let fixture = Fixture::new();
        fixture.state("idle");
        fixture.send("busy");
        fixture.state("working");
        fixture.send(command);
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut transitions = Vec::new();
        loop {
            if let Ok(update) = fixture.updates.recv_timeout(Duration::from_millis(100)) {
                transitions.extend(update.transitions);
            }
            if !transitions.is_empty() && fixture.service.snapshot().states.is_empty() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "missing exit notification for {command}"
            );
        }
        std::thread::sleep(Duration::from_millis(250));
        transitions.extend(
            fixture
                .updates
                .try_iter()
                .flat_map(|update| update.transitions),
        );
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].kind, kind);
    }
}

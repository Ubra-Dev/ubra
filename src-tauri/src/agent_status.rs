//! One owner for status history. Client queries only read cached snapshots.
use crate::agent_watch::{Observation, PaneAgent, Watcher};
use crate::pty_manager::{AgentExit, PaneId, PtyManager};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const STABLE_EVIDENCE: Duration = Duration::from_millis(200);
const LOST_EVIDENCE: Duration = Duration::from_millis(750);
const WORKER_TICK: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    #[serde(flatten)]
    pub state: PaneAgent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_instance_id: Option<String>,
    pub reason: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTransition {
    pub event_id: String,
    pub pane_id: PaneId,
    pub agent_instance_id: String,
    pub kind: String,
    pub agent: String,
    pub cli: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AgentUpdate {
    pub revision: u64,
    pub states: BTreeMap<PaneId, AgentStatus>,
    pub transitions: Vec<AgentTransition>,
}

struct History {
    observation: Observation,
    confirmed: AgentStatus,
    candidate: Option<(String, String, Duration)>,
    unresolved_task: bool,
    completed: bool,
}

#[derive(Default)]
struct Tracker {
    histories: HashMap<PaneId, History>,
    disappearing: HashMap<PaneId, (History, Duration)>,
    event_sequence: u64,
}

impl Tracker {
    fn transition(&mut self, id: PaneId, h: &History, kind: &str) -> Option<AgentTransition> {
        let (agent, cli, cwd) = h.confirmed.state.identity()?;
        let instance = h.confirmed.agent_instance_id.clone()?;
        self.event_sequence += 1;
        Some(AgentTransition {
            event_id: format!("{instance}:{}", self.event_sequence),
            pane_id: id,
            agent_instance_id: instance,
            kind: kind.into(),
            agent: agent.into(),
            cli: cli.into(),
            cwd: cwd.map(Into::into),
        })
    }

    /// `now` is elapsed monotonic time; tests supply it without sleeping.
    fn update(
        &mut self,
        mut observations: BTreeMap<PaneId, Observation>,
        exits: Vec<AgentExit>,
        now: Duration,
    ) -> (BTreeMap<PaneId, AgentStatus>, Vec<AgentTransition>) {
        let mut transitions = Vec::new();
        for exit in exits {
            // Observations and reaping are separate snapshots. Never resurrect
            // an instance from evidence collected just before its exit.
            observations.remove(&exit.id);
            if let Some(h) = self
                .histories
                .remove(&exit.id)
                .or_else(|| self.disappearing.remove(&exit.id).map(|(h, _)| h))
            {
                if exit.deliberate {
                    continue;
                }
                let direct = h.observation.matched_pid == Some(exit.root_pid);
                let kind = if direct && exit.success == Some(true) && !h.completed {
                    Some("task-completed")
                } else if (direct && exit.success == Some(false))
                    || h.unresolved_task
                    || h.confirmed.state.state_key() == "blocked"
                {
                    Some("agent-stopped")
                } else {
                    None
                };
                if let Some(kind) = kind {
                    if let Some(event) = self.transition(exit.id, &h, kind) {
                        transitions.push(event);
                    }
                }
            }
        }
        // A direct CLI may vanish from sysinfo before its PTY reader reaps
        // the exit result. Hold identity briefly so success is attributable.
        let restore: Vec<_> = self
            .disappearing
            .iter()
            .filter_map(|(id, (h, _))| {
                (observations.get(id).and_then(|o| o.instance_id.as_ref())
                    == h.observation.instance_id.as_ref())
                .then_some(*id)
            })
            .collect();
        for id in restore {
            let (h, _) = self.disappearing.remove(&id).unwrap();
            self.histories.insert(id, h);
        }
        let removed: Vec<_> = self
            .histories
            .iter()
            .filter_map(|(id, h)| {
                let instance = observations.get(id).and_then(|o| o.instance_id.as_ref());
                (instance != h.observation.instance_id.as_ref()).then_some(*id)
            })
            .collect();
        for id in removed {
            let h = self.histories.remove(&id).unwrap();
            let replacement = observations
                .get(&id)
                .is_some_and(|o| o.instance_id.is_some());
            if h.observation.matched_pid == Some(h.observation.root_pid) && !replacement {
                self.disappearing.insert(id, (h, now));
            } else if h.unresolved_task || h.confirmed.state.state_key() == "blocked" {
                if let Some(event) = self.transition(id, &h, "agent-stopped") {
                    transitions.push(event);
                }
            }
        }
        let expired: Vec<_> = self
            .disappearing
            .iter()
            .filter_map(|(id, (_, since))| {
                (now.saturating_sub(*since) >= Duration::from_millis(1200)
                    || observations
                        .get(id)
                        .is_some_and(|o| o.instance_id.is_some()))
                .then_some(*id)
            })
            .collect();
        for id in expired {
            let (h, _) = self.disappearing.remove(&id).unwrap();
            if h.unresolved_task || h.confirmed.state.state_key() == "blocked" {
                if let Some(event) = self.transition(id, &h, "agent-stopped") {
                    transitions.push(event);
                }
            }
        }
        let mut states = BTreeMap::new();
        for (id, observation) in observations {
            if observation.instance_id.is_none() {
                if let Some((h, _)) = self.disappearing.get(&id) {
                    let (agent, cli, cwd) = h.confirmed.state.identity().unwrap();
                    let unknown = PaneAgent::Unknown {
                        agent: agent.into(),
                        cli: cli.into(),
                        cwd: cwd.map(Into::into),
                    };
                    states.insert(
                        id,
                        status(&h.observation, unknown, "process:awaiting-exit-result"),
                    );
                } else {
                    states.insert(
                        id,
                        status(&observation, observation.state.clone(), &observation.reason),
                    );
                }
                continue;
            }
            let h = self.histories.entry(id).or_insert_with(|| {
                let initial = match observation.state.identity() {
                    Some((agent, cli, cwd)) => PaneAgent::Unknown {
                        agent: agent.into(),
                        cli: cli.into(),
                        cwd: cwd.map(Into::into),
                    },
                    None => PaneAgent::Idle,
                };
                History {
                    observation: observation.clone(),
                    confirmed: status(&observation, initial, "awaiting-stable-evidence"),
                    candidate: None,
                    unresolved_task: false,
                    completed: false,
                }
            });
            let raw = observation.state.state_key();
            let confirmed = h.confirmed.state.state_key();
            let target = if raw == "idle" && h.completed {
                "done"
            } else {
                raw
            };
            let previous = confirmed.to_string();
            let mut completion = false;
            if raw == "blocked" || target == confirmed {
                h.candidate = None;
                h.confirmed = status(
                    &observation,
                    if target == "done" {
                        observation.state.as_done()
                    } else {
                        observation.state.clone()
                    },
                    &observation.reason,
                );
            } else {
                let candidate = h
                    .candidate
                    .get_or_insert_with(|| (target.into(), observation.reason.clone(), now));
                if candidate.0 != target || candidate.1 != observation.reason {
                    *candidate = (target.into(), observation.reason.clone(), now);
                }
                let threshold = if raw == "unknown" {
                    LOST_EVIDENCE
                } else {
                    STABLE_EVIDENCE
                };
                if now.saturating_sub(candidate.2) >= threshold {
                    let mut next = observation.state.clone();
                    if raw == "working" {
                        h.unresolved_task = true;
                        h.completed = false;
                    } else if raw == "idle" && h.unresolved_task {
                        h.unresolved_task = false;
                        h.completed = true;
                        completion = true;
                        next = next.as_done();
                    } else if target == "done" {
                        next = next.as_done();
                    }
                    h.confirmed = status(&observation, next, &observation.reason);
                    h.candidate = None;
                }
            }
            h.observation = observation;
            let current = h.confirmed.clone();
            if previous != current.state.state_key() {
                eprintln!(
                    "ubra: agent status pane={id} instance={} {previous}->{} reason={}",
                    current.agent_instance_id.as_deref().unwrap_or("none"),
                    current.state.state_key(),
                    current.reason
                );
            }
            states.insert(id, current);
            if completion {
                // Release the entry borrow before assigning the event id.
                let h = self.histories.remove(&id).unwrap();
                if let Some(event) = self.transition(id, &h, "task-completed") {
                    transitions.push(event);
                }
                self.histories.insert(id, h);
            }
        }
        (states, transitions)
    }
}

fn status(o: &Observation, state: PaneAgent, reason: &str) -> AgentStatus {
    AgentStatus {
        state,
        agent_instance_id: o.instance_id.clone(),
        reason: reason.into(),
        source: if reason.starts_with("process:") || reason == "no-agent-process" {
            "process"
        } else if reason.starts_with("screen:") {
            "screen"
        } else {
            "unknown"
        }
        .into(),
    }
}

struct Shared {
    latest: Mutex<AgentUpdate>,
    changed: Condvar,
    reload: AtomicBool,
    stopped: AtomicBool,
}

pub struct AgentStatusService {
    shared: Arc<Shared>,
    manager: std::sync::Weak<PtyManager>,
}

impl AgentStatusService {
    pub fn start(
        manager: &Arc<PtyManager>,
        rules_dir: Option<PathBuf>,
        publish: impl Fn(AgentUpdate) + Send + 'static,
    ) -> Self {
        let activity = manager
            .take_activity_receiver()
            .expect("one status worker per PTY manager");
        let shared = Arc::new(Shared {
            latest: Mutex::new(AgentUpdate::default()),
            changed: Condvar::new(),
            reload: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        });
        let weak_manager = Arc::downgrade(manager);
        let service = Self {
            shared: shared.clone(),
            manager: weak_manager.clone(),
        };
        std::thread::Builder::new()
            .name("ubra-agent-status".into())
            .spawn(move || {
                let mut watcher = match rules_dir {
                    Some(dir) => Watcher::with_dir(dir),
                    None => Watcher::bundled(),
                };
                let mut tracker = Tracker::default();
                let started = Instant::now();
                // A fixed 100ms cadence coalesces bursts without starvation from
                // continuously arriving output. Process discovery remains 1Hz.
                loop {
                    if shared.stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    let Some(manager) = weak_manager.upgrade() else {
                        break;
                    };
                    if shared.reload.swap(false, Ordering::SeqCst) {
                        watcher.reload_rules();
                    }
                    let observations = watcher.observe(&manager);
                    let exits = manager.drain_agent_exits();
                    let (states, transitions) =
                        tracker.update(observations, exits, started.elapsed());
                    drop(manager);
                    let update = {
                        let mut latest = shared.latest.lock().unwrap();
                        if latest.states != states || !transitions.is_empty() {
                            *latest = AgentUpdate {
                                revision: latest.revision + 1,
                                states,
                                transitions,
                            };
                            shared.changed.notify_all();
                            Some(latest.clone())
                        } else {
                            None
                        }
                    };
                    if let Some(update) = update {
                        publish(update);
                    }
                    let deadline = Instant::now() + WORKER_TICK;
                    loop {
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        if remaining.is_zero() || shared.stopped.load(Ordering::SeqCst) {
                            break;
                        }
                        if activity.recv_timeout(remaining).is_err() {
                            break;
                        }
                    }
                }
            })
            .expect("spawn shared status worker");
        service
    }

    pub fn snapshot(&self) -> AgentUpdate {
        let mut snapshot = self.shared.latest.lock().unwrap().clone();
        // A query is a snapshot, not a replay of the latest notification.
        snapshot.transitions.clear();
        snapshot
    }

    pub fn reload_rules(&self) {
        self.shared.reload.store(true, Ordering::SeqCst);
        if let Some(manager) = self.manager.upgrade() {
            manager.wake_status();
        }
    }

    pub fn wait_for_revision(&self, revision: u64, timeout: Duration) {
        let latest = self.shared.latest.lock().unwrap();
        let _ = self
            .shared
            .changed
            .wait_timeout_while(latest, timeout, |latest| {
                latest.revision <= revision && !self.shared.stopped.load(Ordering::SeqCst)
            })
            .unwrap();
    }
}

impl Drop for AgentStatusService {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        self.shared.changed.notify_all();
        if let Some(manager) = self.manager.upgrade() {
            manager.wake_status();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(state: &str, instance: &str) -> Observation {
        let (agent, cli, cwd) = ("Agent".into(), "test-cli".into(), None);
        let state = match state {
            "working" => PaneAgent::Working { agent, cli, cwd },
            "blocked" => PaneAgent::Blocked { agent, cli, cwd },
            "idle" => PaneAgent::Ready { agent, cli, cwd },
            _ => PaneAgent::Unknown { agent, cli, cwd },
        };
        Observation {
            reason: format!("screen:{}", state.state_key()),
            state,
            instance_id: Some(instance.into()),
            root_pid: 10,
            matched_pid: Some(11),
        }
    }

    fn tick(
        t: &mut Tracker,
        state: &str,
        ms: u64,
    ) -> (BTreeMap<u32, AgentStatus>, Vec<AgentTransition>) {
        t.update(
            BTreeMap::from([(1, observation(state, "instance-a"))]),
            vec![],
            Duration::from_millis(ms),
        )
    }

    #[test]
    fn explicit_busy_then_ready_completes_once_and_redraws_do_not_restart() {
        let mut t = Tracker::default();
        tick(&mut t, "idle", 0);
        assert_eq!(tick(&mut t, "idle", 200).0[&1].state.state_key(), "idle");
        tick(&mut t, "working", 300);
        assert_eq!(
            tick(&mut t, "working", 500).0[&1].state.state_key(),
            "working"
        );
        assert_eq!(
            tick(&mut t, "working", 60_000).0[&1].state.state_key(),
            "working"
        );
        tick(&mut t, "idle", 60_100);
        let (states, events) = tick(&mut t, "idle", 60_300);
        assert_eq!(states[&1].state.state_key(), "done");
        assert_eq!(events.len(), 1);
        for ms in [60_400, 70_000, 90_000] {
            let (states, events) = tick(&mut t, "idle", ms);
            assert_eq!(states[&1].state.state_key(), "done");
            assert!(events.is_empty());
        }
    }

    #[test]
    fn queries_cannot_advance_time_or_invent_completion() {
        let mut t = Tracker::default();
        tick(&mut t, "working", 0);
        for _ in 0..1000 {
            assert_eq!(
                tick(&mut t, "working", 0).0[&1].state.state_key(),
                "unknown"
            );
        }
        tick(&mut t, "working", 200);
        for ms in [1000, 6000, 60_000] {
            assert!(tick(&mut t, "working", ms).1.is_empty());
        }
    }

    #[test]
    fn partial_redraw_preserves_status_then_unknown_never_means_finished() {
        let mut t = Tracker::default();
        tick(&mut t, "working", 0);
        tick(&mut t, "working", 200);
        assert_eq!(
            tick(&mut t, "unknown", 300).0[&1].state.state_key(),
            "working"
        );
        assert_eq!(
            tick(&mut t, "working", 500).0[&1].state.state_key(),
            "working"
        );
        tick(&mut t, "unknown", 600);
        let (states, events) = tick(&mut t, "unknown", 1350);
        assert_eq!(states[&1].state.state_key(), "unknown");
        assert!(events.is_empty());
    }

    #[test]
    fn blocked_without_observed_task_then_ready_is_idle() {
        let mut t = Tracker::default();
        assert_eq!(
            tick(&mut t, "blocked", 0).0[&1].state.state_key(),
            "blocked"
        );
        tick(&mut t, "idle", 100);
        let (states, events) = tick(&mut t, "idle", 300);
        assert_eq!(states[&1].state.state_key(), "idle");
        assert!(events.is_empty());
    }

    #[test]
    fn replacement_resets_task_and_marks_unresolved_stop_once() {
        let mut t = Tracker::default();
        tick(&mut t, "working", 0);
        tick(&mut t, "working", 200);
        let obs = BTreeMap::from([(1, observation("idle", "instance-b"))]);
        let (_, events) = t.update(obs.clone(), vec![], Duration::from_millis(300));
        assert_eq!(events[0].kind, "agent-stopped");
        let (states, events) = t.update(obs, vec![], Duration::from_millis(500));
        assert_eq!(states[&1].state.state_key(), "idle");
        assert!(events.is_empty());
    }

    #[test]
    fn shell_success_is_not_agent_success_and_deliberate_close_is_silent() {
        for deliberate in [false, true] {
            let mut t = Tracker::default();
            tick(&mut t, "working", 0);
            tick(&mut t, "working", 200);
            let (_, events) = t.update(
                BTreeMap::new(),
                vec![AgentExit {
                    id: 1,
                    root_pid: 10,
                    success: Some(true),
                    deliberate,
                }],
                Duration::from_secs(1),
            );
            if deliberate {
                assert!(events.is_empty());
            } else {
                assert_eq!(events[0].kind, "agent-stopped");
            }
        }
    }

    #[test]
    fn disappearance_before_reaping_retains_direct_exit_result() {
        let mut t = Tracker::default();
        let mut o = observation("working", "direct");
        o.matched_pid = Some(10);
        t.update(BTreeMap::from([(1, o.clone())]), vec![], Duration::ZERO);
        t.update(
            BTreeMap::from([(1, o.clone())]),
            vec![],
            Duration::from_millis(200),
        );
        assert!(t
            .update(BTreeMap::new(), vec![], Duration::from_millis(300))
            .1
            .is_empty());
        let (_, events) = t.update(
            BTreeMap::from([(1, o)]),
            vec![AgentExit {
                id: 1,
                root_pid: 10,
                success: Some(true),
                deliberate: false,
            }],
            Duration::from_millis(1000),
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "task-completed");
        assert!(
            t.histories.is_empty(),
            "stale observations must not resurrect a reaped agent"
        );
    }

    #[test]
    fn missing_direct_exit_result_eventually_marks_unresolved_stop() {
        let mut t = Tracker::default();
        let mut o = observation("working", "direct");
        o.matched_pid = Some(10);
        t.update(BTreeMap::from([(1, o.clone())]), vec![], Duration::ZERO);
        t.update(BTreeMap::from([(1, o)]), vec![], Duration::from_millis(200));
        assert!(t
            .update(BTreeMap::new(), vec![], Duration::from_millis(300))
            .1
            .is_empty());
        let (_, events) = t.update(BTreeMap::new(), vec![], Duration::from_millis(1500));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "agent-stopped");
        assert!(t
            .update(BTreeMap::new(), vec![], Duration::from_millis(2000))
            .1
            .is_empty());
    }

    #[test]
    fn direct_exit_has_attributable_success_and_reaped_events_are_not_replayed() {
        let mut t = Tracker::default();
        let mut o = observation("unknown", "direct");
        o.matched_pid = Some(10);
        t.update(BTreeMap::from([(1, o)]), vec![], Duration::ZERO);
        let (_, events) = t.update(
            BTreeMap::new(),
            vec![AgentExit {
                id: 1,
                root_pid: 10,
                success: Some(true),
                deliberate: false,
            }],
            Duration::from_secs(1),
        );
        assert_eq!(events[0].kind, "task-completed");
        assert!(t
            .update(BTreeMap::new(), vec![], Duration::from_secs(2))
            .1
            .is_empty());
    }
}

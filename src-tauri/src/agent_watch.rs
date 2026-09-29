//! Agent detection (Phase 3): identify the coding-agent CLI running in each
//! live pane by walking the process tree below the pane's root process.
//!
//! Matching is deliberately conservative: a process counts as an agent only
//! when its executable stem, process name, argv[0] stem, or interpreted-script
//! target exactly equals a known agent binary name (case-insensitive,
//! `.exe`-tolerant). Wrapper scripts named like an agent match via the
//! script-target rule (see [`script_target`][self::script_target]).
//! The one exception is versioned binaries such as Muse's
//! `muse-bin-<version>-<build>`, which match by prefix (see [`match_agent`]).

use crate::pty_manager::PaneId;
use crate::screen_rules::{is_screen_blocked, DetectionRules};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use sysinfo::{
    Pid, Process, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind,
};

/// Known agent binaries: (binary stem, display label).
///
/// Covers the popular coding-agent CLIs, plus a few extras (Aider, Goose,
/// Crush). Stems with two spellings (Qoder, Antigravity) cover both the bare
/// binary name and the integration-slug spelling, since either may be the
/// process name on a given machine.
pub const AGENT_TABLE: &[(&str, &str)] = &[
    ("claude", "Claude Code"),
    ("codex", "Codex"),
    ("cursor-agent", "Cursor Agent CLI"),
    ("opencode", "OpenCode"),
    ("gemini", "Gemini CLI"),
    ("copilot", "GitHub Copilot CLI"),
    ("amp", "Amp"),
    ("aider", "Aider"),
    ("goose", "Goose"),
    ("crush", "Crush"),
    ("droid", "Droid"),
    ("qwen", "Qwen Code"),
    ("grok", "Grok CLI"),
    ("kiro", "Kiro CLI"),
    ("cline", "Cline"),
    ("devin", "Devin CLI"),
    ("muse", "Muse"),
    ("pi", "Pi"),
    ("omp", "OMP"),
    ("kimi", "Kimi Code CLI"),
    ("hermes", "Hermes Agent"),
    ("qoder", "Qoder CLI"),
    ("qodercli", "Qoder CLI"),
    ("letta", "Letta Code"),
    ("kilo", "Kilo Code CLI"),
    ("mastracode", "MastraCode"),
    ("antigravity", "Antigravity CLI"),
    ("antigravity-cli", "Antigravity CLI"),
    ("maki", "Maki"),
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PaneAgent {
    Working {
        agent: String,
        cli: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// The agent process is suspended (SIGTSTP/Ctrl+Z): alive but unable to
    /// progress until resumed. Deliberately narrow — screen-based approval
    /// detection is a later phase.
    Blocked {
        agent: String,
        cli: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// An agent process matched within the starting-grace window: present,
    /// but not yet observed in a steady state.
    Unknown {
        agent: String,
        cli: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// The agent process is alive but its screen has been silent for
    /// [`DONE_CALM_POLLS`] consecutive polls after previously producing
    /// output: an interactive agent sitting at its input prompt. Only the
    /// [`Watcher::poll_with_grace`] post-pass produces this; [`scan_panes`]
    /// never does.
    Done {
        agent: String,
        cli: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    Idle,
}

/// Seconds a freshly-seen agent process reports [`PaneAgent::Unknown`]
/// before [`PaneAgent::Working`]. Covers launcher warmup (version checks,
/// updates) before the agent is actually running.
pub const STARTING_GRACE_SECS: u64 = 5;

/// Consecutive polls with a byte-identical screen that flip a previously
/// active [`PaneAgent::Working`] pane to [`PaneAgent::Done`]. At the 2s poll
/// interval this is ~6s of terminal silence: streaming tokens, spinners, and
/// progress lines all change the screen, so a calm screen means the agent is
/// sitting at its input prompt.
pub const DONE_CALM_POLLS: u32 = 3;

/// A live pane and the PID of the process spawned directly in its PTY.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneRoots {
    pub id: PaneId,
    pub root_pid: u32,
}

/// Match an executable/process-name stem against the agent table.
///
/// Returns the canonical `(cli, label)` pair: `cli` is the binary name from
/// [`AGENT_TABLE`] (what the user typed), `label` is the display name.
/// Besides exact matches, recognizes versioned `muse-bin-*` binaries: the
/// `muse` launcher is a shell script that execs `muse-bin-<version>-<build>`
/// (e.g. `muse-bin-1.4.1-R4503.1`), so the running agent process never bears
/// the bare `muse` name; those map to canonical cli `muse`.
pub fn match_agent(stem: &str) -> Option<(&'static str, &'static str)> {
    let lower = stem.to_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    if let Some(&(cli, label)) = AGENT_TABLE.iter().find(|(name, _)| *name == stem) {
        return Some((cli, label));
    }
    if stem == "muse-bin" || stem.starts_with("muse-bin-") {
        return Some(("muse", "Muse"));
    }
    None
}

/// Script interpreters whose argv[1] names the executed script.
const INTERPRETERS: &[&str] = &[
    "sh", "bash", "dash", "zsh", "ksh", "fish", "python", "python3", "node", "deno", "bun", "ruby",
    "perl",
];

fn stem_of(path: &std::ffi::OsStr) -> Option<&str> {
    Path::new(path).file_stem().and_then(|s| s.to_str())
}

/// When a process runs a script through an interpreter (`sh ./codex`,
/// `node ./claude`, `python -m aider`), return the script/module stem.
/// Returns `None` for flags (`-c`, `-e`) and non-interpreter commands so that
/// e.g. `vim claude.md` never matches.
fn script_target(cmd: &[std::ffi::OsString]) -> Option<&str> {
    let argv0 = stem_of(cmd.first()?)?;
    if argv0.eq_ignore_ascii_case("env") {
        // /usr/bin/env [-flags] <interp> <script...>
        let mut args = cmd.iter().skip(1).filter_map(|s| s.to_str());
        for arg in args.by_ref() {
            if arg.contains('=') || arg.starts_with('-') {
                continue;
            }
            let _interp = arg;
            let script = args.next()?;
            return stem_of(std::ffi::OsStr::new(script));
        }
        return None;
    }
    if !INTERPRETERS.contains(&argv0.to_lowercase().as_str()) {
        return None;
    }
    let script = cmd.get(1)?.to_str()?;
    if script == "-m" {
        let module = cmd.get(2)?.to_str()?;
        return stem_of(std::ffi::OsStr::new(module));
    }
    if script.starts_with('-') {
        return None;
    }
    stem_of(std::ffi::OsStr::new(script))
}

fn identify(proc: &Process) -> Option<(&'static str, &'static str)> {
    if let Some(exe) = proc.exe() {
        if let Some(stem) = exe.file_stem().and_then(|s| s.to_str()) {
            if let Some(found) = match_agent(stem) {
                return Some(found);
            }
        }
    }
    if let Some(found) = match_agent(&proc.name().to_string_lossy()) {
        return Some(found);
    }
    if let Some(argv0) = proc.cmd().first() {
        if let Some(stem) = stem_of(argv0) {
            let stem = stem.strip_prefix('-').unwrap_or(stem); // login shells: "-zsh"
            if let Some(found) = match_agent(stem) {
                return Some(found);
            }
        }
    }
    if let Some(script) = script_target(proc.cmd()) {
        if let Some(found) = match_agent(script) {
            return Some(found);
        }
    }
    None
}

/// A matched agent process plus the signals used to classify it.
struct AgentMatch {
    cli: &'static str,
    label: &'static str,
    status: ProcessStatus,
    run_time_secs: u64,
    /// Working directory of the matched agent process, when readable.
    cwd: Option<String>,
}

/// Classify a match: suspended processes are blocked, approval UI on the
/// live screen is blocked (and beats the starting grace: visible UI means
/// initialized), processes younger than the grace are unknown, everything
/// else is working.
fn classify(
    m: &AgentMatch,
    screen: Option<&str>,
    rules: &DetectionRules,
    grace_secs: u64,
) -> PaneAgent {
    let (agent, cli, cwd) = (m.label.to_string(), m.cli.to_string(), m.cwd.clone());
    if m.status == ProcessStatus::Stop {
        return PaneAgent::Blocked { agent, cli, cwd };
    }
    if screen.is_some_and(|s| is_screen_blocked(rules, m.cli, s)) {
        return PaneAgent::Blocked { agent, cli, cwd };
    }
    if m.run_time_secs < grace_secs {
        return PaneAgent::Unknown { agent, cli, cwd };
    }
    PaneAgent::Working { agent, cli, cwd }
}

/// Scan all panes against a process snapshot, screen snapshots by pane id,
/// and detection rules. Panes whose root PID is unknown (0) or absent
/// report [`PaneAgent::Idle`]; matches younger than `grace_secs` report
/// [`PaneAgent::Unknown`].
pub fn scan_panes(
    sys: &System,
    panes: &[PaneRoots],
    screens: &HashMap<PaneId, String>,
    rules: &DetectionRules,
    grace_secs: u64,
) -> BTreeMap<PaneId, PaneAgent> {
    let processes = sys.processes();
    let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, proc) in processes {
        if let Some(parent) = proc.parent() {
            children.entry(parent).or_default().push(*pid);
        }
    }
    panes
        .iter()
        .map(|pane| {
            let agent = if pane.root_pid == 0 {
                None
            } else {
                find_agent(processes, &children, Pid::from_u32(pane.root_pid))
            };
            let screen = screens.get(&pane.id).map(String::as_str);
            let state = agent.map_or(PaneAgent::Idle, |m| classify(&m, screen, rules, grace_secs));
            (pane.id, state)
        })
        .collect()
}

fn find_agent(
    processes: &HashMap<Pid, Process>,
    children: &HashMap<Pid, Vec<Pid>>,
    root: Pid,
) -> Option<AgentMatch> {
    let mut seen = HashSet::new();
    let mut queue = vec![root];
    while let Some(pid) = queue.pop() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(proc) = processes.get(&pid) {
            if let Some((cli, label)) = identify(proc) {
                // Effectively-dead matches are not agents: keep searching so
                // an exiting agent never flashes blocked.
                match proc.status() {
                    ProcessStatus::Zombie | ProcessStatus::Dead => {}
                    status => {
                        return Some(AgentMatch {
                            cli,
                            label,
                            status,
                            run_time_secs: proc.run_time(),
                            cwd: proc.cwd().map(|p| p.to_string_lossy().into_owned()),
                        });
                    }
                }
            }
        }
        if let Some(kids) = children.get(&pid) {
            queue.extend(kids.iter().copied());
        }
    }
    None
}

fn snapshot_and_scan(
    manager: &crate::pty_manager::PtyManager,
    rules: &DetectionRules,
    grace_secs: u64,
) -> (BTreeMap<PaneId, PaneAgent>, HashMap<PaneId, String>) {
    // The snapshot is rebuilt per poll rather than refreshed in place:
    // sysinfo only records process status at discovery, so a reused
    // `System` would report a suspended (SIGSTOP) agent as working forever.
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet),
    );
    let roots = manager.pane_roots();
    let screens: HashMap<PaneId, String> = roots
        .iter()
        .filter_map(|p| manager.screen_text(p.id).map(|t| (p.id, t)))
        .collect();
    let states = scan_panes(&sys, &roots, &screens, rules, grace_secs);
    (states, screens)
}

/// Per-pane screen-activity memory for the done post-pass.
#[derive(Debug, Clone, Copy, Default)]
struct CalmState {
    /// Hash of the screen at the previous poll.
    hash: u64,
    /// Consecutive polls the screen hash was unchanged.
    calm_polls: u32,
    /// Whether the screen changed at least once while tracked. Gates
    /// [`PaneAgent::Done`]: a fresh REPL that never produced output stays
    /// working instead of reporting a finish it never started.
    saw_output: bool,
}

fn screen_hash(screen: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    screen.hash(&mut hasher);
    hasher.finish()
}

/// Flip long-calm [`PaneAgent::Working`] panes to [`PaneAgent::Done`].
///
/// Only working panes with a captured screen participate: blocked/unknown
/// panes keep their tracking untouched (their states already describe them),
/// panes without a screen fail toward working, and idle/vanished panes are
/// pruned so a recycled pane id never inherits another pane's calm.
fn apply_calm(
    calm: &mut HashMap<PaneId, CalmState>,
    states: &mut BTreeMap<PaneId, PaneAgent>,
    screens: &HashMap<PaneId, String>,
) {
    for (id, state) in states.iter_mut() {
        let PaneAgent::Working { agent, cli, cwd } = state else {
            if matches!(state, PaneAgent::Idle) {
                calm.remove(id);
            }
            continue;
        };
        let Some(screen) = screens.get(id) else {
            calm.remove(id);
            continue;
        };
        let hash = screen_hash(screen);
        let entry = calm.entry(*id).or_insert(CalmState {
            hash,
            calm_polls: 0,
            saw_output: false,
        });
        if entry.hash == hash {
            entry.calm_polls += 1;
        } else {
            // Screen changed since the previous poll: fresh output.
            *entry = CalmState {
                hash,
                calm_polls: 0,
                saw_output: true,
            };
        }
        if entry.saw_output && entry.calm_polls >= DONE_CALM_POLLS {
            *state = PaneAgent::Done {
                agent: std::mem::take(agent),
                cli: std::mem::take(cli),
                cwd: cwd.take(),
            };
        }
    }
    calm.retain(|id, _| states.contains_key(id));
}

/// Poll with bundled detection rules only.
pub fn poll_once(manager: &crate::pty_manager::PtyManager) -> BTreeMap<PaneId, PaneAgent> {
    Watcher::bundled().poll(manager)
}

/// Poll with bundled rules and an explicit starting grace (tests).
pub fn poll_once_with_grace(
    manager: &crate::pty_manager::PtyManager,
    grace_secs: u64,
) -> BTreeMap<PaneId, PaneAgent> {
    Watcher::bundled().poll_with_grace(manager, grace_secs)
}

/// The agent watcher: polls panes against process + screen snapshots,
/// honoring a rules dir for local detection overrides.
pub struct Watcher {
    rules_dir: Option<PathBuf>,
    rules: DetectionRules,
    calm: HashMap<PaneId, CalmState>,
}

impl Watcher {
    /// Bundled detection rules only.
    pub fn bundled() -> Self {
        Self {
            rules_dir: None,
            rules: DetectionRules::bundled(),
            calm: HashMap::new(),
        }
    }

    /// Bundled rules plus `<dir>/<cli>.toml` overrides, loaded now.
    pub fn with_dir(rules_dir: PathBuf) -> Self {
        let rules = DetectionRules::load(&rules_dir);
        Self {
            rules_dir: Some(rules_dir),
            rules,
            calm: HashMap::new(),
        }
    }

    /// Explicit rules (tests).
    pub fn with_rules(rules: DetectionRules) -> Self {
        Self {
            rules_dir: None,
            rules,
            calm: HashMap::new(),
        }
    }

    /// Re-read the rules dir (no-op without one). Control commands can
    /// trigger this; the startup load covers the common case.
    pub fn reload_rules(&mut self) {
        if let Some(dir) = &self.rules_dir {
            self.rules = DetectionRules::load(dir);
        }
    }

    /// Take a fresh snapshot and scan every live pane of the manager.
    pub fn poll(
        &mut self,
        manager: &crate::pty_manager::PtyManager,
    ) -> BTreeMap<PaneId, PaneAgent> {
        self.poll_with_grace(manager, STARTING_GRACE_SECS)
    }

    /// Snapshot and scan with an explicit starting grace.
    pub fn poll_with_grace(
        &mut self,
        manager: &crate::pty_manager::PtyManager,
        grace_secs: u64,
    ) -> BTreeMap<PaneId, PaneAgent> {
        let (mut states, screens) = snapshot_and_scan(manager, &self.rules, grace_secs);
        apply_calm(&mut self.calm, &mut states, &screens);
        states
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_calm, classify, match_agent, script_target, AgentMatch, CalmState, PaneAgent,
        DONE_CALM_POLLS,
    };
    use crate::screen_rules::DetectionRules;
    use std::collections::{BTreeMap, HashMap};
    use std::ffi::OsString;
    use sysinfo::ProcessStatus;

    fn cmd(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn matched(status: ProcessStatus, run_time_secs: u64) -> AgentMatch {
        AgentMatch {
            cli: "codex",
            label: "Codex",
            status,
            run_time_secs,
            cwd: None,
        }
    }

    fn classify_plain(m: &AgentMatch, grace_secs: u64) -> PaneAgent {
        classify(m, None, &DetectionRules::bundled(), grace_secs)
    }

    #[test]
    fn classify_suspended_is_blocked() {
        // Suspension wins over age: a young stopped process is blocked.
        for run_time_secs in [0, 3, 30] {
            assert_eq!(
                classify_plain(&matched(ProcessStatus::Stop, run_time_secs), 5),
                PaneAgent::Blocked {
                    agent: "Codex".to_string(),
                    cli: "codex".to_string(),
                    cwd: None,
                }
            );
        }
    }

    #[test]
    fn classify_young_is_unknown_until_grace_expires() {
        assert_eq!(
            classify_plain(&matched(ProcessStatus::Sleep, 0), 5),
            PaneAgent::Unknown {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: None,
            }
        );
        assert_eq!(
            classify_plain(&matched(ProcessStatus::Run, 4), 5),
            PaneAgent::Unknown {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: None,
            }
        );
        // Grace boundary is inclusive: run_time == grace reads working.
        assert_eq!(
            classify_plain(&matched(ProcessStatus::Sleep, 5), 5),
            PaneAgent::Working {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: None,
            }
        );
        assert_eq!(
            classify_plain(&matched(ProcessStatus::Run, 3600), 5),
            PaneAgent::Working {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: None,
            }
        );
    }

    #[test]
    fn classify_carries_agent_cwd() {
        let m = AgentMatch {
            cwd: Some("/tmp/proj".to_string()),
            ..matched(ProcessStatus::Sleep, 30)
        };
        assert_eq!(
            classify_plain(&m, 5),
            PaneAgent::Working {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: Some("/tmp/proj".to_string()),
            }
        );
    }

    #[test]
    fn classify_screen_blocked_beats_grace() {
        // Visible approval UI means initialized, even within the grace window.
        let screen = "Do you want to proceed?\n❯ 1. Yes";
        let m = AgentMatch {
            cli: "claude",
            label: "Claude Code",
            ..matched(ProcessStatus::Sleep, 0)
        };
        assert_eq!(
            classify(&m, Some(screen), &DetectionRules::bundled(), 5),
            PaneAgent::Blocked {
                agent: "Claude Code".to_string(),
                cli: "claude".to_string(),
                cwd: None,
            }
        );
    }

    #[test]
    fn classify_screen_without_evidence_falls_back_to_age() {
        let rules = DetectionRules::bundled();
        assert_eq!(
            classify_plain(&matched(ProcessStatus::Sleep, 1), 5),
            PaneAgent::Unknown {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: None,
            }
        );
        assert_eq!(
            classify(
                &matched(ProcessStatus::Sleep, 30),
                Some("working…"),
                &rules,
                5
            ),
            PaneAgent::Working {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: None,
            }
        );
    }

    fn working_states() -> BTreeMap<u32, PaneAgent> {
        BTreeMap::from([(
            1,
            PaneAgent::Working {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: Some("/tmp/proj".to_string()),
            },
        )])
    }

    fn screens_for(screen: &str) -> HashMap<u32, String> {
        HashMap::from([(1, screen.to_string())])
    }

    fn done_states() -> BTreeMap<u32, PaneAgent> {
        BTreeMap::from([(
            1,
            PaneAgent::Done {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: Some("/tmp/proj".to_string()),
            },
        )])
    }

    #[test]
    fn calm_flips_working_to_done_after_silence() {
        let mut calm: HashMap<u32, CalmState> = HashMap::new();
        // First sight records the hash; the change marks output seen.
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("prompt v1"));
        assert!(matches!(states[&1], PaneAgent::Working { .. }));
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("streaming…"));
        assert!(matches!(states[&1], PaneAgent::Working { .. }));
        // Silence: still working until the threshold poll.
        for _ in 0..DONE_CALM_POLLS - 1 {
            let mut states = working_states();
            apply_calm(&mut calm, &mut states, &screens_for("streaming…"));
            assert!(matches!(states[&1], PaneAgent::Working { .. }));
        }
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("streaming…"));
        assert_eq!(states, done_states());
    }

    #[test]
    fn calm_requires_prior_output() {
        // A fresh REPL that never produced output stays working: no finish
        // without a start, and no spurious "finished" notification.
        let mut calm: HashMap<u32, CalmState> = HashMap::new();
        for _ in 0..DONE_CALM_POLLS + 2 {
            let mut states = working_states();
            apply_calm(&mut calm, &mut states, &screens_for("fresh prompt"));
            assert!(matches!(states[&1], PaneAgent::Working { .. }));
        }
    }

    #[test]
    fn calm_resets_on_new_output() {
        let mut calm: HashMap<u32, CalmState> = HashMap::new();
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("a"));
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("b"));
        for _ in 0..DONE_CALM_POLLS {
            let mut states = working_states();
            apply_calm(&mut calm, &mut states, &screens_for("b"));
        }
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("b"));
        assert_eq!(states, done_states());
        // Fresh output flips back to working; silence re-arms done.
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("c"));
        assert!(matches!(states[&1], PaneAgent::Working { .. }));
        for _ in 0..DONE_CALM_POLLS {
            let mut states = working_states();
            apply_calm(&mut calm, &mut states, &screens_for("c"));
        }
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("c"));
        assert_eq!(states, done_states());
    }

    #[test]
    fn calm_leaves_non_working_states_alone_and_prunes() {
        let mut calm: HashMap<u32, CalmState> = HashMap::new();
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("a"));
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("b"));
        assert!(calm.contains_key(&1));
        // Blocked keeps its state (and its tracking): approval UI already
        // describes the pane better than calm does.
        let mut states = BTreeMap::from([(
            1,
            PaneAgent::Blocked {
                agent: "Codex".to_string(),
                cli: "codex".to_string(),
                cwd: None,
            },
        )]);
        apply_calm(&mut calm, &mut states, &screens_for("b"));
        assert!(matches!(states[&1], PaneAgent::Blocked { .. }));
        assert!(calm.contains_key(&1));
        // No screen capture fails toward working and drops tracking.
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &HashMap::new());
        assert!(matches!(states[&1], PaneAgent::Working { .. }));
        assert!(!calm.contains_key(&1));
        // Idle and vanished panes are pruned so recycled ids start clean.
        calm.insert(
            1,
            CalmState {
                hash: 7,
                calm_polls: 9,
                saw_output: true,
            },
        );
        let mut states = BTreeMap::from([(1, PaneAgent::Idle)]);
        apply_calm(&mut calm, &mut states, &screens_for("b"));
        assert!(!calm.contains_key(&1));
        calm.insert(
            2,
            CalmState {
                hash: 7,
                calm_polls: 9,
                saw_output: true,
            },
        );
        let mut states = working_states();
        apply_calm(&mut calm, &mut states, &screens_for("b"));
        assert!(!calm.contains_key(&2));
    }

    #[test]
    fn done_serializes_as_done_state() {
        // Frontend contract: the snapshot carries state:"done".
        let json = serde_json::to_string(&done_states()).unwrap();
        assert!(json.contains(r#""state":"done""#), "{json}");
        assert!(json.contains(r#""cli":"codex""#), "{json}");
    }

    #[test]
    fn known_agents_match() {
        assert_eq!(match_agent("claude"), Some(("claude", "Claude Code")));
        assert_eq!(match_agent("codex"), Some(("codex", "Codex")));
        assert_eq!(
            match_agent("cursor-agent"),
            Some(("cursor-agent", "Cursor Agent CLI"))
        );
        assert_eq!(match_agent("opencode"), Some(("opencode", "OpenCode")));
        assert_eq!(
            match_agent("copilot"),
            Some(("copilot", "GitHub Copilot CLI"))
        );
        assert_eq!(match_agent("muse"), Some(("muse", "Muse")));
        assert_eq!(match_agent("pi"), Some(("pi", "Pi")));
        assert_eq!(match_agent("omp"), Some(("omp", "OMP")));
    }

    #[test]
    fn extended_agents_match() {
        // Every agent beyond the original core table.
        assert_eq!(match_agent("kimi"), Some(("kimi", "Kimi Code CLI")));
        assert_eq!(match_agent("hermes"), Some(("hermes", "Hermes Agent")));
        assert_eq!(match_agent("qoder"), Some(("qoder", "Qoder CLI")));
        assert_eq!(match_agent("qodercli"), Some(("qodercli", "Qoder CLI")));
        assert_eq!(match_agent("letta"), Some(("letta", "Letta Code")));
        assert_eq!(match_agent("kilo"), Some(("kilo", "Kilo Code CLI")));
        assert_eq!(
            match_agent("mastracode"),
            Some(("mastracode", "MastraCode"))
        );
        assert_eq!(
            match_agent("antigravity"),
            Some(("antigravity", "Antigravity CLI"))
        );
        assert_eq!(
            match_agent("antigravity-cli"),
            Some(("antigravity-cli", "Antigravity CLI"))
        );
        assert_eq!(match_agent("maki"), Some(("maki", "Maki")));
    }

    #[test]
    fn matching_is_case_insensitive_and_exe_tolerant() {
        assert_eq!(match_agent("Claude"), Some(("claude", "Claude Code")));
        assert_eq!(match_agent("codex.exe"), Some(("codex", "Codex")));
        assert_eq!(match_agent("CODEX.EXE"), Some(("codex", "Codex")));
        assert_eq!(match_agent("Pi.EXE"), Some(("pi", "Pi")));
        assert_eq!(match_agent("OMP"), Some(("omp", "OMP")));
    }

    #[test]
    fn unknown_processes_do_not_match() {
        for name in [
            "sh",
            "zsh",
            "bash",
            "vim",
            "node",
            "npm",
            "sleep",
            "python3",
            "",
            "my-claude-notes",
            "muse-notes",
            "muse-binary",
        ] {
            assert_eq!(match_agent(name), None, "unexpected match for {name:?}");
        }
    }

    #[test]
    fn versioned_muse_binaries_match() {
        // The `muse` launcher execs a versioned binary; the running process
        // never bears the bare `muse` name.
        assert_eq!(
            match_agent("muse-bin-1.4.1-R4503.1"),
            Some(("muse", "Muse"))
        );
        // `file_stem` strips the trailing dotted component from exe/argv0.
        assert_eq!(match_agent("muse-bin-1.4.1-R4503"), Some(("muse", "Muse")));
        assert_eq!(match_agent("muse-bin"), Some(("muse", "Muse")));
        assert_eq!(match_agent("MUSE-BIN-2.0.0"), Some(("muse", "Muse")));
        assert_eq!(match_agent("muse-bin-1.2.3.exe"), Some(("muse", "Muse")));
    }

    #[test]
    fn script_target_finds_interpreted_agents() {
        assert_eq!(
            script_target(&cmd(&["/bin/sh", "/opt/bin/codex"])),
            Some("codex")
        );
        assert_eq!(
            script_target(&cmd(&["node", "/usr/local/bin/claude", "--resume"])),
            Some("claude")
        );
        assert_eq!(
            script_target(&cmd(&["/usr/bin/env", "node", "/x/gemini"])),
            Some("gemini")
        );
        assert_eq!(
            script_target(&cmd(&["python3", "-m", "aider"])),
            Some("aider")
        );
    }

    #[test]
    fn script_target_ignores_flags_and_plain_commands() {
        assert_eq!(script_target(&cmd(&["/bin/sh", "-c", "echo hi"])), None);
        assert_eq!(script_target(&cmd(&["python3", "-c", "x"])), None);
        assert_eq!(script_target(&cmd(&["vim", "claude.md"])), None);
        assert_eq!(script_target(&cmd(&["npm", "run", "dev"])), None);
        assert_eq!(script_target(&cmd(&["/bin/zsh"])), None);
        assert_eq!(script_target(&[]), None);
    }
}

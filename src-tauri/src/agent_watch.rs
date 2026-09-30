//! Agent detection: identify the coding-agent CLI running in each
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
use crate::screen_rules::{DetectionRules, ScreenState};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
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
    ("mimo", "MiMoCode"),
    ("agy", "Antigravity CLI"),
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
    /// progress until resumed, or a recognized approval/question screen.
    Blocked {
        agent: String,
        cli: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// An agent process is present without recognized activity evidence.
    Unknown {
        agent: String,
        cli: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// An observed task returned to a recognized input prompt.
    Done {
        agent: String,
        cli: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// A live CLI at its input prompt, before an observed task.
    #[serde(rename = "idle")]
    Ready {
        agent: String,
        cli: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    Idle,
}

// Kept for API compatibility. Age alone never establishes activity.
pub const STARTING_GRACE_SECS: u64 = 5;

impl PaneAgent {
    pub fn state_key(&self) -> &'static str {
        match self {
            Self::Working { .. } => "working",
            Self::Blocked { .. } => "blocked",
            Self::Unknown { .. } => "unknown",
            Self::Done { .. } => "done",
            Self::Ready { .. } | Self::Idle => "idle",
        }
    }

    pub fn identity(&self) -> Option<(&str, &str, Option<&str>)> {
        match self {
            Self::Working { agent, cli, cwd }
            | Self::Blocked { agent, cli, cwd }
            | Self::Unknown { agent, cli, cwd }
            | Self::Done { agent, cli, cwd }
            | Self::Ready { agent, cli, cwd } => Some((agent, cli, cwd.as_deref())),
            Self::Idle => None,
        }
    }

    pub fn as_done(&self) -> Self {
        match self.identity() {
            Some((agent, cli, cwd)) => Self::Done {
                agent: agent.into(),
                cli: cli.into(),
                cwd: cwd.map(Into::into),
            },
            None => Self::Idle,
        }
    }
}

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
#[derive(Clone)]
struct AgentMatch {
    cli: &'static str,
    label: &'static str,
    status: ProcessStatus,
    pid: u32,
    start_time: u64,
    /// Working directory of the matched agent process, when readable.
    cwd: Option<String>,
}

/// Classification requires explicit screen evidence; neither age nor CPU
/// scheduling status means the agent has started a task.
fn classify(
    m: &AgentMatch,
    screen: Option<&str>,
    rules: &DetectionRules,
    _grace_secs: u64,
) -> (PaneAgent, String) {
    let (agent, cli, cwd) = (m.label.to_string(), m.cli.to_string(), m.cwd.clone());
    if m.status == ProcessStatus::Stop {
        return (
            PaneAgent::Blocked { agent, cli, cwd },
            "process:suspended".into(),
        );
    }
    match screen.and_then(|s| rules.evidence(m.cli, s)) {
        Some(evidence) => {
            let state = match evidence.kind {
                ScreenState::Blocked => PaneAgent::Blocked { agent, cli, cwd },
                ScreenState::Working => PaneAgent::Working { agent, cli, cwd },
                ScreenState::Idle => PaneAgent::Ready { agent, cli, cwd },
            };
            (state, format!("screen:{}", evidence.rule_id))
        }
        None => (
            PaneAgent::Unknown { agent, cli, cwd },
            "no-recognized-evidence".into(),
        ),
    }
}

/// Raw evidence includes process identity so recycled PIDs and new agents
/// cannot inherit completion history.
#[derive(Debug, Clone)]
pub struct Observation {
    pub state: PaneAgent,
    pub instance_id: Option<String>,
    pub matched_pid: Option<u32>,
    pub root_pid: u32,
    pub reason: String,
}

/// Scan all panes against a process snapshot, screen snapshots by pane id,
/// and detection rules. Panes whose root PID is unknown (0) or absent
/// report [`PaneAgent::Idle`]. Matched agents without explicit evidence
/// report [`PaneAgent::Unknown`], irrespective of `grace_secs`.
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
            let state = agent.map_or(PaneAgent::Idle, |m| {
                classify(&m, screen, rules, grace_secs).0
            });
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
                            pid: pid.as_u32(),
                            start_time: proc.start_time(),
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

/// Poll once without task history (compatibility helper for process tests).
pub fn poll_once(manager: &crate::pty_manager::PtyManager) -> BTreeMap<PaneId, PaneAgent> {
    Watcher::bundled().poll(manager)
}

pub fn poll_once_with_grace(
    manager: &crate::pty_manager::PtyManager,
    grace_secs: u64,
) -> BTreeMap<PaneId, PaneAgent> {
    Watcher::bundled().poll_with_grace(manager, grace_secs)
}

/// Shared evidence collector. Only the status service advances task history.
pub struct Watcher {
    rules_dir: Option<PathBuf>,
    rules: DetectionRules,
    system: Option<System>,
    process_refreshed: Option<Instant>,
    roots: Vec<(PaneId, u32)>,
    matched: HashMap<PaneId, Option<AgentMatch>>,
    evidence_cache: HashMap<PaneId, (u64, Observation)>,
}

impl Watcher {
    pub fn bundled() -> Self {
        Self::with_rules(DetectionRules::bundled())
    }

    pub fn with_dir(rules_dir: PathBuf) -> Self {
        Self {
            rules: DetectionRules::load(&rules_dir),
            rules_dir: Some(rules_dir),
            system: None,
            process_refreshed: None,
            roots: Vec::new(),
            matched: HashMap::new(),
            evidence_cache: HashMap::new(),
        }
    }

    pub fn with_rules(rules: DetectionRules) -> Self {
        Self {
            rules,
            rules_dir: None,
            system: None,
            process_refreshed: None,
            roots: Vec::new(),
            matched: HashMap::new(),
            evidence_cache: HashMap::new(),
        }
    }

    pub fn reload_rules(&mut self) {
        if let Some(dir) = &self.rules_dir {
            self.rules = DetectionRules::load(dir);
        }
        self.evidence_cache.clear();
    }

    pub fn observe(
        &mut self,
        manager: &crate::pty_manager::PtyManager,
    ) -> BTreeMap<PaneId, Observation> {
        let roots = manager.pane_roots();
        let mut root_keys: Vec<_> = roots.iter().map(|r| (r.id, r.root_pid)).collect();
        root_keys.sort_unstable();
        let now = Instant::now();
        if self
            .process_refreshed
            .is_none_or(|at| now.duration_since(at) >= Duration::from_secs(1))
            || self.roots != root_keys
        {
            // Rebuild: sysinfo records suspension status on process discovery.
            let mut system = System::new();
            system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing()
                    .with_exe(UpdateKind::OnlyIfNotSet)
                    .with_cmd(UpdateKind::OnlyIfNotSet)
                    .with_cwd(UpdateKind::OnlyIfNotSet),
            );
            self.system = Some(system);
            self.process_refreshed = Some(now);
            self.roots = root_keys;
            self.evidence_cache.clear();
            let processes = self.system.as_ref().unwrap().processes();
            let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
            for (pid, process) in processes {
                if let Some(parent) = process.parent() {
                    children.entry(parent).or_default().push(*pid);
                }
            }
            for children in children.values_mut() {
                children.sort_unstable();
            }
            self.matched = roots
                .iter()
                .map(|root| {
                    (
                        root.id,
                        if root.root_pid == 0 {
                            None
                        } else {
                            find_agent(processes, &children, Pid::from_u32(root.root_pid))
                        },
                    )
                })
                .collect();
        }
        roots
            .into_iter()
            .map(|root| {
                let revision = manager.screen_revision(root.id).unwrap_or(0);
                if let Some((cached_revision, observation)) = self.evidence_cache.get(&root.id) {
                    if *cached_revision == revision {
                        return (root.id, observation.clone());
                    }
                }
                let observation = match self.matched.get(&root.id).and_then(Option::as_ref) {
                    Some(m) => {
                        let (state, reason) =
                            classify(m, manager.screen_text(root.id).as_deref(), &self.rules, 0);
                        Observation {
                            state,
                            reason,
                            instance_id: Some(format!("{}:{}:{}", root.id, m.pid, m.start_time)),
                            matched_pid: Some(m.pid),
                            root_pid: root.root_pid,
                        }
                    }
                    None => Observation {
                        state: PaneAgent::Idle,
                        reason: "no-agent-process".into(),
                        instance_id: None,
                        matched_pid: None,
                        root_pid: root.root_pid,
                    },
                };
                self.evidence_cache
                    .insert(root.id, (revision, observation.clone()));
                (root.id, observation)
            })
            .collect()
    }

    pub fn poll(
        &mut self,
        manager: &crate::pty_manager::PtyManager,
    ) -> BTreeMap<PaneId, PaneAgent> {
        self.observe(manager)
            .into_iter()
            .map(|(id, o)| (id, o.state))
            .collect()
    }

    pub fn poll_with_grace(
        &mut self,
        manager: &crate::pty_manager::PtyManager,
        _grace_secs: u64,
    ) -> BTreeMap<PaneId, PaneAgent> {
        self.poll(manager)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn cmd(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn matched(status: ProcessStatus) -> AgentMatch {
        AgentMatch {
            cli: "codex",
            label: "Codex",
            status,
            pid: 42,
            start_time: 1,
            cwd: Some("/tmp/proj".into()),
        }
    }

    #[test]
    fn existence_age_and_output_alone_are_not_work() {
        for screen in [
            None,
            Some("hello"),
            Some("working…"),
            Some("user typed a prompt"),
        ] {
            assert!(matches!(
                classify(
                    &matched(ProcessStatus::Run),
                    screen,
                    &DetectionRules::bundled(),
                    0
                )
                .0,
                PaneAgent::Unknown { .. }
            ));
        }
    }

    #[test]
    fn suspension_is_blocked_without_screen() {
        let (state, reason) = classify(
            &matched(ProcessStatus::Stop),
            None,
            &DetectionRules::bundled(),
            0,
        );
        assert!(matches!(state, PaneAgent::Blocked { cwd: Some(_), .. }));
        assert_eq!(reason, "process:suspended");
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
        assert_eq!(match_agent("mimo"), Some(("mimo", "MiMoCode")));
        assert_eq!(match_agent("agy"), Some(("agy", "Antigravity CLI")));
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

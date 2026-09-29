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
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

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
    Working { agent: String, cli: String },
    Idle,
}

/// A live pane and the PID of the process spawned directly in its PTY.
#[derive(Debug, Clone, Copy)]
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

/// Scan all panes against a process snapshot. Panes whose root PID is unknown
/// (0) or absent report [`PaneAgent::Idle`].
pub fn scan_panes(sys: &System, panes: &[PaneRoots]) -> BTreeMap<PaneId, PaneAgent> {
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
            let state = agent.map_or(PaneAgent::Idle, |(cli, label)| PaneAgent::Working {
                agent: label.to_string(),
                cli: cli.to_string(),
            });
            (pane.id, state)
        })
        .collect()
}

fn find_agent(
    processes: &HashMap<Pid, Process>,
    children: &HashMap<Pid, Vec<Pid>>,
    root: Pid,
) -> Option<(&'static str, &'static str)> {
    let mut seen = HashSet::new();
    let mut queue = vec![root];
    while let Some(pid) = queue.pop() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(proc) = processes.get(&pid) {
            if let Some(found) = identify(proc) {
                return Some(found);
            }
        }
        if let Some(kids) = children.get(&pid) {
            queue.extend(kids.iter().copied());
        }
    }
    None
}

/// Refresh the process list (names, parents, exe, argv) and scan every live
/// pane of the given manager.
pub fn poll_once(
    manager: &crate::pty_manager::PtyManager,
    sys: &mut System,
) -> BTreeMap<PaneId, PaneAgent> {
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );
    scan_panes(sys, &manager.pane_roots())
}

#[cfg(test)]
mod tests {
    use super::{match_agent, script_target};
    use std::ffi::OsString;

    fn cmd(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
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

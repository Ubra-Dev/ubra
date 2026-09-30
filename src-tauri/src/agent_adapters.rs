//! Agent adapter registry and session-reporting integrations.
//!
//! One [`Adapter`] covers every family and alias in the detected-agent
//! table ([`crate::agent_watch::AGENT_TABLE`]), plus official executable
//! aliases such as `kiro-cli`. Adapters report the exact main conversation
//! id/path through Ubra hooks so recovery can resume; identity is never
//! inferred from the newest session file or project directory.
//!
//! Hooks call `ubra-cli agent-report`, which forwards to the daemon's
//! `agent_report` op. Reports carry the pane key and session incarnation
//! from the process environment; stale incarnations are rejected and
//! nested (child-agent) conversations never replace the pane's main
//! recovery reference.

use std::fs;
use std::path::{Path, PathBuf};

/// Adapter for one agent family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adapter {
    /// Stable family id (matches the detected-agent table stem).
    pub family: &'static str,
    /// Display label.
    pub label: &'static str,
    /// Executable stems, including official aliases.
    pub executables: &'static [&'static str],
    /// How this family reports sessions and resumes.
    pub integration: Integration,
}

/// Session-reporting mechanism for a family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integration {
    /// Claude-compatible `settings.json` hooks (`SessionStart`/`SessionEnd`).
    ClaudeHooks,
    /// No verified hook mechanism: explicit hook-reported resume commands
    /// only, otherwise the shell fallback with a retry action.
    ReportedOnly,
}

/// Every detected family plus official aliases. Order matches
/// [`crate::agent_watch::AGENT_TABLE`].
pub const ADAPTERS: &[Adapter] = &[
    Adapter {
        family: "claude",
        label: "Claude Code",
        executables: &["claude"],
        integration: Integration::ClaudeHooks,
    },
    Adapter {
        family: "codex",
        label: "Codex",
        executables: &["codex"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "cursor-agent",
        label: "Cursor Agent CLI",
        executables: &["cursor-agent"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "opencode",
        label: "OpenCode",
        executables: &["opencode"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "gemini",
        label: "Gemini CLI",
        executables: &["gemini"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "copilot",
        label: "GitHub Copilot CLI",
        executables: &["copilot"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "amp",
        label: "Amp",
        executables: &["amp"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "aider",
        label: "Aider",
        executables: &["aider"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "goose",
        label: "Goose",
        executables: &["goose"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "crush",
        label: "Crush",
        executables: &["crush"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "droid",
        label: "Droid",
        executables: &["droid"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "qwen",
        label: "Qwen Code",
        executables: &["qwen"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "grok",
        label: "Grok CLI",
        executables: &["grok"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "kiro",
        label: "Kiro CLI",
        executables: &["kiro", "kiro-cli"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "cline",
        label: "Cline",
        executables: &["cline"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "devin",
        label: "Devin CLI",
        executables: &["devin"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "muse",
        label: "Muse",
        executables: &["muse"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "pi",
        label: "Pi",
        executables: &["pi"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "omp",
        label: "OMP",
        executables: &["omp"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "kimi",
        label: "Kimi Code CLI",
        executables: &["kimi"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "hermes",
        label: "Hermes Agent",
        executables: &["hermes"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "qoder",
        label: "Qoder CLI",
        executables: &["qoder", "qodercli"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "letta",
        label: "Letta Code",
        executables: &["letta"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "kilo",
        label: "Kilo Code CLI",
        executables: &["kilo"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "mastracode",
        label: "MastraCode",
        executables: &["mastracode"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "antigravity",
        label: "Antigravity CLI",
        executables: &["antigravity", "antigravity-cli", "agy"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "maki",
        label: "Maki",
        executables: &["maki"],
        integration: Integration::ReportedOnly,
    },
    Adapter {
        family: "mimo",
        label: "MiMoCode",
        executables: &["mimo"],
        integration: Integration::ReportedOnly,
    },
];

/// Find the adapter owning an executable stem (case-insensitive, `.exe`
///-tolerant), including aliases.
pub fn find_by_executable(stem: &str) -> Option<&'static Adapter> {
    let lower = stem.to_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    ADAPTERS
        .iter()
        .find(|adapter| adapter.family == stem || adapter.executables.contains(&stem))
}

/// Build resume argv for an exact conversation reference. An explicit
/// agent-reported command always wins; registry templates exist only for
/// CLIs with a verified resume flag. `None` means shell fallback.
pub fn resume_argv(
    adapter: &Adapter,
    exe: &str,
    reference: &str,
    reported: Option<&[String]>,
) -> Option<Vec<String>> {
    if let Some(argv) = reported.filter(|argv| !argv.is_empty()) {
        if argv.iter().all(|arg| !arg.contains('\0')) {
            return Some(argv.to_vec());
        }
        return None;
    }
    match adapter.family {
        "claude" => Some(vec![
            exe.to_string(),
            "--resume".to_string(),
            reference.to_string(),
        ]),
        _ => None,
    }
}

/// Whether a report belongs to the pane's main conversation or to a nested
/// child agent. The hook process is a direct child of the reporting agent,
/// so a second agent executable between the reporter and the pane root
/// proves nesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportRole {
    Main,
    Child,
}

pub fn classify_report(
    reporter_pid: u32,
    root_pid: u32,
    parent_of: &dyn Fn(u32) -> Option<u32>,
    name_of: &dyn Fn(u32) -> Option<String>,
) -> ReportRole {
    let mut pid = match parent_of(reporter_pid) {
        Some(pid) => pid,
        None => return ReportRole::Main,
    };
    let mut agents_seen = 0_u32;
    for _ in 0..64 {
        if let Some(name) = name_of(pid) {
            let stem = name.rsplit(['/', '\\']).next().unwrap_or(&name);
            if find_by_executable(stem).is_some() {
                agents_seen += 1;
                if agents_seen >= 2 {
                    return ReportRole::Child;
                }
            }
        }
        if pid == root_pid {
            break;
        }
        match parent_of(pid) {
            Some(parent) if parent != pid => pid = parent,
            _ => break,
        }
    }
    ReportRole::Main
}

/// Live process-tree role check via sysinfo. Unknown trees accept the
/// report as main (a missed child only risks an extra resume candidate,
/// while a missed main breaks recovery).
pub fn classify_report_live(reporter_pid: u32, root_pid: u32) -> ReportRole {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        false,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    let processes = system.processes();
    classify_report(
        reporter_pid,
        root_pid,
        &|pid| {
            processes
                .get(&Pid::from_u32(pid))
                .and_then(|p| p.parent())
                .map(|p| p.as_u32())
        },
        &|pid| {
            processes
                .get(&Pid::from_u32(pid))
                .map(|p| p.name().to_string_lossy().into_owned())
        },
    )
}

/// Parsed hook event: exact reference plus optional resume details.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HookReport {
    pub reference: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
}

/// Parse Claude-compatible hook stdin JSON. `session_id` is the exact
/// reference; `transcript_path` is the fallback identity.
pub fn parse_hook_stdin(text: &str) -> HookReport {
    let value: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let string = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    HookReport {
        reference: string("session_id").or_else(|| string("transcript_path")),
        cwd: string("cwd"),
        model: string("model"),
    }
}

/// Marker identifying Ubra-owned hook entries for merge/removal.
pub const UBRA_HOOK_MARKER: &str = "ubraManaged";

/// User-level Claude settings path under a home dir.
pub fn claude_settings_path(home: &Path) -> PathBuf {
    home.join(".claude").join("settings.json")
}

fn ubra_hook_command(cli_path: &str, event: &str) -> String {
    format!("{cli_path} agent-report --hook-event {event}")
}

fn ubra_hook_entry(cli_path: &str, event: &str) -> serde_json::Value {
    serde_json::json!({
        "matcher": "*",
        UBRA_HOOK_MARKER: true,
        "hooks": [{ "type": "command", "command": ubra_hook_command(cli_path, event) }],
    })
}

fn is_ubra_entry(entry: &serde_json::Value) -> bool {
    entry.get(UBRA_HOOK_MARKER) == Some(&serde_json::Value::Bool(true))
}

/// Install (merge) Ubra SessionStart/SessionEnd hooks into the Claude user
/// settings, preserving existing user and third-party entries.
pub fn install_claude_hooks(home: &Path, cli_path: &str) -> anyhow::Result<bool> {
    let path = claude_settings_path(home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut doc: serde_json::Value = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("existing {} is not valid JSON: {e}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(e) => return Err(e.into()),
    };
    if !doc.is_object() {
        anyhow::bail!("existing {} is not a JSON object", path.display());
    }
    let mut changed = false;
    for event in ["SessionStart", "SessionEnd"] {
        let hooks_missing = doc.pointer("/hooks").is_none();
        if hooks_missing {
            let obj = doc.as_object_mut().ok_or_else(|| {
                anyhow::anyhow!("existing {} is not a JSON object", path.display())
            })?;
            obj.insert("hooks".to_string(), serde_json::json!({}));
        }
        let hooks = doc.pointer_mut("/hooks").filter(|v| v.is_object());
        let Some(hooks) = hooks.and_then(serde_json::Value::as_object_mut) else {
            anyhow::bail!("existing {} has a non-object hooks section", path.display());
        };
        let list = hooks
            .entry(event.to_string())
            .or_insert_with(|| serde_json::json!([]));
        let Some(list) = list.as_array_mut() else {
            anyhow::bail!(
                "existing {} has a non-array {event} hooks section",
                path.display()
            );
        };
        list.retain(|entry| !is_ubra_entry(entry));
        list.push(ubra_hook_entry(cli_path, event));
        changed = true;
    }
    if changed {
        fs::write(&path, serde_json::to_string_pretty(&doc)?)?;
    }
    Ok(changed)
}

/// Remove only Ubra-owned hook entries, preserving everything else.
/// Returns true when the file changed.
pub fn remove_claude_hooks(home: &Path) -> anyhow::Result<bool> {
    let path = claude_settings_path(home);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    let mut doc: serde_json::Value = serde_json::from_str(&text)?;
    let mut changed = false;
    if let Some(lists) = doc.pointer_mut("/hooks").and_then(|v| v.as_object_mut()) {
        for list in lists.values_mut().filter_map(|v| v.as_array_mut()) {
            let before = list.len();
            list.retain(|entry| !is_ubra_entry(entry));
            changed |= list.len() != before;
        }
    }
    if changed {
        fs::write(&path, serde_json::to_string_pretty(&doc)?)?;
    }
    Ok(changed)
}

/// Integration presence per family: installed hook kinds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IntegrationStatus {
    pub family: String,
    pub label: String,
    pub installed: bool,
    pub mechanism: String,
}

pub fn integration_status(home: &Path) -> Vec<IntegrationStatus> {
    let claude_installed = fs::read_to_string(claude_settings_path(home))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|doc| {
            ["SessionStart", "SessionEnd"].iter().all(|event| {
                doc.pointer(&format!("/hooks/{event}"))
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|list| list.iter().any(is_ubra_entry))
            })
        });
    ADAPTERS
        .iter()
        .map(|adapter| {
            let (installed, mechanism) = match adapter.integration {
                Integration::ClaudeHooks => (claude_installed, "claude-hooks"),
                Integration::ReportedOnly => (false, "reported-only"),
            };
            IntegrationStatus {
                family: adapter.family.to_string(),
                label: adapter.label.to_string(),
                installed,
                mechanism: mechanism.to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn scratch_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ubra-adapters-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn registry_covers_detected_table_and_aliases() {
        for (stem, _) in crate::agent_watch::AGENT_TABLE {
            assert!(
                find_by_executable(stem).is_some(),
                "missing adapter for {stem}"
            );
        }
        assert_eq!(find_by_executable("kiro-cli").unwrap().family, "kiro");
        assert_eq!(find_by_executable("KIRO").unwrap().family, "kiro");
        assert_eq!(find_by_executable("qodercli").unwrap().family, "qoder");
        assert_eq!(find_by_executable("agy").unwrap().family, "antigravity");
        assert!(find_by_executable("not-an-agent").is_none());
    }

    #[test]
    fn resume_prefers_reported_argv_then_verified_templates() {
        let claude = find_by_executable("claude").unwrap();
        assert_eq!(
            resume_argv(claude, "claude", "sess-1", None).unwrap(),
            vec!["claude", "--resume", "sess-1"]
        );
        let reported = vec!["codex".to_string(), "resume".to_string(), "abc".to_string()];
        let codex = find_by_executable("codex").unwrap();
        assert_eq!(
            resume_argv(codex, "codex", "abc", Some(&reported)).unwrap(),
            reported
        );
        assert!(resume_argv(codex, "codex", "abc", None).is_none());
        let nul = vec!["codex\0".to_string()];
        assert!(resume_argv(codex, "codex", "abc", Some(&nul)).is_none());
    }

    #[test]
    fn hook_stdin_prefers_session_id() {
        let report = parse_hook_stdin(r#"{"session_id":"s1","cwd":"/w","model":"m"}"#);
        assert_eq!(report.reference.as_deref(), Some("s1"));
        assert_eq!(report.cwd.as_deref(), Some("/w"));
        let fallback = parse_hook_stdin(r#"{"transcript_path":"/t.jsonl"}"#);
        assert_eq!(fallback.reference.as_deref(), Some("/t.jsonl"));
        assert!(parse_hook_stdin("nope").reference.is_none());
    }

    fn tree(
        parents: &[(u32, u32)],
        names: &[(u32, &str)],
    ) -> (HashMap<u32, u32>, HashMap<u32, String>) {
        (
            parents.iter().map(|(c, p)| (*c, *p)).collect(),
            names.iter().map(|(p, n)| (*p, n.to_string())).collect(),
        )
    }

    #[test]
    fn classifier_keeps_main_and_excludes_children() {
        // shell(100) -> claude(200) -> hook(300): main.
        let (parents, names) = tree(&[(300, 200), (200, 100)], &[(200, "claude"), (100, "zsh")]);
        assert_eq!(
            classify_report(300, 100, &|p| parents.get(&p).copied(), &|p| names
                .get(&p)
                .cloned()),
            ReportRole::Main
        );
        // agent directly in pane: claude(100) -> hook(300): main.
        let (parents, names) = tree(&[(300, 100)], &[(100, "claude")]);
        assert_eq!(
            classify_report(300, 100, &|p| parents.get(&p).copied(), &|p| names
                .get(&p)
                .cloned()),
            ReportRole::Main
        );
        // shell(100) -> claude(200) -> codex(250) -> hook(300): child.
        let (parents, names) = tree(
            &[(300, 250), (250, 200), (200, 100)],
            &[(250, "codex"), (200, "claude"), (100, "zsh")],
        );
        assert_eq!(
            classify_report(300, 100, &|p| parents.get(&p).copied(), &|p| names
                .get(&p)
                .cloned()),
            ReportRole::Child
        );
    }

    #[test]
    fn claude_hooks_merge_and_remove_preserving_user_entries() {
        let home = scratch_home("merge");
        let settings = claude_settings_path(&home);
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(
            &settings,
            r#"{"hooks":{"SessionStart":[{"matcher":"*","hooks":[{"type":"command","command":"other"}]}]}}"#,
        )
        .unwrap();
        install_claude_hooks(&home, "/bin/ubra-cli").unwrap();
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        let starts = doc
            .pointer("/hooks/SessionStart")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(starts.len(), 2);
        assert!(starts.iter().any(|e| !is_ubra_entry(e)));
        assert!(starts.iter().any(is_ubra_entry));
        assert!(doc
            .pointer("/hooks/SessionEnd")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .any(is_ubra_entry));
        // Idempotent reinstall.
        install_claude_hooks(&home, "/bin/ubra-cli").unwrap();
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(
            doc.pointer("/hooks/SessionStart")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(
            integration_status(&home)
                .iter()
                .find(|s| s.family == "claude")
                .unwrap()
                .installed
        );
        // Removal keeps the user entry.
        remove_claude_hooks(&home).unwrap();
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        let starts = doc
            .pointer("/hooks/SessionStart")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(starts.len(), 1);
        assert!(!is_ubra_entry(&starts[0]));
        let _ = std::fs::remove_dir_all(&home);
    }
}

//! Screen-based agent detection (Phase 2): classify approval/question UI
//! from live pane screens.
//!
//! Process identification still decides *whether* a pane hosts an agent
//! ([`crate::agent_watch`]); rules here decide whether that agent is
//! *blocked* on the user. Rules are strict multi-evidence ANDs over the
//! bottom window of the pane's emulated screen: no match means "no
//! evidence", never blocked.
//!
//! Bundled rules live in [`DetectionRules::bundled`]. Per-CLI TOML files in
//! the rules dir (`<cli>.toml`) replace that CLI's bundled rules entirely:
//!
//! ```toml
//! [[blocked]]
//! id = "approval-prompt"
//! contains = ["do you want to proceed", "❯"]
//! ```
//!
//! Matching is case-insensitive substring search. Invalid files are ignored
//! with a warning; a missing dir means bundled rules only.

use std::collections::HashMap;
use std::path::Path;

/// Live-screen lines examined for approval UI, counting from the bottom.
pub const SCREEN_WINDOW_LINES: usize = 40;

/// One blocked pattern: every `contains` substring (case-insensitive) must
/// appear in the screen window. Substrings are normalized to lowercase at
/// construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedRule {
    pub id: String,
    pub contains: Vec<String>,
}

impl BlockedRule {
    pub fn new(id: &str, contains: Vec<String>) -> Self {
        Self {
            id: id.to_string(),
            contains: contains.iter().map(|s| s.to_lowercase()).collect(),
        }
    }

    fn matches(&self, window_lower: &str) -> bool {
        !self.contains.is_empty() && self.contains.iter().all(|p| window_lower.contains(p))
    }
}

/// Blocked rules by canonical CLI plus agent-agnostic rules.
#[derive(Debug, Clone, Default)]
pub struct DetectionRules {
    per_cli: HashMap<String, Vec<BlockedRule>>,
    any: Vec<BlockedRule>,
}

#[derive(Debug, serde::Deserialize)]
struct RuleFile {
    #[serde(default)]
    blocked: Vec<FileRule>,
}

#[derive(Debug, serde::Deserialize)]
struct FileRule {
    #[serde(default)]
    id: String,
    #[serde(default)]
    contains: Vec<String>,
}

impl DetectionRules {
    /// Bundled rules. PROVISIONAL: patterns were written from best-known UI
    /// shapes, not observed screens. Strict ANDs keep misses safe (an
    /// unrecognized prompt reads working, never blocked); correct them via
    /// `<data-dir>/agent-detection/<cli>.toml` overrides.
    pub fn bundled() -> Self {
        Self {
            per_cli: HashMap::from([(
                "claude".to_string(),
                vec![BlockedRule::new(
                    "approval-prompt",
                    vec!["do you want to proceed".to_string(), "❯".to_string()],
                )],
            )]),
            any: Vec::new(),
        }
    }

    /// Rules applying to `cli` (agent-agnostic first, then cli-specific).
    fn for_cli(&self, cli: &str) -> impl Iterator<Item = &BlockedRule> {
        self.any
            .iter()
            .chain(self.per_cli.get(cli).into_iter().flatten())
    }

    /// Load bundled rules plus `<dir>/<cli>.toml` overrides (each file
    /// replaces its CLI's bundled rules). See module docs for tolerance.
    pub fn load(dir: &Path) -> Self {
        let mut rules = Self::bundled();
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return rules,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            let Some(cli) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_lowercase)
            else {
                continue;
            };
            match Self::load_file(&path) {
                Ok(file_rules) => {
                    rules.per_cli.insert(cli, file_rules);
                }
                Err(e) => eprintln!(
                    "ubra: ignoring invalid detection file {}: {e}",
                    path.display()
                ),
            }
        }
        rules
    }

    fn load_file(path: &Path) -> anyhow::Result<Vec<BlockedRule>> {
        let text = std::fs::read_to_string(path)?;
        let file: RuleFile = toml::from_str(&text)?;
        Ok(file
            .blocked
            .into_iter()
            .filter(|r| !r.id.trim().is_empty() && !r.contains.is_empty())
            .map(|r| BlockedRule::new(&r.id, r.contains))
            .collect())
    }
}

/// Bottom window of a screen snapshot, lowercased for matching.
fn bottom_window(screen: &str) -> String {
    let lines: Vec<&str> = screen.lines().collect();
    let skip = lines.len().saturating_sub(SCREEN_WINDOW_LINES);
    lines[skip..].join("\n").to_lowercase()
}

/// True when any rule for `cli` (or any agent) matches the screen window.
pub fn is_screen_blocked(rules: &DetectionRules, cli: &str, screen: &str) -> bool {
    let window = bottom_window(screen);
    rules.for_cli(cli).any(|r| r.matches(&window))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch_dir() -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ubra-rules-test-{}-{id}", std::process::id()))
    }

    fn rules_for(cli: &str, id: &str, contains: &[&str]) -> DetectionRules {
        DetectionRules {
            per_cli: HashMap::from([(
                cli.to_string(),
                vec![BlockedRule::new(
                    id,
                    contains.iter().map(|s| s.to_string()).collect(),
                )],
            )]),
            any: Vec::new(),
        }
    }

    #[test]
    fn blocked_requires_all_evidence_case_insensitive() {
        let rules = rules_for("codex", "approval", &["allow this command", "❯"]);
        let full = "Run `cargo test`?\nAllow This Command\n❯ 1. yes\n  2. no";
        assert!(is_screen_blocked(&rules, "codex", full));
        assert!(!is_screen_blocked(
            &rules,
            "codex",
            "Allow This Command\nworking…"
        ));
        assert!(!is_screen_blocked(&rules, "codex", "❯ /help\nworking…"));
        assert!(!is_screen_blocked(&rules, "codex", "plain shell output"));
        assert!(!is_screen_blocked(&rules, "codex", ""));
    }

    #[test]
    fn rules_scope_to_cli() {
        let rules = rules_for("codex", "approval", &["allow this command", "❯"]);
        let screen = "Allow this command\n❯ 1. yes";
        assert!(is_screen_blocked(&rules, "codex", screen));
        // Same screen text under another agent does not match cli rules.
        assert!(!is_screen_blocked(&rules, "claude", screen));
        // Agent-agnostic rules match every cli.
        let any = DetectionRules {
            per_cli: HashMap::new(),
            any: vec![BlockedRule::new("generic", vec!["❯".to_string()])],
        };
        assert!(is_screen_blocked(&any, "claude", screen));
        assert!(is_screen_blocked(&any, "whatever", screen));
    }

    #[test]
    fn window_ignores_lines_above_it() {
        let rules = rules_for("codex", "approval", &["allow this command", "❯"]);
        let mut old = vec!["Allow this command", "❯ 1. yes"];
        old.extend(std::iter::repeat_n(
            "filler output",
            SCREEN_WINDOW_LINES + 5,
        ));
        assert!(!is_screen_blocked(&rules, "codex", &old.join("\n")));
        let mut recent = vec!["filler output"; SCREEN_WINDOW_LINES + 5];
        recent.extend(["Allow this command", "❯ 1. yes"]);
        assert!(is_screen_blocked(&rules, "codex", &recent.join("\n")));
    }

    #[test]
    fn load_missing_dir_is_bundled() {
        let rules = DetectionRules::load(&scratch_dir().join("nope"));
        assert!(is_screen_blocked(
            &rules,
            "claude",
            "Do you want to proceed?\n❯ 1. Yes"
        ));
        assert!(!is_screen_blocked(&rules, "codex", "anything"));
    }

    #[test]
    fn load_override_replaces_and_bad_files_ignored() {
        let dir = scratch_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("codex.toml"),
            "[[blocked]]\nid = \"custom\"\ncontains = [\"proceed\", \"yes?\"]\n",
        )
        .unwrap();
        std::fs::write(dir.join("bad.toml"), "[[blocked\nnot toml").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        // Empty rule (no evidence) must be dropped, not match everything.
        std::fs::write(dir.join("pi.toml"), "[[blocked]]\nid = \"empty\"\n").unwrap();

        let rules = DetectionRules::load(&dir);
        assert!(is_screen_blocked(&rules, "codex", "Proceed?\nYes?"));
        // Bundled claude rules survive alongside the override.
        assert!(is_screen_blocked(
            &rules,
            "claude",
            "Do you want to proceed?\n❯ 1. Yes"
        ));
        // The empty rule file replaced pi's (nonexistent) bundled rules with nothing.
        assert!(!is_screen_blocked(&rules, "pi", "Proceed?\nYes?"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

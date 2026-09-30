//! Evidence-based screen classification. Process detection identifies the CLI;
//! this module recognizes only known, bounded interactive UI markers.
//! Missing or ambiguous evidence is `None`, never inferred activity.
//!
//! `<cli>.toml` replaces that CLI's entire bundled profile. Existing
//! `[[blocked]]`/`id`/`contains` files retain their forty-line matching window.
//! Optional `[[working]]` and `[[idle]]` sections use the same format, and an
//! optional `[[hold]]` section marks viewer screens (transcript, picker) whose
//! stale markers must not move classification. New `window_lines`,
//! `not_contains`, `line_prefixes`, and `tail_contains` fields allow stricter
//! active-footer rules. See fixtures/agent_screens/README.md.

use std::collections::HashMap;
use std::path::Path;

pub const SCREEN_WINDOW_LINES: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    Blocked,
    Working,
    Idle,
    /// A viewer (transcript, picker) showing history instead of live state.
    /// Hold evidence never moves classification; the tracker keeps its
    /// confirmed state while a hold rule matches.
    Hold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenEvidence {
    pub kind: ScreenState,
    pub rule_id: String,
}

#[derive(Debug, Clone)]
struct ScreenRule {
    id: String,
    kind: ScreenState,
    contains: Vec<String>,
    not_contains: Vec<String>,
    line_prefixes: Vec<String>,
    tail_contains: Vec<String>,
    window_lines: usize,
    // Bundled approval menus are obsolete when the main composer follows them.
    superseded_by_input: Option<char>,
    composer_binding: Option<ComposerBinding>,
}

/// Busy evidence belongs to one composer, not any nearby historical footer.
#[derive(Debug, Clone)]
struct ComposerBinding {
    marker: String,
    prefixes: Vec<String>,
    // Codex/Gemini render their indicator above their composer. Claude and
    // OpenCode render their interrupt footer below it.
    indicator_before_composer: bool,
}

impl ScreenRule {
    fn new(kind: ScreenState, id: &str, contains: &[&str], window_lines: usize) -> Self {
        Self {
            id: id.to_string(),
            kind,
            contains: contains.iter().map(|s| normalize(s)).collect(),
            not_contains: Vec::new(),
            line_prefixes: Vec::new(),
            tail_contains: Vec::new(),
            window_lines,
            superseded_by_input: None,
            composer_binding: None,
        }
    }

    fn matches(&self, lines: &[String]) -> bool {
        let window = &lines[lines.len().saturating_sub(self.window_lines)..];
        // Joining normalized lines allows a marker to wrap at word boundaries.
        let text = window.join(" ");
        if self.contains.is_empty()
            || !self.contains.iter().all(|p| text.contains(p))
            || self.not_contains.iter().any(|p| text.contains(p))
            || (!self.line_prefixes.is_empty()
                && !window
                    .iter()
                    .any(|l| self.line_prefixes.iter().any(|p| l.starts_with(p))))
        {
            return false;
        }
        let tail = window[window.len().saturating_sub(3)..].join(" ");
        if !self.tail_contains.iter().all(|p| tail.contains(p)) {
            return false;
        }
        if let Some(prompt) = self.superseded_by_input {
            // A fresh composer after an old approval menu proves that menu is
            // no longer active. Numbered selected options are not composers.
            if window.iter().enumerate().any(|(at, line)| {
                line.strip_prefix(prompt).is_some_and(|rest| {
                    let rest = rest.trim_start();
                    !rest.chars().next().is_some_and(|c| c.is_ascii_digit())
                        && window[..at].join(" ").contains(&self.contains[0])
                })
            }) {
                return false;
            }
        }
        if let Some(binding) = &self.composer_binding {
            let Some(marker_at) = text.rfind(&binding.marker) else {
                return false;
            };
            let marker_end = marker_at + binding.marker.len();
            let mut line_end = 0;
            let mut newer_composers = 0;
            for line in window {
                let line_start = line_end;
                line_end += line.len() + 1;
                if line_start >= marker_end
                    && binding.prefixes.iter().any(|prefix| {
                        line.strip_prefix(prefix).is_some_and(|rest| {
                            !rest
                                .trim_start()
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_ascii_digit())
                        })
                    })
                {
                    newer_composers += 1;
                }
            }
            let allowed = usize::from(binding.indicator_before_composer);
            if newer_composers > allowed {
                return false;
            }
        }
        true
    }

    fn bind_composer(&mut self, marker: &str, prefixes: &[&str], before: bool) {
        self.composer_binding = Some(ComposerBinding {
            marker: normalize(marker),
            prefixes: prefixes.iter().map(|p| normalize(p)).collect(),
            indicator_before_composer: before,
        });
    }
}

#[derive(Debug, Clone, Default)]
pub struct DetectionRules {
    per_cli: HashMap<String, Vec<ScreenRule>>,
}

#[derive(Debug, serde::Deserialize)]
struct RuleFile {
    #[serde(default)]
    blocked: Vec<FileRule>,
    #[serde(default)]
    working: Vec<FileRule>,
    #[serde(default)]
    idle: Vec<FileRule>,
    #[serde(default)]
    hold: Vec<FileRule>,
}

#[derive(Debug, serde::Deserialize)]
struct FileRule {
    #[serde(default)]
    id: String,
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    not_contains: Vec<String>,
    #[serde(default)]
    line_prefixes: Vec<String>,
    #[serde(default)]
    tail_contains: Vec<String>,
    #[serde(default = "default_window")]
    window_lines: usize,
}
fn default_window() -> usize {
    SCREEN_WINDOW_LINES
}

impl DetectionRules {
    /// Conservative profiles backed by installed renderers/primary source.
    /// Fixtures are source-derived cases, not claimed live terminal captures.
    pub fn bundled() -> Self {
        use ScreenState::*;
        let mut profiles = HashMap::new();
        let mut claude_approval = ScreenRule::new(
            Blocked,
            "claude-approval",
            &["do you want to proceed?", "❯", "esc", "cancel"],
            14,
        );
        claude_approval.tail_contains = vec!["esc".into(), "cancel".into()];
        claude_approval.superseded_by_input = Some('❯');
        let mut claude_busy =
            ScreenRule::new(Working, "claude-interrupt-footer", &["esc", "interrupt"], 8);
        claude_busy.tail_contains = vec!["esc".into(), "interrupt".into()];
        claude_busy.bind_composer("interrupt", &["❯"], false);
        let mut claude_ready =
            ScreenRule::new(Idle, "claude-ready-composer", &["❯", "? for shortcuts"], 6);
        claude_ready.line_prefixes = vec!["❯".into()];
        claude_ready.tail_contains = vec!["? for shortcuts".into()];
        claude_ready.not_contains = vec!["interrupt".into(), "esc to cancel".into()];
        // Transcript viewer over live history: hold, never reclassify.
        let claude_viewer = ScreenRule::new(
            Hold,
            "claude-transcript-viewer",
            &["showing detailed transcript"],
            3,
        );
        profiles.insert(
            "claude".into(),
            vec![claude_viewer, claude_approval, claude_busy, claude_ready],
        );

        let mut codex = Vec::new();
        for (id, question) in [
            (
                "codex-command-approval",
                "would you like to run the following command?",
            ),
            (
                "codex-edit-approval",
                "would you like to make the following edits?",
            ),
            (
                "codex-permission-approval",
                "would you like to grant these permissions?",
            ),
        ] {
            let mut rule = ScreenRule::new(Blocked, id, &[question, "yes,", "esc"], 14);
            rule.tail_contains = vec!["esc".into()];
            rule.superseded_by_input = Some('›');
            codex.push(rule);
        }
        let mut codex_busy = ScreenRule::new(
            Working,
            "codex-interrupt-indicator",
            &["esc to interrupt)"],
            8,
        );
        codex_busy.bind_composer("esc to interrupt)", &["›", "❯"], true);
        codex.push(codex_busy);
        let mut codex_ready = ScreenRule::new(Idle, "codex-ready-composer", &["for shortcuts"], 6);
        codex_ready.line_prefixes = vec!["›".into(), "❯".into()];
        codex_ready.tail_contains = vec!["for shortcuts".into()];
        codex_ready.not_contains = vec!["to interrupt".into(), "yes, proceed".into()];
        codex.push(codex_ready);
        profiles.insert("codex".into(), codex);

        let mut gemini = Vec::new();
        for (id, question) in [
            ("gemini-execution-approval", "allow execution of"),
            ("gemini-edit-approval", "apply this change?"),
            ("gemini-info-approval", "do you want to proceed?"),
        ] {
            let mut rule = ScreenRule::new(
                Blocked,
                id,
                &[question, "allow once", "no, suggest changes (esc)"],
                18,
            );
            rule.tail_contains = vec!["no, suggest changes (esc)".into()];
            rule.not_contains = vec!["type your message or @path/to/file".into()];
            gemini.push(rule);
        }
        let mut gemini_busy = ScreenRule::new(
            Working,
            "gemini-responding-indicator",
            &["(esc to cancel,"],
            8,
        );
        gemini_busy.not_contains = vec!["allow once".into()];
        gemini_busy.bind_composer("(esc to cancel,", &[">", "│ >"], true);
        gemini.push(gemini_busy);
        let mut gemini_question = ScreenRule::new(
            Blocked,
            "gemini-text-question",
            &["enter your response", "enter to submit", "esc to cancel"],
            12,
        );
        gemini_question.tail_contains = vec!["enter to submit".into(), "esc to cancel".into()];
        gemini.push(gemini_question);
        let mut gemini_review = ScreenRule::new(
            Blocked,
            "gemini-answer-review",
            &[
                "review your answers:",
                "enter to submit",
                "to edit answers",
                "esc to cancel",
            ],
            18,
        );
        gemini_review.tail_contains = vec!["enter to submit".into(), "esc to cancel".into()];
        gemini.push(gemini_review);
        let mut gemini_ready = ScreenRule::new(
            Idle,
            "gemini-empty-composer",
            &["type your message or @path/to/file"],
            6,
        );
        gemini_ready.line_prefixes = vec![">".into(), "│ >".into()];
        gemini_ready.not_contains = vec!["esc to cancel".into(), "allow once".into()];
        gemini.push(gemini_ready);
        profiles.insert("gemini".into(), gemini);
        let mut opencode_approval = ScreenRule::new(
            Blocked,
            "opencode-permission-options",
            &[
                "allow once",
                "allow always",
                "reject",
                "⇆ select",
                "enter confirm",
            ],
            14,
        );
        opencode_approval.tail_contains = vec!["⇆ select".into(), "enter confirm".into()];
        let mut opencode_busy =
            ScreenRule::new(Working, "opencode-interrupt-footer", &["esc interrupt"], 6);
        opencode_busy.tail_contains = vec!["esc interrupt".into()];
        opencode_busy.bind_composer("esc interrupt", &["ask anything…"], false);
        let mut opencode_busy_confirm = ScreenRule::new(
            Working,
            "opencode-interrupt-confirm-footer",
            &["esc again to interrupt"],
            6,
        );
        opencode_busy_confirm.tail_contains = vec!["esc again to interrupt".into()];
        opencode_busy_confirm.bind_composer("esc again to interrupt", &["ask anything…"], false);
        let mut opencode_ready = ScreenRule::new(
            Idle,
            "opencode-empty-composer",
            &["ask anything…", "commands"],
            6,
        );
        opencode_ready.not_contains = vec!["interrupt".into(), "allow once".into()];
        opencode_ready.tail_contains = vec!["commands".into()];
        profiles.insert(
            "opencode".into(),
            vec![
                opencode_approval,
                opencode_busy,
                opencode_busy_confirm,
                opencode_ready,
            ],
        );

        // MiMoCode renders an OpenCode-style footer: a spinner row with an
        // `esc interrupt` hint while busy (two-stage `esc again to interrupt`
        // after the first press), idle-only `@`/`$`/`/` hints, and a titled
        // permission dialog. The destructive-command variant omits "allow
        // always", so only "allow once" is required.
        let mut mimo_approval = ScreenRule::new(
            Blocked,
            "mimo-permission-options",
            &[
                "permission required",
                "allow once",
                "reject",
                "⇆ select",
                "enter confirm",
            ],
            14,
        );
        mimo_approval.tail_contains = vec!["⇆ select".into(), "enter confirm".into()];
        let mut mimo_question = ScreenRule::new(
            Blocked,
            "mimo-text-question",
            &["type your own answer", "enter", "esc dismiss"],
            12,
        );
        mimo_question.tail_contains = vec!["esc dismiss".into()];
        let mut mimo_busy =
            ScreenRule::new(Working, "mimo-interrupt-footer", &["esc interrupt"], 6);
        mimo_busy.tail_contains = vec!["esc interrupt".into()];
        mimo_busy.bind_composer("esc interrupt", &["type your message..."], false);
        let mut mimo_busy_confirm = ScreenRule::new(
            Working,
            "mimo-interrupt-confirm-footer",
            &["esc again to interrupt"],
            6,
        );
        mimo_busy_confirm.tail_contains = vec!["esc again to interrupt".into()];
        mimo_busy_confirm.bind_composer("esc again to interrupt", &["type your message..."], false);
        let mut mimo_ready = ScreenRule::new(
            Idle,
            "mimo-empty-composer",
            &["type your message...", "attach file", "commands"],
            6,
        );
        mimo_ready.not_contains = vec![
            "interrupt".into(),
            "allow once".into(),
            "permission required".into(),
        ];
        mimo_ready.tail_contains = vec!["commands".into()];
        profiles.insert(
            "mimo".into(),
            vec![
                mimo_approval,
                mimo_question,
                mimo_busy,
                mimo_busy_confirm,
                mimo_ready,
            ],
        );

        // Antigravity (`agy`) renders a single-line generating footer and
        // permission options phrased as conversation-scoped allow/deny rules.
        // No composer marker is evidenced yet, so there is no idle rule:
        // completion needs a live screen capture.
        let agy_approval = ScreenRule::new(
            Blocked,
            "agy-permission-options",
            &["in this conversation", "always allow", "always deny"],
            14,
        );
        let mut agy_busy = ScreenRule::new(
            Working,
            "agy-generating-footer",
            &["generating... (enter/esc to cancel)"],
            6,
        );
        agy_busy.tail_contains = vec!["generating... (enter/esc to cancel)".into()];
        profiles.insert("agy".into(), vec![agy_approval, agy_busy]);

        // cursor-agent approval dialogs pair single-key hints: approve `(y)`
        // with reject `(esc or n)` (delete uses `(n)`), plus per-tool
        // questions. No busy/composer markers are evidenced yet.
        let cursor_approval =
            ScreenRule::new(Blocked, "cursor-approval-options", &["(y)", "esc or n"], 14);
        let cursor_delete = ScreenRule::new(
            Blocked,
            "cursor-delete-confirm",
            &["delete this file?", "(y)", "(n)"],
            14,
        );
        profiles.insert("cursor-agent".into(), vec![cursor_approval, cursor_delete]);
        Self { per_cli: profiles }
    }

    /// Hold wins over every state: a viewer over stale markers must not
    /// move classification. Otherwise blocked wins over busy and ready.
    /// No profile or no match is no evidence.
    pub fn evidence(&self, cli: &str, screen: &str) -> Option<ScreenEvidence> {
        let mut lines: Vec<String> = screen.lines().map(normalize).collect();
        // VT screens include unused trailing rows; ignore those, while retaining
        // blank rows *inside* the UI's bounded footer window.
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        let rules = self.per_cli.get(cli)?;
        for kind in [
            ScreenState::Hold,
            ScreenState::Blocked,
            ScreenState::Working,
            ScreenState::Idle,
        ] {
            if let Some(rule) = rules.iter().find(|r| r.kind == kind && r.matches(&lines)) {
                return Some(ScreenEvidence {
                    kind,
                    rule_id: rule.id.clone(),
                });
            }
        }
        None
    }

    /// A valid override replaces all bundled states, preserving legacy behavior.
    pub fn load(dir: &Path) -> Self {
        let mut rules = Self::bundled();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return rules;
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
                Ok(profile) => {
                    rules.per_cli.insert(cli, profile);
                }
                Err(e) => eprintln!(
                    "ubra: ignoring invalid detection file {}: {e}",
                    path.display()
                ),
            }
        }
        rules
    }

    fn load_file(path: &Path) -> anyhow::Result<Vec<ScreenRule>> {
        let file: RuleFile = toml::from_str(&std::fs::read_to_string(path)?)?;
        let mut rules = Vec::new();
        for (kind, section) in [
            (ScreenState::Blocked, file.blocked),
            (ScreenState::Working, file.working),
            (ScreenState::Idle, file.idle),
            (ScreenState::Hold, file.hold),
        ] {
            for r in section {
                if r.id.trim().is_empty() || r.contains.is_empty() {
                    continue;
                }
                anyhow::ensure!(
                    (1..=SCREEN_WINDOW_LINES).contains(&r.window_lines),
                    "window_lines must be between 1 and {SCREEN_WINDOW_LINES}"
                );
                anyhow::ensure!(
                    r.contains.iter().all(|p| !normalize(p).is_empty()),
                    "contains markers must not be blank"
                );
                rules.push(ScreenRule {
                    id: r.id,
                    kind,
                    contains: r.contains.iter().map(|s| normalize(s)).collect(),
                    not_contains: r.not_contains.iter().map(|s| normalize(s)).collect(),
                    line_prefixes: r.line_prefixes.iter().map(|s| normalize(s)).collect(),
                    tail_contains: r.tail_contains.iter().map(|s| normalize(s)).collect(),
                    window_lines: r.window_lines,
                    superseded_by_input: None,
                    composer_binding: None,
                });
            }
        }
        Ok(rules)
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn is_screen_blocked(rules: &DetectionRules, cli: &str, screen: &str) -> bool {
    rules
        .evidence(cli, screen)
        .is_some_and(|e| e.kind == ScreenState::Blocked)
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
    fn rules_for(cli: &str, contains: &[&str]) -> DetectionRules {
        DetectionRules {
            per_cli: HashMap::from([(
                cli.to_string(),
                vec![ScreenRule::new(
                    ScreenState::Blocked,
                    "test",
                    contains,
                    SCREEN_WINDOW_LINES,
                )],
            )]),
        }
    }
    #[test]
    fn blocked_requires_all_evidence_and_scopes_to_cli() {
        let rules = rules_for("codex", &["allow this command", "❯"]);
        assert!(is_screen_blocked(
            &rules,
            "codex",
            "Allow This Command\n❯ 1. yes"
        ));
        for screen in [
            "Allow This Command\nworking…",
            "❯ /help\nworking…",
            "plain shell output",
            "",
        ] {
            assert!(!is_screen_blocked(&rules, "codex", screen));
        }
        assert!(!is_screen_blocked(
            &rules,
            "claude",
            "Allow This Command\n❯ 1. yes"
        ));
    }
    #[test]
    fn legacy_window_ignores_lines_above_it() {
        let rules = rules_for("codex", &["allow this command", "❯"]);
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
    fn source_backed_profiles_classify_only_complete_ui_evidence() {
        let rules = DetectionRules::bundled();
        for (cli, screen, state) in [
            (
                "opencode",
                include_str!("../fixtures/agent_screens/opencode_approval.txt"),
                ScreenState::Blocked,
            ),
            (
                "opencode",
                include_str!("../fixtures/agent_screens/opencode_busy.txt"),
                ScreenState::Working,
            ),
            (
                "opencode",
                include_str!("../fixtures/agent_screens/opencode_ready.txt"),
                ScreenState::Idle,
            ),
            (
                "claude",
                include_str!("../fixtures/agent_screens/claude_approval.txt"),
                ScreenState::Blocked,
            ),
            (
                "claude",
                include_str!("../fixtures/agent_screens/claude_busy.txt"),
                ScreenState::Working,
            ),
            (
                "claude",
                include_str!("../fixtures/agent_screens/claude_ready.txt"),
                ScreenState::Idle,
            ),
            (
                "claude",
                include_str!("../fixtures/agent_screens/claude_viewer.txt"),
                ScreenState::Hold,
            ),
            (
                "codex",
                include_str!("../fixtures/agent_screens/codex_approval.txt"),
                ScreenState::Blocked,
            ),
            (
                "codex",
                include_str!("../fixtures/agent_screens/codex_busy.txt"),
                ScreenState::Working,
            ),
            (
                "codex",
                include_str!("../fixtures/agent_screens/codex_ready.txt"),
                ScreenState::Idle,
            ),
            (
                "gemini",
                include_str!("../fixtures/agent_screens/gemini_approval.txt"),
                ScreenState::Blocked,
            ),
            (
                "gemini",
                include_str!("../fixtures/agent_screens/gemini_busy.txt"),
                ScreenState::Working,
            ),
            (
                "gemini",
                include_str!("../fixtures/agent_screens/gemini_ready.txt"),
                ScreenState::Idle,
            ),
            (
                "gemini",
                include_str!("../fixtures/agent_screens/gemini_question.txt"),
                ScreenState::Blocked,
            ),
            (
                "gemini",
                include_str!("../fixtures/agent_screens/gemini_review.txt"),
                ScreenState::Blocked,
            ),
            (
                "mimo",
                include_str!("../fixtures/agent_screens/mimo_approval.txt"),
                ScreenState::Blocked,
            ),
            (
                "mimo",
                include_str!("../fixtures/agent_screens/mimo_question.txt"),
                ScreenState::Blocked,
            ),
            (
                "mimo",
                include_str!("../fixtures/agent_screens/mimo_busy.txt"),
                ScreenState::Working,
            ),
            (
                "mimo",
                include_str!("../fixtures/agent_screens/mimo_ready.txt"),
                ScreenState::Idle,
            ),
            (
                "agy",
                include_str!("../fixtures/agent_screens/agy_approval.txt"),
                ScreenState::Blocked,
            ),
            (
                "agy",
                include_str!("../fixtures/agent_screens/agy_busy.txt"),
                ScreenState::Working,
            ),
            (
                "cursor-agent",
                include_str!("../fixtures/agent_screens/cursor_approval.txt"),
                ScreenState::Blocked,
            ),
        ] {
            assert_eq!(
                rules.evidence(cli, screen).map(|e| e.kind),
                Some(state),
                "{cli}: {screen}"
            );
        }
        for cli in ["codex", "claude", "gemini", "opencode", "aider"] {
            for screen in [
                "",
                "\n\n",
                "Working",
                "Do you want to proceed?",
                "plain output",
            ] {
                assert_eq!(rules.evidence(cli, screen), None, "{cli}: {screen}");
            }
        }
    }
    #[test]
    fn old_approval_before_current_composer_is_not_blocked() {
        let rules = DetectionRules::bundled();
        for (cli, approval, ready) in [
            (
                "claude",
                include_str!("../fixtures/agent_screens/claude_approval.txt"),
                include_str!("../fixtures/agent_screens/claude_ready.txt"),
            ),
            (
                "codex",
                include_str!("../fixtures/agent_screens/codex_approval.txt"),
                include_str!("../fixtures/agent_screens/codex_ready.txt"),
            ),
            (
                "gemini",
                include_str!("../fixtures/agent_screens/gemini_approval.txt"),
                include_str!("../fixtures/agent_screens/gemini_ready.txt"),
            ),
        ] {
            let screen = format!("{approval}\n{ready}");
            assert!(!is_screen_blocked(&rules, cli, &screen), "{cli}");
        }
    }
    #[test]
    fn adjacent_ready_composer_invalidates_old_approval_even_with_cancel_in_tail() {
        let rules = DetectionRules::bundled();
        assert!(!is_screen_blocked(
            &rules,
            "claude",
            "Do you want to proceed?\n❯ 1. Yes\nesc to cancel\n❯ draft"
        ));
        assert!(!is_screen_blocked(
            &rules,
            "claude",
            "Do you want to\nproceed?\n❯ 1. Yes\nesc to cancel\n❯ draft"
        ));
        assert!(!is_screen_blocked(&rules, "codex", "Would you like to run the following command?\n› 1. Yes, proceed\nesc to cancel\n› draft"));
    }
    #[test]
    fn active_busy_evidence_survives_recent_approval_history() {
        let rules = DetectionRules::bundled();
        let screen = "Would you like to run the following command?\n› 1. Yes, proceed\nesc to cancel\nWorking (3s • esc to interrupt)\n›\n? for shortcuts";
        assert_eq!(
            rules.evidence("codex", screen).map(|e| e.kind),
            Some(ScreenState::Working)
        );
    }
    #[test]
    fn newer_composer_invalidates_adjacent_busy_history() {
        let rules = DetectionRules::bundled();
        for (cli, busy, ready) in [
            (
                "codex",
                include_str!("../fixtures/agent_screens/codex_busy.txt"),
                include_str!("../fixtures/agent_screens/codex_ready.txt"),
            ),
            (
                "claude",
                include_str!("../fixtures/agent_screens/claude_busy.txt"),
                include_str!("../fixtures/agent_screens/claude_ready.txt"),
            ),
            (
                "gemini",
                include_str!("../fixtures/agent_screens/gemini_busy.txt"),
                include_str!("../fixtures/agent_screens/gemini_ready.txt"),
            ),
            (
                "opencode",
                include_str!("../fixtures/agent_screens/opencode_busy.txt"),
                include_str!("../fixtures/agent_screens/opencode_ready.txt"),
            ),
            (
                "mimo",
                include_str!("../fixtures/agent_screens/mimo_busy.txt"),
                include_str!("../fixtures/agent_screens/mimo_ready.txt"),
            ),
        ] {
            assert_eq!(
                rules.evidence(cli, busy).map(|e| e.kind),
                Some(ScreenState::Working),
                "{cli}: active busy indicator must survive its own composer"
            );
            assert_ne!(
                rules
                    .evidence(cli, &format!("{busy}\n{ready}"))
                    .map(|e| e.kind),
                Some(ScreenState::Working),
                "{cli}: newer ready composer must invalidate historical busy indicator"
            );
            assert_eq!(
                rules
                    .evidence(cli, &format!("{ready}\n{busy}"))
                    .map(|e| e.kind),
                Some(ScreenState::Working),
                "{cli}: new busy evidence must survive older ready history"
            );
        }
    }
    #[test]
    fn mimo_two_stage_interrupt_and_destructive_approval_match() {
        let rules = DetectionRules::bundled();
        // A second esc press arms `esc again to interrupt`; it must still
        // read as Working, not fall through to Idle or Unknown.
        assert_eq!(
            rules
                .evidence("mimo", "esc again to interrupt  tab switch mode")
                .map(|e| e.kind),
            Some(ScreenState::Working)
        );
        // The destructive-command dialog omits "allow always".
        assert_eq!(
            rules
                .evidence(
                    "mimo",
                    "△ Permission required\n✗ Confirm irreversible deletion\nAllow once  Reject\n⇆ select  enter confirm",
                )
                .map(|e| e.kind),
            Some(ScreenState::Blocked)
        );
    }
    #[test]
    fn cursor_delete_confirm_uses_plain_y_n_hints() {
        let rules = DetectionRules::bundled();
        assert_eq!(
            rules
                .evidence("cursor-agent", "Delete this file?\nDelete (y)  Keep (n)")
                .map(|e| e.kind),
            Some(ScreenState::Blocked)
        );
        // Hints without the delete question match nothing.
        assert_eq!(rules.evidence("cursor-agent", "Delete (y)  Keep (n)"), None);
    }
    #[test]
    fn wrapping_case_and_unused_rows_do_not_change_evidence() {
        let rules = DetectionRules::bundled();
        assert_eq!(
            rules
                .evidence("claude", "❯ draft\n  ?   FOR\n SHORTCUTS\n\n\n")
                .map(|e| e.kind),
            Some(ScreenState::Idle)
        );
        assert_eq!(
            rules
                .evidence("gemini", "Thinking... (esc to\n cancel, 9s)\n\n")
                .map(|e| e.kind),
            Some(ScreenState::Working)
        );
        assert_eq!(
            rules
                .evidence("codex", "› typing a draft\n? for shortcuts")
                .map(|e| e.kind),
            Some(ScreenState::Idle)
        );
    }
    #[test]
    fn stale_markers_far_above_footer_are_not_activity() {
        let rules = DetectionRules::bundled();
        let screen = format!(
            "Working (1s • esc to interrupt)\n{}\n› draft\n? for shortcuts",
            "answer\n".repeat(10)
        );
        assert_eq!(
            rules.evidence("codex", &screen).map(|e| e.kind),
            Some(ScreenState::Idle)
        );
    }
    #[test]
    fn load_override_replaces_all_states_and_invalid_files_are_ignored() {
        let dir = scratch_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("codex.toml"),
            "[[blocked]]\nid = \"custom\"\ncontains = [\"proceed\", \"yes?\"]\n",
        )
        .unwrap();
        std::fs::write(dir.join("bad.toml"), "[[blocked\nnot toml").unwrap();
        std::fs::write(
            dir.join("claude.toml"),
            "[[working]]\nid = \"invalid\"\ncontains = [\"busy\"]\nwindow_lines = 0\n",
        )
        .unwrap();
        std::fs::write(dir.join("pi.toml"), "[[blocked]]\nid = \"empty\"\n").unwrap();
        let rules = DetectionRules::load(&dir);
        assert!(is_screen_blocked(&rules, "codex", "Proceed?\nYes?"));
        assert_eq!(
            rules.evidence(
                "codex",
                include_str!("../fixtures/agent_screens/codex_busy.txt")
            ),
            None
        );
        assert!(is_screen_blocked(
            &rules,
            "claude",
            include_str!("../fixtures/agent_screens/claude_approval.txt")
        ));
        assert_eq!(rules.evidence("pi", "Proceed?\nYes?"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn optional_sections_support_strict_footer_constraints() {
        let dir = scratch_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("custom.toml"), "[[working]]\nid = \"busy\"\ncontains = [\"running\"]\nwindow_lines = 4\nnot_contains = [\"ready\"]\nline_prefixes = [\"status:\"]\ntail_contains = [\"cancel\"]\n\n[[idle]]\nid = \"ready\"\ncontains = [\"ready\"]\n").unwrap();
        let rules = DetectionRules::load(&dir);
        assert_eq!(
            rules
                .evidence("custom", "status: running\nesc cancel")
                .map(|e| e.kind),
            Some(ScreenState::Working)
        );
        assert_eq!(rules.evidence("custom", "output running\nesc cancel"), None);
        assert_eq!(
            rules
                .evidence("custom", "status: running\nready\nesc cancel")
                .map(|e| e.kind),
            Some(ScreenState::Idle)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn hold_rules_win_over_stale_state_markers() {
        let dir = scratch_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("custom.toml"), "[[hold]]\nid = \"viewer\"\ncontains = [\"showing detailed transcript\"]\nwindow_lines = 3\n\n[[blocked]]\nid = \"approval\"\ncontains = [\"do you want to proceed\"]\n").unwrap();
        let rules = DetectionRules::load(&dir);
        // A viewer over stale approval text holds state instead of blocking.
        let screen = "Do you want to proceed?\nShowing detailed transcript\nup/down scroll";
        assert_eq!(
            rules.evidence("custom", screen).map(|e| e.kind),
            Some(ScreenState::Hold)
        );
        // Without the viewer marker the stale text is still evidence.
        assert_eq!(
            rules
                .evidence("custom", "Do you want to proceed?")
                .map(|e| e.kind),
            Some(ScreenState::Blocked)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

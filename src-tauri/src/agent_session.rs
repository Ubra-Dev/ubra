//! Agent session discovery: map a running `(cli, cwd)` to a resumable session.
//!
//! Resume strategies are per-CLI and verified against real installs, never
//! guessed: an unknown CLI yields no session, and its pane keeps today's
//! bare-command restore. Covered CLIs:
//!
//! - `codex`: the newest rollout record under `~/.codex/sessions` whose
//!   recorded cwd matches the pane, resumed via `codex resume <id>`.
//!
//! CLIs resumed without an id (`claude --continue`) need no capture and
//! are handled by the frontend resume table instead.

use std::io::Read as _;
use std::path::{Path, PathBuf};

/// Resumable session reference. Only id-addressed sessions are discovered;
/// path-addressed CLIs stay on bare-command restore until verified.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AgentSessionRef {
    pub kind: SessionRefKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRefKind {
    Id,
}

impl AgentSessionRef {
    fn id(value: String) -> Option<Self> {
        if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
            return None;
        }
        Some(Self {
            kind: SessionRefKind::Id,
            value,
        })
    }
}

/// Newest session for `cli` in `cwd`, or `None` when the CLI has no
/// verified capture. Bounded and total: unreadable stores read as absent.
pub fn discover_session(home: &Path, cli: &str, cwd: &str) -> Option<AgentSessionRef> {
    match cli.to_lowercase().as_str() {
        "codex" => discover_codex(&home.join(".codex").join("sessions"), cwd),
        _ => None,
    }
}

/// Rollout records carry `payload.session_id` and `payload.cwd` on their
/// first line; the newest file for the pane's cwd wins. The walk is
/// depth- and count-bounded so large stores cannot stall the watcher.
fn discover_codex(root: &Path, cwd: &str) -> Option<AgentSessionRef> {
    const MAX_DEPTH: usize = 6;
    const MAX_FILES: usize = 5000;
    const MAX_LINE: u64 = 8192;

    let mut best: Option<(std::time::SystemTime, PathBuf, String)> = None;
    let mut seen = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
                continue;
            }
            if path
                .extension()
                .is_none_or(|ext| !ext.eq_ignore_ascii_case("jsonl"))
            {
                continue;
            }
            seen += 1;
            if seen > MAX_FILES {
                break;
            }
            let (modified, id) = match codex_record(&path, cwd, MAX_LINE) {
                Some(found) => found,
                None => continue,
            };
            let replace = best
                .as_ref()
                .is_none_or(|(mtime, best_path, _)| (modified, &path) > (*mtime, best_path));
            if replace {
                best = Some((modified, path, id));
            }
        }
    }
    best.and_then(|(_, _, id)| AgentSessionRef::id(id))
}

/// First-line `session_meta` for `cwd`: `(mtime, session_id)`. Any parse
/// or shape mismatch reads as absent; only the head line is ever read.
fn codex_record(path: &Path, cwd: &str, max_line: u64) -> Option<(std::time::SystemTime, String)> {
    let file = std::fs::File::open(path).ok()?;
    let modified = file.metadata().and_then(|meta| meta.modified()).ok()?;
    let mut line = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(file.take(max_line)), &mut line)
        .ok()?;
    let record: serde_json::Value = serde_json::from_str(&line).ok()?;
    if record.get("type").and_then(|t| t.as_str()) != Some("session_meta") {
        return None;
    }
    let payload = record.get("payload")?;
    if payload.get("cwd").and_then(|c| c.as_str()) != Some(cwd) {
        return None;
    }
    let id = payload
        .get("session_id")
        .and_then(|id| id.as_str())?
        .to_string();
    (!id.is_empty()).then_some((modified, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch_home() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ubra-agent-session-test-{}-{id}",
            std::process::id()
        ))
    }

    fn rollout(sessions: &Path, name: &str, cwd: &str, id: &str) -> PathBuf {
        let dir = sessions.join("2026").join("10").join("01");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(
            &path,
            serde_json::json!({
                "type": "session_meta",
                "payload": {"session_id": id, "cwd": cwd},
            })
            .to_string(),
        )
        .unwrap();
        // Distinct mtimes order newest-wins deterministically.
        std::thread::sleep(std::time::Duration::from_millis(15));
        path
    }

    #[test]
    fn codex_discovers_newest_session_for_cwd() {
        let home = scratch_home();
        let sessions = home.join(".codex").join("sessions");
        rollout(&sessions, "rollout-old.jsonl", "/work/a", "id-old");
        rollout(&sessions, "rollout-new.jsonl", "/work/a", "id-new");
        rollout(&sessions, "rollout-elsewhere.jsonl", "/work/b", "id-b");

        assert_eq!(
            discover_session(&home, "codex", "/work/a"),
            AgentSessionRef::id("id-new".to_string())
        );
        assert_eq!(
            discover_session(&home, "CODEX", "/work/b"),
            AgentSessionRef::id("id-b".to_string())
        );
        assert_eq!(discover_session(&home, "codex", "/work/missing"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn codex_ignores_malformed_and_foreign_records() {
        let home = scratch_home();
        let sessions = home.join(".codex").join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join("rollout-bad.jsonl"), b"{not json\n").unwrap();
        std::fs::write(
            sessions.join("rollout-event.jsonl"),
            "{\"type\":\"other\",\"payload\":{}}\n",
        )
        .unwrap();
        std::fs::write(
            sessions.join("rollout-nocwd.jsonl"),
            "{\"type\":\"session_meta\",\"payload\":{\"session_id\":\"x\"}}\n",
        )
        .unwrap();
        std::fs::write(sessions.join("notes.txt"), b"not a rollout").unwrap();
        assert_eq!(discover_session(&home, "codex", "/work/a"), None);

        rollout(&sessions, "rollout-good.jsonl", "/work/a", "id-good");
        assert_eq!(
            discover_session(&home, "codex", "/work/a"),
            AgentSessionRef::id("id-good".to_string())
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn unknown_clis_and_missing_stores_yield_no_session() {
        let home = scratch_home();
        assert_eq!(discover_session(&home, "droid", "/work/a"), None);
        assert_eq!(discover_session(&home, "claude", "/work/a"), None);
        assert_eq!(discover_session(&home, "", "/work/a"), None);
        assert_eq!(discover_session(&home, "codex", ""), None);
        // No .codex dir at all.
        assert_eq!(discover_session(&home, "codex", "/work/a"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn session_ids_reject_blanks_and_control_bytes() {
        assert!(AgentSessionRef::id(String::new()).is_none());
        assert!(AgentSessionRef::id("ok-123_ABC".to_string()).is_some());
        assert!(AgentSessionRef::id("bad\nid".to_string()).is_none());
        assert!(AgentSessionRef::id("x".repeat(513)).is_none());
    }
}

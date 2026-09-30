//! Versioned per-pane recovery checkpoints for daemon/GUI restarts.
//!
//! The daemon persists one [`RecoveryRecord`] per stable pane key under
//! `<data-dir>/recovery/`. Records hold the launch configuration, last
//! confirmed directory, dimensions, recent terminal history, exit state,
//! and the exact agent resume reference reported by Ubra integrations.
//! Layout removal closes the runtime session and deletes the record, so
//! stale output can never attach to a different pane.
//!
//! Writes are atomic (temp file + rename) with private permissions, and
//! unreadable/unsupported documents are preserved for the existing
//! recovery workflow instead of being overwritten.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Current recovery-record schema version.
pub const RECOVERY_VERSION: u32 = 1;
/// Schema versions this build can read.
pub const READABLE_VERSIONS: &[u32] = &[1];
/// Directory under the app data dir holding per-pane records.
pub const RECOVERY_DIR: &str = "recovery";
/// Cap on persisted recent history per pane (4 MiB).
pub const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;
/// Records are bounded so one pane cannot exhaust the data dir.
pub const MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;

/// Exact agent conversation reference reported by an integration hook.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReference {
    /// Adapter family id (e.g. `claude`, `codex`, `gemini`).
    pub family: String,
    /// Exact main conversation id or session path; never inferred.
    pub reference: String,
    /// Resume argv (program + args) supplied or built by the adapter.
    pub resume_argv: Vec<String>,
    /// Agent executable used for resume template selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    /// Model to preserve across recovery, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Final state of a naturally exited pane.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitState {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
}

/// Durable per-pane checkpoint. `key` is the stable layout pane id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRecord {
    pub version: u32,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    pub cols: u16,
    pub rows: u16,
    /// Last confirmed working directory (from hooks/OSC 7), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_cwd: Option<String>,
    /// Recent raw terminal history (replayed on recovery), byte-capped.
    #[serde(default)]
    pub history: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<ExitState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentReference>,
}

impl RecoveryRecord {
    pub fn new(key: String, cols: u16, rows: u16) -> Self {
        Self {
            version: RECOVERY_VERSION,
            key,
            shell: None,
            cwd: None,
            args: Vec::new(),
            cols,
            rows,
            last_cwd: None,
            history: String::new(),
            exit: None,
            agent: None,
        }
    }

    /// Trim persisted history to [`MAX_HISTORY_BYTES`] on a char boundary.
    pub fn truncate_history(&mut self) {
        truncate_tail(&mut self.history, MAX_HISTORY_BYTES);
    }
}

/// Keep the last `max_bytes` of `text`, cutting on a char boundary.
pub fn truncate_tail(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let cut = text.len() - max_bytes;
    let boundary = text
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= cut)
        .unwrap_or(text.len());
    text.drain(..boundary);
}

/// Stable filename for a pane key: sanitized stem plus a hash suffix so
/// distinct keys never collide.
pub fn record_file_name(key: &str) -> String {
    let mut stem: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    stem.truncate(48);
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{stem}-{:016x}.json", hasher.finish())
}

pub fn recovery_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(RECOVERY_DIR)
}

/// Load every readable record. Unreadable files are left in place for the
/// recovery workflow; unsupported versions are reported as errors.
pub fn load_all(data_dir: &Path) -> (Vec<RecoveryRecord>, Vec<PathBuf>) {
    let dir = recovery_dir(data_dir);
    let entries: Vec<fs::DirEntry> = fs::read_dir(&dir)
        .map(|entries| entries.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    let mut records = Vec::new();
    let mut unreadable = Vec::new();
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(text) => match parse_record(&text) {
                Ok(record) => records.push(record),
                Err(_) => unreadable.push(path),
            },
            Err(_) => unreadable.push(path),
        }
    }
    (records, unreadable)
}

/// Load one pane's record. Missing files yield `None`; corrupt files
/// yield `None` and stay in place for the recovery workflow.
pub fn load(data_dir: &Path, key: &str) -> Option<RecoveryRecord> {
    let text = fs::read_to_string(recovery_dir(data_dir).join(record_file_name(key))).ok()?;
    parse_record(&text).ok()
}

fn parse_record(text: &str) -> anyhow::Result<RecoveryRecord> {
    anyhow::ensure!(
        text.len() as u64 <= MAX_RECORD_BYTES,
        "recovery record exceeds size limit"
    );
    let value: serde_json::Value = serde_json::from_str(text)?;
    let version = value.get("version").and_then(serde_json::Value::as_u64);
    anyhow::ensure!(
        version.is_some_and(|v| READABLE_VERSIONS.contains(&(v as u32))),
        "unsupported recovery record version"
    );
    let mut record: RecoveryRecord = serde_json::from_value(value)?;
    anyhow::ensure!(!record.key.is_empty(), "recovery record missing key");
    record.truncate_history();
    Ok(record)
}

/// Atomically persist one record with private permissions.
pub fn save(data_dir: &Path, record: &RecoveryRecord) -> anyhow::Result<()> {
    let dir = recovery_dir(data_dir);
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    let mut record = record.clone();
    record.version = RECOVERY_VERSION;
    record.truncate_history();
    let text = serde_json::to_string(&record)?;
    let path = dir.join(record_file_name(&record.key));
    let tmp = dir.join(format!(
        ".recovery-{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        use std::io::Write;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Remove one pane's record. Missing files are not an error.
pub fn remove(data_dir: &Path, key: &str) -> anyhow::Result<()> {
    let path = recovery_dir(data_dir).join(record_file_name(key));
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Remove every record (used when screen-history saving is disabled).
pub fn remove_all(data_dir: &Path) -> usize {
    let dir = recovery_dir(data_dir);
    let mut removed = 0;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json")
                && fs::remove_file(&path).is_ok()
            {
                removed += 1;
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ubra-recovery-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_preserves_record() {
        let dir = scratch();
        let mut record = RecoveryRecord::new("pane-abc".to_string(), 80, 24);
        record.shell = Some("/bin/zsh".to_string());
        record.cwd = Some("/tmp".to_string());
        record.args = vec!["-l".to_string()];
        record.history = "hello".to_string();
        record.agent = Some(AgentReference {
            family: "claude".to_string(),
            reference: "sess-1".to_string(),
            resume_argv: vec![
                "claude".to_string(),
                "--resume".to_string(),
                "sess-1".to_string(),
            ],
            exe: Some("claude".to_string()),
            model: Some("sonnet".to_string()),
        });
        save(&dir, &record).unwrap();
        let (records, unreadable) = load_all(&dir);
        assert!(unreadable.is_empty());
        assert_eq!(records, vec![record]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_is_capped_on_char_boundary() {
        let mut text = "界".repeat(10);
        truncate_tail(&mut text, 7);
        assert!(text.len() <= 7);
        assert_eq!(text, "界界");
    }

    #[test]
    fn corrupt_and_future_documents_are_preserved() {
        let dir = scratch();
        let rec_dir = recovery_dir(&dir);
        fs::create_dir_all(&rec_dir).unwrap();
        fs::write(rec_dir.join("bad.json"), "not json").unwrap();
        fs::write(rec_dir.join("future.json"), r#"{"version":99,"key":"k"}"#).unwrap();
        let (records, unreadable) = load_all(&dir);
        assert!(records.is_empty());
        assert_eq!(unreadable.len(), 2);
        assert!(rec_dir.join("bad.json").is_file());
        assert!(rec_dir.join("future.json").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn filenames_are_stable_and_distinct() {
        let a = record_file_name("pane-1");
        assert_eq!(a, record_file_name("pane-1"));
        assert_ne!(a, record_file_name("pane-2"));
        assert_ne!(record_file_name("a/b"), record_file_name("a_b"));
        assert!(a.ends_with(".json"));
    }

    #[test]
    fn remove_missing_is_ok() {
        let dir = scratch();
        remove(&dir, "pane-nope").unwrap();
        let _ = fs::remove_dir_all(&dir);
    }
}

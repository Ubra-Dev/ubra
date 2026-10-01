//! Layout persistence.
//!
//! The frontend owns the workspace/tab/pane tree; Rust durably stores the
//! versioned JSON document. `UBRA_DATA_DIR` overrides the data directory so
//! tests and scripted E2E runs never touch the real app data dir.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::Manager;

/// Current layout schema version. Bumped only with a migration path.
pub const LAYOUT_VERSION: u32 = 2;
const LAYOUT_FILE: &str = "layout.json";
pub const MAX_LAYOUT_BYTES: u64 = 4 * 1024 * 1024;

struct DocumentSpec {
    file: &'static str,
    backup_stem: &'static str,
    label: &'static str,
    current_version: u32,
    readable_versions: &'static [u32],
}

const LAYOUT_SPEC: DocumentSpec = DocumentSpec {
    file: LAYOUT_FILE,
    backup_stem: "layout",
    label: "layout",
    current_version: LAYOUT_VERSION,
    readable_versions: &[1, 2],
};

/// Resolve the data directory, honoring `UBRA_DATA_DIR` for tests/E2E.
pub fn data_dir(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("UBRA_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(app.path().app_data_dir()?)
}

fn validate_doc_version(value: &serde_json::Value, spec: &DocumentSpec) -> anyhow::Result<()> {
    let version = value.as_object().and_then(|obj| obj.get("version")).and_then(
        |version| version.as_u64(),
    );
    anyhow::ensure!(
        version.is_some_and(|version| spec
            .readable_versions
            .iter()
            .any(|supported| u64::from(*supported) == version)),
        "Unsupported or invalid saved {} version",
        spec.label
    );
    Ok(())
}

fn validate_current_version(value: &serde_json::Value, spec: &DocumentSpec) -> anyhow::Result<()> {
    anyhow::ensure!(
        value.is_object()
            && value.get("version").and_then(serde_json::Value::as_u64)
                == Some(u64::from(spec.current_version)),
        "Unsupported or invalid saved {} version",
        spec.label
    );
    Ok(())
}

/// Load only supported documents. Missing is distinct from every read/parse error.
fn load_doc_from(dir: &Path, spec: &DocumentSpec) -> anyhow::Result<Option<serde_json::Value>> {
    let path = dir.join(spec.file);
    let file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return match fs::symlink_metadata(&path) {
                Err(metadata_error) if metadata_error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(None)
                }
                // An existing dangling link is unreadable, not an absent document.
                Ok(_) => Err(e.into()),
                Err(metadata_error) => Err(metadata_error.into()),
            };
        }
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_LAYOUT_BYTES + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_LAYOUT_BYTES,
        "Saved {} exceeds the {MAX_LAYOUT_BYTES} byte limit",
        spec.label
    );
    let document: serde_json::Value = serde_json::from_slice(&bytes)?;
    validate_doc_version(&document, spec)?;
    Ok(Some(document))
}

/// Export an exact-byte backup without parsing or imposing the size limit.
/// Failure never changes the original. Create-new avoids overwriting a prior export.
fn backup_doc_from(dir: &Path, spec: &DocumentSpec) -> anyhow::Result<String> {
    let mut source = fs::File::open(dir.join(spec.file))?;
    anyhow::ensure!(
        source.metadata()?.is_file(),
        "Saved {} is not a regular file",
        spec.label
    );
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|e| anyhow::anyhow!("Backup name entropy failed: {e}"))?;
    let token = format!("{:032x}", u128::from_be_bytes(random));
    let path = dir.join(format!("{}.backup-{token}.json", spec.backup_stem));
    let mut target = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    if let Err(error) = std::io::copy(&mut source, &mut target).and_then(|_| target.sync_all()) {
        let _ = fs::remove_file(&path);
        return Err(error.into());
    }
    Ok(path.to_string_lossy().into_owned())
}

/// Explicit reset consent: preserve the original before replacing it, even if invalid.
fn reset_doc_to(
    dir: &Path,
    spec: &DocumentSpec,
    document: &serde_json::Value,
) -> anyhow::Result<Option<String>> {
    validate_current_version(document, spec)?;
    let backup = match fs::symlink_metadata(dir.join(spec.file)) {
        Ok(_) => Some(backup_doc_from(dir, spec)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let Err(error) = write_doc_to(dir, spec, document) {
        return Err(match &backup {
            Some(path) => anyhow::anyhow!("Reset failed; original backup is at {path}: {error}"),
            None => error,
        });
    }
    Ok(backup)
}

/// Ordinary autosave refuses to overwrite unreadable/corrupt/unsupported documents.
fn save_doc_to(
    dir: &Path,
    spec: &DocumentSpec,
    document: &serde_json::Value,
) -> anyhow::Result<()> {
    load_doc_from(dir, spec)?;
    write_doc_to(dir, spec, document)
}

fn write_doc_to(
    dir: &Path,
    spec: &DocumentSpec,
    document: &serde_json::Value,
) -> anyhow::Result<()> {
    validate_current_version(document, spec)?;
    let bytes = serde_json::to_vec_pretty(document)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_LAYOUT_BYTES,
        "Saved {} exceeds the {MAX_LAYOUT_BYTES} byte limit",
        spec.label
    );
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{}.tmp", spec.file));
    let mut file = fs::File::create(&tmp)?;
    file.write_all(&bytes)?;
    drop(file);
    fs::rename(&tmp, dir.join(spec.file))?;
    Ok(())
}

/// Load only supported documents. Missing is distinct from every read/parse error.
pub fn load_layout_from(dir: &Path) -> anyhow::Result<Option<serde_json::Value>> {
    load_doc_from(dir, &LAYOUT_SPEC)
}

/// Export an exact-byte backup without parsing or imposing the layout size limit.
/// Failure never changes the original. Create-new avoids overwriting a prior export.
pub fn backup_layout_from(dir: &Path) -> anyhow::Result<String> {
    backup_doc_from(dir, &LAYOUT_SPEC)
}

/// Explicit reset consent: preserve the original before replacing it, even if invalid.
pub fn reset_layout_to(dir: &Path, layout: &serde_json::Value) -> anyhow::Result<Option<String>> {
    reset_doc_to(dir, &LAYOUT_SPEC, layout)
}

/// Ordinary autosave refuses to overwrite unreadable/corrupt/unsupported documents.
pub fn save_layout_to(dir: &Path, layout: &serde_json::Value) -> anyhow::Result<()> {
    save_doc_to(dir, &LAYOUT_SPEC, layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ubra-layout-test-{}-{id}", std::process::id()))
    }

    fn current_layout(extra: serde_json::Value) -> serde_json::Value {
        let mut layout = serde_json::json!({
            "version": LAYOUT_VERSION,
            "workspaces": [{ "id": "w1", "name": "demo", "tabs": [], "activeTabId": "" }],
            "activeWorkspaceId": "w1",
        });
        if let (Some(map), Some(patch)) = (layout.as_object_mut(), extra.as_object()) {
            for (key, value) in patch {
                map.insert(key.clone(), value.clone());
            }
        }
        layout
    }

    #[test]
    fn missing_layout_loads_as_none() {
        let dir = scratch_dir();
        assert!(load_layout_from(&dir).unwrap().is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = scratch_dir();
        let layout = current_layout(serde_json::json!({}));
        save_layout_to(&dir, &layout).unwrap();
        assert_eq!(load_layout_from(&dir).unwrap(), Some(layout));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_layout_version_still_loads_without_rewriting() {
        let dir = scratch_dir();
        fs::create_dir_all(&dir).unwrap();
        let legacy = serde_json::json!({"version": 1, "workspaces": [], "activeWorkspaceId": "w"});
        fs::write(dir.join(LAYOUT_FILE), serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(load_layout_from(&dir).unwrap(), Some(legacy));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_layout_is_an_error() {
        let dir = scratch_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(LAYOUT_FILE), b"{not json").unwrap();
        assert!(load_layout_from(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_saved_layout_is_an_error_not_first_run() {
        let dir = scratch_dir();
        fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink("missing-target", dir.join(LAYOUT_FILE)).unwrap();
        assert!(load_layout_from(&dir).is_err());
        assert!(save_layout_to(&dir, &current_layout(serde_json::json!({}))).is_err());
        assert_eq!(
            fs::read_link(dir.join(LAYOUT_FILE)).unwrap(),
            PathBuf::from("missing-target")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_documents_cannot_be_overwritten_by_autosave() {
        for original in [b"{not json".as_slice(), b"{\"version\":999}", b"null"] {
            let dir = scratch_dir();
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(LAYOUT_FILE), original).unwrap();
            assert!(load_layout_from(&dir).is_err());
            assert!(save_layout_to(&dir, &current_layout(serde_json::json!({}))).is_err());
            assert_eq!(fs::read(dir.join(LAYOUT_FILE)).unwrap(), original);
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn export_and_explicit_reset_preserve_exact_original_then_resume_saving() {
        let dir = scratch_dir();
        fs::create_dir_all(&dir).unwrap();
        let original = b"{\"version\":999,\"unrecognized\":\"keep me\"}\n";
        fs::write(dir.join(LAYOUT_FILE), original).unwrap();
        let export = backup_layout_from(&dir).unwrap();
        assert_eq!(fs::read(&export).unwrap(), original);
        assert_eq!(fs::read(dir.join(LAYOUT_FILE)).unwrap(), original);
        let fresh = current_layout(serde_json::json!({"workspaces": []}));
        let backup = reset_layout_to(&dir, &fresh).unwrap().unwrap();
        assert_ne!(backup, export);
        assert_eq!(fs::read(&backup).unwrap(), original);
        assert_eq!(load_layout_from(&dir).unwrap(), Some(fresh));
        let next = current_layout(serde_json::json!({"workspaces": [{"id": "new"}]}));
        save_layout_to(&dir, &next).unwrap();
        assert_eq!(load_layout_from(&dir).unwrap(), Some(next));
        assert_eq!(fs::read(&backup).unwrap(), original);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_original_blocks_reset_instead_of_becoming_first_run() {
        let dir = scratch_dir();
        fs::create_dir_all(dir.join(LAYOUT_FILE)).unwrap();
        assert!(load_layout_from(&dir).is_err());
        assert!(backup_layout_from(&dir).is_err());
        assert!(reset_layout_to(&dir, &current_layout(serde_json::json!({}))).is_err());
        assert!(dir.join(LAYOUT_FILE).is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_document_is_preserved_and_can_still_be_exported() {
        let dir = scratch_dir();
        fs::create_dir_all(&dir).unwrap();
        let original = vec![b' '; MAX_LAYOUT_BYTES as usize + 1];
        fs::write(dir.join(LAYOUT_FILE), &original).unwrap();
        assert!(load_layout_from(&dir).is_err());
        assert!(save_layout_to(&dir, &current_layout(serde_json::json!({}))).is_err());
        let backup = backup_layout_from(&dir).unwrap();
        assert_eq!(fs::read(&backup).unwrap(), original);
        assert_eq!(fs::read(dir.join(LAYOUT_FILE)).unwrap(), original);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_replacement_keeps_original_and_backup() {
        let dir = scratch_dir();
        fs::create_dir_all(&dir).unwrap();
        let original = b"broken but valuable";
        fs::write(dir.join(LAYOUT_FILE), original).unwrap();
        fs::create_dir(dir.join(format!("{LAYOUT_FILE}.tmp"))).unwrap();
        assert!(reset_layout_to(&dir, &current_layout(serde_json::json!({}))).is_err());
        assert_eq!(fs::read(dir.join(LAYOUT_FILE)).unwrap(), original);
        let backups: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("layout.backup-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), original);
        let _ = fs::remove_dir_all(&dir);
    }

}

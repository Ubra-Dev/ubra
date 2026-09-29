//! Layout persistence (Phase 2).
//!
//! The frontend owns the workspace/tab/pane tree; Rust durably stores the
//! versioned JSON document. `UBRA_DATA_DIR` overrides the data directory so
//! tests and scripted E2E runs never touch the real app data dir.

use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// Current layout schema version. Bumped only with a migration path.
pub const LAYOUT_VERSION: u32 = 1;
const LAYOUT_FILE: &str = "layout.json";

/// Resolve the data directory, honoring `UBRA_DATA_DIR` for tests/E2E.
pub fn data_dir(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("UBRA_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(app.path().app_data_dir()?)
}

/// Load the saved layout, or `None` when no layout was saved yet.
pub fn load_layout_from(dir: &Path) -> anyhow::Result<Option<serde_json::Value>> {
    let path = dir.join(LAYOUT_FILE);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Save the layout atomically (write temp file, then rename).
pub fn save_layout_to(dir: &Path, layout: &serde_json::Value) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{LAYOUT_FILE}.tmp"));
    fs::write(&tmp, serde_json::to_string_pretty(layout)?)?;
    fs::rename(&tmp, dir.join(LAYOUT_FILE))?;
    Ok(())
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

    #[test]
    fn missing_layout_loads_as_none() {
        let dir = scratch_dir();
        assert!(load_layout_from(&dir).unwrap().is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = scratch_dir();
        let layout = serde_json::json!({
            "version": LAYOUT_VERSION,
            "workspaces": [{ "id": "w1", "name": "demo", "tabs": [], "activeTabId": "" }],
            "activeWorkspaceId": "w1",
        });
        save_layout_to(&dir, &layout).unwrap();
        assert_eq!(load_layout_from(&dir).unwrap(), Some(layout));
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
}

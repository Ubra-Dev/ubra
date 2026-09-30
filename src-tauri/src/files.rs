//! Workspace file listing for the Explorer panel.
//!
//! One-level directory reads scoped to a caller-provided root. Every path is
//! canonicalized and required to stay inside the canonical root, so `..` and
//! symlink escapes are rejected instead of silently read.

use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Largest single listing; larger directories set `truncated`.
pub const MAX_DIR_ENTRIES: usize = 5000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub entries: Vec<DirEntry>,
    pub truncated: bool,
}

/// List one directory level under `root`. `rel_path` is "" for the root
/// itself; relative paths use "/" separators on every platform.
pub fn list_dir(root: &str, rel_path: &str) -> Result<DirListing, String> {
    if root.contains('\0') || rel_path.contains('\0') {
        return Err("Path must not contain NUL bytes.".to_string());
    }
    if Path::new(rel_path).is_absolute() {
        return Err("Path must be relative to the workspace.".to_string());
    }
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("Cannot open workspace folder: {e}"))?;
    let joined = if rel_path.is_empty() || rel_path == "." {
        canonical_root.clone()
    } else {
        canonical_root.join(rel_path)
    };
    let canonical_target =
        fs::canonicalize(&joined).map_err(|_| "Cannot open folder.".to_string())?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err("Folder is outside the workspace.".to_string());
    }
    let read = fs::read_dir(&canonical_target).map_err(|_| "Cannot read folder.".to_string())?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in read {
        let entry = entry.map_err(|_| "Cannot read folder.".to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        // symlink_metadata does not follow links; a symlink's directory-ness
        // follows its target so linked folders expand, and expanding it
        // re-enters list_dir where canonicalization enforces containment.
        let meta =
            fs::symlink_metadata(entry.path()).map_err(|_| format!("Cannot stat '{name}'."))?;
        let file_type = meta.file_type();
        let is_dir = file_type.is_dir()
            || (file_type.is_symlink() && fs::metadata(entry.path()).is_ok_and(|m| m.is_dir()));
        entries.push(DirEntry {
            name,
            is_dir,
            is_symlink: file_type.is_symlink(),
            size: if file_type.is_file() { meta.len() } else { 0 },
            modified_ms: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64),
        });
        if entries.len() > MAX_DIR_ENTRIES {
            entries.pop();
            truncated = true;
            break;
        }
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(DirListing { entries, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_dir(id: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ubra-files-test-{}-{id}", std::process::id()))
    }

    fn remove_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn lists_one_level_sorted_with_git_skipped() {
        let dir = test_dir("list");
        remove_dir(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::create_dir_all(dir.join("sub/nested")).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join("b.txt"), "b").unwrap();
        fs::write(dir.join("A.txt"), "a").unwrap();
        fs::write(dir.join(".hidden"), "h").unwrap();

        let root = dir.to_string_lossy().into_owned();
        let listing = list_dir(&root, "").unwrap();
        assert!(!listing.truncated);
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        // Directories first, then case-insensitive name order; .git skipped.
        assert_eq!(names, vec!["sub", ".hidden", "A.txt", "b.txt"]);
        assert!(listing.entries[0].is_dir);
        assert!(!listing.entries[0].is_symlink);
        assert_eq!(listing.entries[0].size, 0);
        assert_eq!(listing.entries[2].size, 1);
        assert!(listing.entries.iter().all(|e| e.modified_ms.is_some()));

        // Relisting a child returns only that level.
        let child = list_dir(&root, "sub").unwrap();
        let child_names: Vec<&str> = child.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(child_names, vec!["nested"]);

        remove_dir(&dir);
    }

    #[test]
    fn rejects_absolute_paths_nul_and_escapes() {
        let dir = test_dir("escapes");
        remove_dir(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        let root = dir.to_string_lossy().into_owned();

        assert!(list_dir(&root, "/etc").unwrap_err().contains("relative"));
        assert!(list_dir("has\0nul", "").unwrap_err().contains("NUL"));
        assert!(list_dir(&root, "a\0b").unwrap_err().contains("NUL"));
        assert!(list_dir(&root, "..").unwrap_err().contains("outside"));
        assert!(list_dir(&root, "sub/../..")
            .unwrap_err()
            .contains("outside"));
        // A `..` that stays inside the root is harmless.
        assert!(list_dir(&root, "sub/..").is_ok());

        remove_dir(&dir);
    }

    #[test]
    fn rejects_missing_and_non_directory_targets() {
        let dir = test_dir("missing");
        remove_dir(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("file.txt"), "x").unwrap();
        let root = dir.to_string_lossy().into_owned();

        assert!(list_dir(&root, "nope").is_err());
        assert!(list_dir(&root, "file.txt").is_err());
        assert!(list_dir(dir.join("nope").to_str().unwrap(), "").is_err());

        remove_dir(&dir);
    }

    #[test]
    fn truncates_huge_directories() {
        let dir = test_dir("truncate");
        remove_dir(&dir);
        fs::create_dir_all(&dir).unwrap();
        for i in 0..MAX_DIR_ENTRIES + 3 {
            fs::write(dir.join(format!("f{i:05}.txt")), "x").unwrap();
        }
        let listing = list_dir(&dir.to_string_lossy(), "").unwrap();
        assert!(listing.truncated);
        assert_eq!(listing.entries.len(), MAX_DIR_ENTRIES);

        remove_dir(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escapes_but_allows_inner_links() {
        use std::os::unix::fs::symlink;

        let dir = test_dir("symlink");
        let outside = test_dir("symlink-outside");
        remove_dir(&dir);
        remove_dir(&outside);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, dir.join("escape")).unwrap();
        symlink(dir.join("sub"), dir.join("inner")).unwrap();
        fs::write(dir.join("real.txt"), "x").unwrap();
        symlink(dir.join("real.txt"), dir.join("filelink")).unwrap();
        let root = dir.to_string_lossy().into_owned();

        let listing = list_dir(&root, "").unwrap();
        let escape = listing.entries.iter().find(|e| e.name == "escape").unwrap();
        assert!(escape.is_symlink);
        assert!(escape.is_dir);
        let filelink = listing
            .entries
            .iter()
            .find(|e| e.name == "filelink")
            .unwrap();
        assert!(filelink.is_symlink);
        assert!(!filelink.is_dir);
        assert!(list_dir(&root, "escape").unwrap_err().contains("outside"));
        assert!(list_dir(&root, "inner").is_ok());

        remove_dir(&dir);
        remove_dir(&outside);
    }
}

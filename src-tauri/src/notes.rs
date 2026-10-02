//! Markdown note storage for the Notes panel.
//!
//! Plain `.md` files under the app-data directory, split into a `global`
//! scope shared by every workspace and one `workspaces/<id>` scope per Ubra
//! workspace. Workspace ids are generated (`ws-<uuid>`), never user paths,
//! so notes survive project-folder moves and exist before any folder is set.
//! All writes are atomic (tmp file + rename) and every name/id is validated
//! so `..` and separators can never escape the notes directory.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Largest single note accepted for read/write; larger files are refused.
pub const MAX_NOTE_BYTES: usize = 1024 * 1024;

/// Largest listing; larger scopes set `truncated`.
pub const MAX_NOTES_LISTED: usize = 2000;

/// Longest search query; longer queries are rejected.
pub const MAX_QUERY_CHARS: usize = 200;

/// Most hits returned per search; the rest set `truncated`.
pub const MAX_SEARCH_HITS: usize = 100;

/// Bytes read from each note while listing titles and searching.
const SCAN_BYTES: u64 = 256 * 1024;

/// Leading bytes sniffed for NUL to detect binary files.
const BINARY_SNIFF_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteScope {
    Global,
    Workspace,
}

impl NoteScope {
    fn parse(scope: &str) -> Result<Self, String> {
        match scope {
            "global" => Ok(NoteScope::Global),
            "workspace" => Ok(NoteScope::Workspace),
            _ => Err("Scope must be \"global\" or \"workspace\".".to_string()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            NoteScope::Global => "global",
            NoteScope::Workspace => "workspace",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteEntry {
    /// File name without the `.md` suffix; the note's id within its scope.
    pub name: String,
    pub title: String,
    pub updated_ms: Option<u64>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteListing {
    pub entries: Vec<NoteEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteContent {
    pub content: String,
    pub truncated: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteHit {
    pub scope: String,
    pub name: String,
    pub title: String,
    pub snippet: String,
    pub updated_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSearchResults {
    pub hits: Vec<NoteHit>,
    pub truncated: bool,
}

/// Validate a workspace id: generated ids only (`ws-<uuid>` shape).
fn check_workspace_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 128 {
        return Err("Workspace id is invalid.".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Workspace id is invalid.".to_string());
    }
    Ok(())
}

/// Validate a note name (no suffix): single path segment, no escapes.
fn check_note_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 100 {
        return Err("Note name is invalid.".to_string());
    }
    if name.starts_with('.') || name.contains("..") {
        return Err("Note name is invalid.".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' || c == '.')
    {
        return Err("Note name is invalid.".to_string());
    }
    Ok(())
}

fn scope_dir(
    data_dir: &Path,
    scope: NoteScope,
    workspace_id: Option<&str>,
) -> Result<PathBuf, String> {
    let notes = data_dir.join("notes");
    match scope {
        NoteScope::Global => Ok(notes.join("global")),
        NoteScope::Workspace => {
            let id = workspace_id.unwrap_or("");
            check_workspace_id(id)?;
            Ok(notes.join("workspaces").join(id))
        }
    }
}

fn note_path(dir: &Path, name: &str) -> Result<PathBuf, String> {
    check_note_name(name)?;
    Ok(dir.join(format!("{name}.md")))
}

fn modified_ms(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// Title from the first `# ` heading, else the first non-empty line, else the name.
fn title_for(content: &str, name: &str) -> String {
    for line in content.lines() {
        let line = line.trim();
        if let Some(heading) = line.strip_prefix("# ") {
            let heading = heading.trim();
            if !heading.is_empty() {
                return truncate(heading, 120);
            }
        }
    }
    for line in content.lines() {
        let line = line.trim().trim_start_matches('#').trim();
        if !line.is_empty() {
            return truncate(line, 120);
        }
    }
    name.to_string()
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let end = text
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    format!("{}…", &text[..end])
}

/// Read at most `limit + 1` bytes to detect truncation; refuse binary files.
fn read_bounded(path: &Path, limit: usize) -> Result<(String, bool, u64), String> {
    use std::io::Read;
    let meta = fs::metadata(path).map_err(|_| "Cannot open note.".to_string())?;
    if !meta.is_file() {
        return Err("Note is not a file.".to_string());
    }
    let size = meta.len();
    let file = fs::File::open(path).map_err(|_| "Cannot open note.".to_string())?;
    let mut buf = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|_| "Cannot read note.".to_string())?;
    let truncated = buf.len() > limit;
    if truncated {
        buf.truncate(limit);
    }
    let sniff_end = buf.len().min(BINARY_SNIFF_BYTES);
    if buf[..sniff_end].contains(&0) {
        return Err("Note is not a text file.".to_string());
    }
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated, size))
}

fn entry_for(dir: &Path, name: &str) -> Option<NoteEntry> {
    let path = dir.join(format!("{name}.md"));
    let meta = fs::metadata(&path).ok()?;
    if !meta.is_file() {
        return None;
    }
    // Titles come from a bounded head read; an unreadable note still lists.
    let head = read_bounded(&path, 4096)
        .map(|(text, _, _)| text)
        .unwrap_or_default();
    Some(NoteEntry {
        title: title_for(&head, name),
        name: name.to_string(),
        updated_ms: modified_ms(&path),
        size: meta.len(),
    })
}

/// List one scope's notes, newest first. A missing scope is an empty list.
pub fn list_notes(
    data_dir: &Path,
    scope: &str,
    workspace_id: Option<&str>,
) -> Result<NoteListing, String> {
    let dir = scope_dir(data_dir, NoteScope::parse(scope)?, workspace_id)?;
    let read = match fs::read_dir(&dir) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NoteListing {
                entries: Vec::new(),
                truncated: false,
            });
        }
        Err(_) => return Err("Cannot read notes folder.".to_string()),
    };
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in read {
        let entry = entry.map_err(|_| "Cannot read notes folder.".to_string())?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(name) = file_name.strip_suffix(".md") else {
            continue;
        };
        if check_note_name(name).is_err() {
            continue;
        }
        if let Some(note) = entry_for(&dir, name) {
            entries.push(note);
            if entries.len() > MAX_NOTES_LISTED {
                entries.pop();
                truncated = true;
                break;
            }
        }
    }
    entries.sort_by(|a, b| {
        b.updated_ms
            .cmp(&a.updated_ms)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(NoteListing { entries, truncated })
}

/// Read a note's full text (up to `MAX_NOTE_BYTES`).
pub fn read_note(
    data_dir: &Path,
    scope: &str,
    workspace_id: Option<&str>,
    name: &str,
) -> Result<NoteContent, String> {
    let dir = scope_dir(data_dir, NoteScope::parse(scope)?, workspace_id)?;
    let path = note_path(&dir, name)?;
    let (content, truncated, size) = read_bounded(&path, MAX_NOTE_BYTES)?;
    if size > MAX_NOTE_BYTES as u64 {
        return Err("Note is too large to open.".to_string());
    }
    Ok(NoteContent {
        content,
        truncated,
        size,
    })
}

/// Create or overwrite a note atomically (tmp file + rename).
pub fn write_note(
    data_dir: &Path,
    scope: &str,
    workspace_id: Option<&str>,
    name: &str,
    content: &str,
) -> Result<NoteEntry, String> {
    if content.len() > MAX_NOTE_BYTES {
        return Err("Note is too large to save.".to_string());
    }
    if content.contains('\0') {
        return Err("Note must not contain NUL bytes.".to_string());
    }
    let dir = scope_dir(data_dir, NoteScope::parse(scope)?, workspace_id)?;
    let path = note_path(&dir, name)?;
    fs::create_dir_all(&dir).map_err(|_| "Cannot save note.".to_string())?;
    let tmp = dir.join(format!(".{name}.tmp"));
    if let Err(e) = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &path)?;
        Ok(())
    })() {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Cannot save note: {e}"));
    }
    entry_for(&dir, name).ok_or_else(|| "Cannot save note.".to_string())
}

/// Rename a note within its scope.
pub fn rename_note(
    data_dir: &Path,
    scope: &str,
    workspace_id: Option<&str>,
    old_name: &str,
    new_name: &str,
) -> Result<NoteEntry, String> {
    if old_name == new_name {
        let dir = scope_dir(data_dir, NoteScope::parse(scope)?, workspace_id)?;
        note_path(&dir, old_name)?;
        return entry_for(&dir, old_name).ok_or_else(|| "Note does not exist.".to_string());
    }
    let dir = scope_dir(data_dir, NoteScope::parse(scope)?, workspace_id)?;
    let from = note_path(&dir, old_name)?;
    let to = note_path(&dir, new_name)?;
    if fs::symlink_metadata(&to).is_ok() {
        return Err("A note with that name already exists.".to_string());
    }
    fs::rename(&from, &to).map_err(|_| "Cannot rename note.".to_string())?;
    entry_for(&dir, new_name).ok_or_else(|| "Cannot rename note.".to_string())
}

/// Delete a note; deleting a missing note is a no-op success.
pub fn delete_note(
    data_dir: &Path,
    scope: &str,
    workspace_id: Option<&str>,
    name: &str,
) -> Result<(), String> {
    let dir = scope_dir(data_dir, NoteScope::parse(scope)?, workspace_id)?;
    let path = note_path(&dir, name)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Cannot delete note.".to_string()),
    }
}

/// Case-insensitive substring search across global notes plus one workspace
/// scope (when given), newest hits first.
pub fn search_notes(
    data_dir: &Path,
    query: &str,
    workspace_id: Option<&str>,
) -> Result<NoteSearchResults, String> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
        return Err("Search query is invalid.".to_string());
    }
    let needle = query.to_lowercase();
    let mut scopes = vec![(NoteScope::Global, None)];
    if let Some(id) = workspace_id {
        check_workspace_id(id)?;
        scopes.push((NoteScope::Workspace, Some(id)));
    }
    let mut hits = Vec::new();
    let mut truncated = false;
    'outer: for (scope, id) in scopes {
        let dir = scope_dir(data_dir, scope, id)?;
        let read = match fs::read_dir(&dir) {
            Ok(read) => read,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Cannot search notes.".to_string()),
        };
        for entry in read {
            let entry = entry.map_err(|_| "Cannot search notes.".to_string())?;
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let Some(name) = file_name.strip_suffix(".md") else {
                continue;
            };
            if check_note_name(name).is_err() {
                continue;
            }
            let path = dir.join(format!("{name}.md"));
            let Ok((text, _, _)) = read_bounded(&path, SCAN_BYTES as usize) else {
                continue;
            };
            // Case-insensitive match against the name and the body. The match
            // offset locates the snippet in the original text; lowercasing can
            // shift non-ASCII byte offsets, but `snippet_for` clamps and snaps
            // to char boundaries so the window stays valid.
            let body_at = text.to_lowercase().find(&needle);
            if body_at.is_none() && !name.to_lowercase().contains(&needle) {
                continue;
            }
            let snippet = match body_at {
                Some(at) => snippet_for(&text, at),
                None => truncate(&text.split_whitespace().collect::<Vec<_>>().join(" "), 160),
            };
            if hits.len() >= MAX_SEARCH_HITS {
                truncated = true;
                break 'outer;
            }
            hits.push(NoteHit {
                scope: scope.as_str().to_string(),
                title: title_for(&text, name),
                name: name.to_string(),
                snippet,
                updated_ms: modified_ms(&path),
            });
        }
    }
    hits.sort_by(|a, b| {
        b.updated_ms
            .cmp(&a.updated_ms)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    Ok(NoteSearchResults { hits, truncated })
}

/// ~120-char window around a byte offset, snapped to char boundaries.
fn snippet_for(text: &str, at: usize) -> String {
    const RADIUS: usize = 60;
    let at = at.min(text.len());
    let bounds: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let lo = at.saturating_sub(RADIUS);
    let hi = at.saturating_add(RADIUS);
    let start = bounds
        .iter()
        .rev()
        .find(|&&i| i <= lo)
        .copied()
        .unwrap_or(0);
    let end = bounds
        .iter()
        .find(|&&i| i >= hi)
        .copied()
        .unwrap_or(text.len());
    let mut snippet: String = text[start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if start > 0 {
        snippet = format!("…{snippet}");
    }
    if end < text.len() {
        snippet.push('…');
    }
    truncate(&snippet, 160)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ubra-notes-test-{}-{id}", std::process::id()))
    }

    fn remove_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn crud_round_trip_with_titles_newest_first() {
        let dir = scratch_dir();
        remove_dir(&dir);
        let ws = Some("ws-abc-123");

        // Missing scopes list as empty, never an error.
        assert!(list_notes(&dir, "global", None).unwrap().entries.is_empty());
        assert!(list_notes(&dir, "workspace", ws)
            .unwrap()
            .entries
            .is_empty());

        let first = write_note(&dir, "workspace", ws, "first-note", "# First\nhello\n").unwrap();
        assert_eq!(first.title, "First");
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = write_note(&dir, "workspace", ws, "second", "no heading here\n").unwrap();
        assert_eq!(second.title, "no heading here");

        let listing = list_notes(&dir, "workspace", ws).unwrap();
        assert!(!listing.truncated);
        assert_eq!(listing.entries.len(), 2);
        // Newest first.
        assert_eq!(listing.entries[0].name, "second");
        assert_eq!(listing.entries[1].name, "first-note");

        // Scopes are isolated.
        assert!(list_notes(&dir, "global", None).unwrap().entries.is_empty());

        let content = read_note(&dir, "workspace", ws, "first-note").unwrap();
        assert_eq!(content.content, "# First\nhello\n");
        assert!(!content.truncated);

        let renamed = rename_note(&dir, "workspace", ws, "first-note", "renamed").unwrap();
        assert_eq!(renamed.name, "renamed");
        assert!(read_note(&dir, "workspace", ws, "first-note").is_err());
        assert!(read_note(&dir, "workspace", ws, "renamed").is_ok());

        // Rename onto an existing note is refused.
        assert!(rename_note(&dir, "workspace", ws, "renamed", "second").is_err());

        delete_note(&dir, "workspace", ws, "renamed").unwrap();
        // Deleting a missing note succeeds.
        delete_note(&dir, "workspace", ws, "renamed").unwrap();
        let listing = list_notes(&dir, "workspace", ws).unwrap();
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(first.size, 14);

        remove_dir(&dir);
    }

    #[test]
    fn rejects_bad_scopes_ids_and_names() {
        let dir = scratch_dir();
        remove_dir(&dir);

        assert!(list_notes(&dir, "bogus", None).is_err());
        assert!(list_notes(&dir, "workspace", None).is_err());
        assert!(list_notes(&dir, "workspace", Some("")).is_err());
        assert!(list_notes(&dir, "workspace", Some("../x")).is_err());
        assert!(list_notes(&dir, "workspace", Some("a/b")).is_err());

        assert!(write_note(&dir, "global", None, "", "x").is_err());
        assert!(write_note(&dir, "global", None, "../escape", "x").is_err());
        assert!(write_note(&dir, "global", None, "a/b", "x").is_err());
        assert!(write_note(&dir, "global", None, ".hidden", "x").is_err());
        assert!(write_note(&dir, "global", None, "has\0nul", "x").is_err());
        assert!(write_note(&dir, "global", None, "ok-name", "has\0nul").is_err());

        // Nothing escaped the data dir.
        assert!(!dir.join("escape.md").exists());
        assert!(!dir.parent().unwrap().join("escape.md").exists());

        remove_dir(&dir);
    }

    #[test]
    fn refuses_oversize_notes() {
        let dir = scratch_dir();
        remove_dir(&dir);
        let big = "x".repeat(MAX_NOTE_BYTES + 1);
        assert!(write_note(&dir, "global", None, "big", &big).is_err());
        // A hand-placed oversize file cannot be opened either.
        let scope = dir.join("notes").join("global");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("big.md"), &big).unwrap();
        assert!(read_note(&dir, "global", None, "big").is_err());
        // …but it still lists (titles read a bounded head).
        assert_eq!(list_notes(&dir, "global", None).unwrap().entries.len(), 1);
        remove_dir(&dir);
    }

    #[test]
    fn search_finds_across_scopes_with_snippets() {
        let dir = scratch_dir();
        remove_dir(&dir);
        let ws = Some("ws-1");
        write_note(
            &dir,
            "global",
            None,
            "shopping",
            "# Lists\nbuy milk tomorrow\n",
        )
        .unwrap();
        write_note(&dir, "workspace", ws, "todo", "remember the MILK run\n").unwrap();
        write_note(&dir, "workspace", ws, "other", "nothing relevant\n").unwrap();

        let results = search_notes(&dir, "milk", ws).unwrap();
        assert!(!results.truncated);
        assert_eq!(results.hits.len(), 2);
        let scopes: Vec<&str> = results.hits.iter().map(|h| h.scope.as_str()).collect();
        assert!(scopes.contains(&"global"));
        assert!(scopes.contains(&"workspace"));
        assert!(results
            .hits
            .iter()
            .all(|h| h.snippet.to_lowercase().contains("milk")));

        // Case-insensitive, trimmed.
        assert_eq!(search_notes(&dir, "  MILK  ", ws).unwrap().hits.len(), 2);
        // Name matches too.
        assert_eq!(search_notes(&dir, "shopping", ws).unwrap().hits.len(), 1);

        // Empty and overlong queries are rejected.
        assert!(search_notes(&dir, "   ", ws).is_err());
        assert!(search_notes(&dir, &"q".repeat(MAX_QUERY_CHARS + 1), ws).is_err());
        assert!(search_notes(&dir, "milk", Some("../x")).is_err());

        remove_dir(&dir);
    }

    #[test]
    fn titles_fall_back_to_first_line_then_name() {
        assert_eq!(title_for("# Hello\nbody\n", "n"), "Hello");
        assert_eq!(title_for("## Sub\nbody\n", "n"), "Sub");
        assert_eq!(title_for("\n\nplain line\n", "n"), "plain line");
        assert_eq!(title_for("\n\n", "fallback"), "fallback");
        assert_eq!(title_for("", "fallback"), "fallback");
    }

    #[test]
    fn non_markdown_and_tmp_files_are_ignored() {
        let dir = scratch_dir();
        remove_dir(&dir);
        write_note(&dir, "global", None, "real", "content\n").unwrap();
        let scope = dir.join("notes").join("global");
        fs::write(scope.join("stray.txt"), "x").unwrap();
        fs::write(scope.join(".real.tmp"), "x").unwrap();
        let listing = list_notes(&dir, "global", None).unwrap();
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "real");
        remove_dir(&dir);
    }
}

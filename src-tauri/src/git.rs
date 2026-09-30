//! Git operations for the Source Control panel.
//!
//! Shells out to the user's `git` CLI (argument arrays, never a shell) with
//! the workspace root as its working directory, so push/pull reuse the
//! user's config and credential helpers. Paths are validated to stay inside
//! the root, output is capped, and every call has a timeout so a hung
//! credential prompt can never wedge the UI.

use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Timeout for local operations (status, diff, stage, commit, ...).
const LOCAL_TIMEOUT: Duration = Duration::from_secs(15);
/// Timeout for network operations (push, pull).
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
/// Largest captured stdout per call; status/diff report truncation.
const MAX_OUTPUT: usize = 4 * 1024 * 1024;
/// Largest captured diff output.
const MAX_DIFF_OUTPUT: usize = 256 * 1024;
/// Largest stderr kept for error messages.
const MAX_ERROR: usize = 4096;
/// Spawn failure meaning git itself is missing (the root is checked first,
// so a missing binary is the only remaining NotFound cause).
const GIT_MISSING: &str = "Git is not installed or not on PATH.";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    pub path: String,
    pub status: String,
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: Vec<ChangeEntry>,
    pub unstaged: Vec<ChangeEntry>,
    pub untracked: Vec<String>,
    pub truncated: bool,
}

impl GitStatus {
    fn not_a_repo() -> Self {
        GitStatus {
            is_repo: false,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiff {
    pub diff: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranches {
    pub current: Option<String>,
    pub branches: Vec<String>,
}

struct GitOutput {
    stdout: String,
    truncated: bool,
}

fn run_git(root: &Path, args: &[&str], timeout: Duration) -> Result<GitOutput, String> {
    run_git_capped(root, args, timeout, MAX_OUTPUT)
}

fn run_git_capped(
    root: &Path,
    args: &[&str],
    timeout: Duration,
    cap: usize,
) -> Result<GitOutput, String> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GIT_MISSING.to_string()
            } else {
                format!("Cannot run git: {e}")
            }
        })?;
    // Reader threads first: polling try_wait without draining the pipes
    // would deadlock once a pipe buffer fills.
    let out_handle = std::thread::spawn({
        let out = child.stdout.take();
        move || read_capped(out, cap)
    });
    let err_handle = std::thread::spawn({
        let err = child.stderr.take();
        move || read_capped(err, MAX_ERROR)
    });
    let start = Instant::now();
    let status = loop {
        match child
            .try_wait()
            .map_err(|e| format!("Cannot run git: {e}"))?
        {
            Some(status) => break status,
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_handle.join();
                    let _ = err_handle.join();
                    return Err(format!(
                        "Git command timed out after {}s.",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };
    let (stdout, truncated) = out_handle.join().unwrap_or_default();
    let (stderr, _) = err_handle.join().unwrap_or_default();
    if !status.success() {
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            "Git command failed.".to_string()
        } else {
            format!("Git failed: {detail}")
        });
    }
    Ok(GitOutput { stdout, truncated })
}

fn read_capped(pipe: Option<impl Read>, cap: usize) -> (String, bool) {
    let mut buf = Vec::new();
    let mut truncated = false;
    if let Some(mut pipe) = pipe {
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    if buf.len() + n > cap {
                        buf.extend_from_slice(&chunk[..cap - buf.len()]);
                        truncated = true;
                        // Drain the rest so the child never blocks on us.
                        let _ = std::io::copy(&mut pipe, &mut std::io::sink());
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                Err(_) => break,
            }
        }
    }
    (String::from_utf8_lossy(&buf).into_owned(), truncated)
}

fn check_root(root: &str) -> Result<PathBuf, String> {
    if root.contains('\0') {
        return Err("Path must not contain NUL bytes.".to_string());
    }
    let path = PathBuf::from(root);
    if !path.is_absolute() {
        return Err("Workspace folder must be an absolute path.".to_string());
    }
    if !path.is_dir() {
        return Err("Workspace folder is unavailable.".to_string());
    }
    Ok(path)
}

/// Lexical containment for git path args (works for untracked/deleted files
/// that canonicalize cannot see): rejects absolute paths and any `..` that
/// climbs above the root. Returns forward slashes, which git accepts on
/// every platform.
fn check_rel_path(path: &str) -> Result<String, String> {
    if path.contains('\0') {
        return Err("Path must not contain NUL bytes.".to_string());
    }
    if path.is_empty() {
        return Err("Path must not be empty.".to_string());
    }
    // Normalize separators first so Windows-style input can't smuggle an
    // absolute path or `..` past the lexical walk on any platform.
    let normalized = path.replace('\\', "/");
    if Path::new(path).is_absolute() || normalized.starts_with('/') {
        return Err("Path must be relative to the workspace.".to_string());
    }
    let mut depth = 0i32;
    for component in normalized.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return Err("Path is outside the workspace.".to_string());
                }
            }
            _ => depth += 1,
        }
    }
    Ok(normalized)
}

fn check_branch(branch: &str) -> Result<&str, String> {
    if branch.contains('\0') {
        return Err("Branch must not contain NUL bytes.".to_string());
    }
    let name = branch.trim();
    if name.is_empty() {
        return Err("Branch must not be empty.".to_string());
    }
    if name.starts_with('-') {
        return Err("Invalid branch name.".to_string());
    }
    Ok(name)
}

fn check_message(message: &str) -> Result<&str, String> {
    if message.contains('\0') {
        return Err("Commit message must not contain NUL bytes.".to_string());
    }
    if message.trim().is_empty() {
        return Err("Commit message must not be empty.".to_string());
    }
    Ok(message)
}

/// Short user feedback: command output, or a fallback when git was silent.
fn summarize(stdout: &str, fallback: &str) -> String {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.chars().take(2000).collect()
    }
}

fn has_head(root: &Path) -> bool {
    run_git(
        root,
        &["rev-parse", "--verify", "--quiet", "HEAD"],
        LOCAL_TIMEOUT,
    )
    .is_ok()
}

fn parse_branch_header(header: &str) -> (Option<String>, Option<String>, u32, u32) {
    // Examples: "main...origin/main [ahead 2, behind 1]", "main",
    // "No commits yet on main", "HEAD (no branch)".
    let mut rest = header;
    let mut ahead = 0u32;
    let mut behind = 0u32;
    if let Some(bracket) = rest.find(" [") {
        let counts = &rest[bracket + 2..];
        for part in counts.trim_end_matches(']').split(", ") {
            if let Some(n) = part.strip_prefix("ahead ") {
                ahead = n.parse().unwrap_or(0);
            } else if let Some(n) = part.strip_prefix("behind ") {
                behind = n.parse().unwrap_or(0);
            }
        }
        rest = &rest[..bracket];
    }
    if rest == "HEAD (no branch)" {
        return (None, None, ahead, behind);
    }
    if let Some(name) = rest.strip_prefix("No commits yet on ") {
        return match name.split_once("...") {
            Some((branch, upstream)) => (
                Some(branch.to_string()),
                Some(upstream.to_string()),
                ahead,
                behind,
            ),
            None => (Some(name.to_string()), None, ahead, behind),
        };
    }
    match rest.split_once("...") {
        Some((branch, upstream)) => (
            Some(branch.to_string()),
            Some(upstream.to_string()),
            ahead,
            behind,
        ),
        None => (Some(rest.to_string()), None, ahead, behind),
    }
}

fn parse_status(output: &str, truncated: bool) -> Result<GitStatus, String> {
    let mut status = GitStatus {
        is_repo: true,
        truncated,
        ..Default::default()
    };
    let mut records: Vec<&str> = output.split('\0').collect();
    if truncated {
        // A capped read can end mid-record; drop the partial tail.
        records.pop();
    }
    let mut records = records.into_iter();
    match records.next() {
        Some(header) => match header.strip_prefix("## ") {
            Some(rest) => {
                let (branch, upstream, ahead, behind) = parse_branch_header(rest);
                status.branch = branch;
                status.upstream = upstream;
                status.ahead = ahead;
                status.behind = behind;
            }
            None if header.is_empty() => {}
            None => return Err("Cannot parse git status.".to_string()),
        },
        None => return Err("Cannot parse git status.".to_string()),
    }
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        // "XY path": the two status bytes and separator are ASCII, so
        // byte indexing is safe up to the path start.
        if record.len() < 4 || record.as_bytes()[2] != b' ' {
            return Err("Cannot parse git status.".to_string());
        }
        let x = record.as_bytes()[0] as char;
        let y = record.as_bytes()[1] as char;
        let path = record[3..].to_string();
        // Renames/copies carry the old path as an extra NUL field.
        let old_path = if x == 'R' || x == 'C' {
            match records.next() {
                Some(old) if !old.is_empty() => Some(old.to_string()),
                _ => return Err("Cannot parse git status.".to_string()),
            }
        } else {
            None
        };
        if x == '?' && y == '?' {
            status.untracked.push(path);
        } else {
            if x != ' ' && x != '?' {
                status.staged.push(ChangeEntry {
                    path: path.clone(),
                    status: x.to_string(),
                    old_path: old_path.clone(),
                });
            }
            if y != ' ' && y != '?' {
                status.unstaged.push(ChangeEntry {
                    path,
                    status: y.to_string(),
                    old_path,
                });
            }
        }
    }
    Ok(status)
}

pub fn status(root: &str) -> Result<GitStatus, String> {
    let root = check_root(root)?;
    match run_git(
        &root,
        &["rev-parse", "--is-inside-work-tree"],
        LOCAL_TIMEOUT,
    ) {
        Err(e) if e == GIT_MISSING => return Err(e),
        Err(_) => return Ok(GitStatus::not_a_repo()),
        Ok(out) if out.stdout.trim() != "true" => return Ok(GitStatus::not_a_repo()),
        Ok(_) => {}
    }
    let out = run_git(
        &root,
        &["status", "--porcelain=v1", "-b", "-z"],
        LOCAL_TIMEOUT,
    )?;
    parse_status(&out.stdout, out.truncated)
}

pub fn diff_file(root: &str, path: &str, staged: bool) -> Result<GitDiff, String> {
    let root = check_root(root)?;
    let rel = check_rel_path(path)?;
    let rel = rel.as_str();
    let args: &[&str] = if staged {
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--unified=3",
            "--cached",
            "--",
            rel,
        ]
    } else {
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--unified=3",
            "--",
            rel,
        ]
    };
    let out = run_git_capped(&root, args, LOCAL_TIMEOUT, MAX_DIFF_OUTPUT)?;
    Ok(GitDiff {
        diff: out.stdout,
        truncated: out.truncated,
    })
}

pub fn stage(root: &str, paths: &[String]) -> Result<String, String> {
    let root = check_root(root)?;
    if paths.is_empty() {
        let out = run_git(&root, &["add", "-A"], LOCAL_TIMEOUT)?;
        return Ok(summarize(&out.stdout, "Staged all changes."));
    }
    let mut args: Vec<&str> = vec!["add", "--"];
    let rels: Vec<String> = paths
        .iter()
        .map(|p| check_rel_path(p))
        .collect::<Result<_, _>>()?;
    args.extend(rels.iter().map(|s| s.as_str()));
    let out = run_git(&root, &args, LOCAL_TIMEOUT)?;
    Ok(summarize(&out.stdout, "Staged."))
}

pub fn unstage(root: &str, paths: &[String]) -> Result<String, String> {
    let root = check_root(root)?;
    // `reset` needs HEAD; before the first commit the index holds only
    // additions, where `rm --cached` is the correct unstage.
    if paths.is_empty() {
        let args: &[&str] = if has_head(&root) {
            &["reset", "-q"]
        } else {
            &["rm", "-r", "--cached", "--quiet", "--", "."]
        };
        let out = run_git(&root, args, LOCAL_TIMEOUT)?;
        return Ok(summarize(&out.stdout, "Unstaged all changes."));
    }
    let rels: Vec<String> = paths
        .iter()
        .map(|p| check_rel_path(p))
        .collect::<Result<_, _>>()?;
    let mut args: Vec<&str> = if has_head(&root) {
        vec!["reset", "-q", "--"]
    } else {
        vec!["rm", "--cached", "--quiet", "--"]
    };
    args.extend(rels.iter().map(|s| s.as_str()));
    let out = run_git(&root, &args, LOCAL_TIMEOUT)?;
    Ok(summarize(&out.stdout, "Unstaged."))
}

pub fn commit(root: &str, message: &str) -> Result<String, String> {
    let root = check_root(root)?;
    let message = check_message(message)?;
    let out = run_git(&root, &["commit", "-m", message], LOCAL_TIMEOUT)?;
    Ok(summarize(&out.stdout, "Committed."))
}

pub fn push(root: &str) -> Result<String, String> {
    let root = check_root(root)?;
    let out = run_git(&root, &["push"], NETWORK_TIMEOUT)?;
    Ok(summarize(&out.stdout, "Pushed."))
}

pub fn pull(root: &str) -> Result<String, String> {
    let root = check_root(root)?;
    // Fast-forward only: a button must never create a surprise merge commit.
    let out = run_git(&root, &["pull", "--ff-only"], NETWORK_TIMEOUT)?;
    Ok(summarize(&out.stdout, "Pulled."))
}

pub fn branches(root: &str) -> Result<GitBranches, String> {
    let root = check_root(root)?;
    let out = run_git(
        &root,
        &["branch", "--no-color", "--format=%(refname:short)"],
        LOCAL_TIMEOUT,
    )?;
    let current = run_git(&root, &["branch", "--show-current"], LOCAL_TIMEOUT)?
        .stdout
        .lines()
        .next()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());
    let branches = out
        .stdout
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(GitBranches { current, branches })
}

pub fn switch(root: &str, branch: &str) -> Result<String, String> {
    let root = check_root(root)?;
    let branch = check_branch(branch)?;
    let out = run_git(&root, &["switch", branch], LOCAL_TIMEOUT)?;
    Ok(summarize(&out.stdout, &format!("Switched to '{branch}'.")))
}

pub fn init(root: &str) -> Result<String, String> {
    let root = check_root(root)?;
    let out = run_git(&root, &["init"], LOCAL_TIMEOUT)?;
    Ok(summarize(&out.stdout, "Initialized git repository."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn test_dir(id: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ubra-git-test-{}-{id}", std::process::id()))
    }

    fn remove_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    /// Deterministic git for fixtures: fixed identity, no signing, no
    /// autocrlf rewrites, no user/system config.
    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-c")
            .arg("user.name=test")
            .arg("-c")
            .arg("user.email=test@example.com")
            .arg("-c")
            .arg("commit.gpgsign=false")
            .arg("-c")
            .arg("core.autocrlf=false")
            .args(args)
            .current_dir(dir)
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn root_str(dir: &Path) -> String {
        dir.to_string_lossy().into_owned()
    }

    #[test]
    fn parses_branch_headers() {
        assert_eq!(
            parse_branch_header("main...origin/main [ahead 2, behind 1]"),
            (
                Some("main".to_string()),
                Some("origin/main".to_string()),
                2,
                1
            )
        );
        assert_eq!(
            parse_branch_header("main...origin/main [ahead 1]"),
            (
                Some("main".to_string()),
                Some("origin/main".to_string()),
                1,
                0
            )
        );
        assert_eq!(
            parse_branch_header("main"),
            (Some("main".to_string()), None, 0, 0)
        );
        assert_eq!(
            parse_branch_header("No commits yet on main"),
            (Some("main".to_string()), None, 0, 0)
        );
        assert_eq!(parse_branch_header("HEAD (no branch)"), (None, None, 0, 0));
    }

    #[test]
    fn parses_porcelain_status_with_rename() {
        let output = "## main...origin/main [ahead 1]\0M  kept.txt\0MM both.txt\0R  new.txt\0old.txt\0?? fresh.txt\0";
        let status = parse_status(output, false).unwrap();
        assert!(status.is_repo);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!(status.ahead, 1);
        assert_eq!(status.behind, 0);
        assert!(!status.truncated);
        assert_eq!(status.staged.len(), 3);
        assert_eq!(status.staged[0].path, "kept.txt");
        assert_eq!(status.staged[0].status, "M");
        assert_eq!(status.staged[2].path, "new.txt");
        assert_eq!(status.staged[2].status, "R");
        assert_eq!(status.staged[2].old_path.as_deref(), Some("old.txt"));
        assert_eq!(status.unstaged.len(), 1);
        assert_eq!(status.unstaged[0].path, "both.txt");
        assert_eq!(status.unstaged[0].status, "M");
        assert_eq!(status.untracked, vec!["fresh.txt".to_string()]);
    }

    #[test]
    fn rejects_malformed_porcelain_records() {
        assert!(parse_status("## main\0broken", false).is_err());
        assert!(parse_status("## main\0R  new.txt\0", false).is_err());
        assert!(parse_status("no-header\0", false).is_err());
    }

    #[test]
    fn validates_paths_branches_and_messages() {
        assert_eq!(check_rel_path("a/b.txt").unwrap(), "a/b.txt");
        assert_eq!(check_rel_path("dir/").unwrap(), "dir/");
        assert_eq!(check_rel_path("./x").unwrap(), "./x");
        assert_eq!(check_rel_path("sub/..").unwrap(), "sub/..");
        assert_eq!(check_rel_path("a\\b").unwrap(), "a/b");
        assert!(check_rel_path("").unwrap_err().contains("empty"));
        assert!(check_rel_path("/abs").unwrap_err().contains("relative"));
        assert!(check_rel_path("..").unwrap_err().contains("outside"));
        assert!(check_rel_path("a/../../x").unwrap_err().contains("outside"));
        assert!(check_rel_path("a\0b").unwrap_err().contains("NUL"));

        assert_eq!(check_branch("feature/x").unwrap(), "feature/x");
        assert!(check_branch("").unwrap_err().contains("empty"));
        assert!(check_branch("-h").unwrap_err().contains("Invalid"));
        assert!(check_branch("a\0b").unwrap_err().contains("NUL"));

        assert_eq!(check_message("hello").unwrap(), "hello");
        assert!(check_message("  ").unwrap_err().contains("empty"));
        assert!(check_message("a\0b").unwrap_err().contains("NUL"));

        assert!(check_root("").unwrap_err().contains("absolute"));
        assert!(check_root("relative/path")
            .unwrap_err()
            .contains("absolute"));
    }

    #[test]
    fn reports_non_repositories_without_error() {
        let dir = test_dir("nonrepo");
        remove_dir(&dir);
        fs::create_dir_all(&dir).unwrap();
        let status = status(&root_str(&dir)).unwrap();
        assert!(!status.is_repo);
        assert!(status.branch.is_none());
        remove_dir(&dir);
    }

    #[test]
    fn rejects_paths_that_escape_before_spawning() {
        let dir = test_dir("escape");
        remove_dir(&dir);
        fs::create_dir_all(&dir).unwrap();
        let root = root_str(&dir);
        assert!(stage(&root, &["../x".to_string()])
            .unwrap_err()
            .contains("outside"));
        assert!(diff_file(&root, "/abs", false)
            .unwrap_err()
            .contains("relative"));
        remove_dir(&dir);
    }

    #[test]
    fn init_stage_unstage_and_commit_round_trip() {
        let dir = test_dir("roundtrip");
        remove_dir(&dir);
        fs::create_dir_all(&dir).unwrap();
        let root = root_str(&dir);

        init(&root).unwrap();
        let fresh = status(&root).unwrap();
        assert!(fresh.is_repo);
        assert!(fresh.branch.is_some());

        fs::write(dir.join("a.txt"), "one\n").unwrap();
        let untracked = status(&root).unwrap();
        assert_eq!(untracked.untracked, vec!["a.txt".to_string()]);

        stage(&root, &["a.txt".to_string()]).unwrap();
        let staged = status(&root).unwrap();
        assert_eq!(staged.staged.len(), 1);
        assert_eq!(staged.staged[0].status, "A");
        assert!(staged.untracked.is_empty());

        // Unstage works before the first commit too.
        unstage(&root, &["a.txt".to_string()]).unwrap();
        let back = status(&root).unwrap();
        assert!(back.staged.is_empty());
        assert_eq!(back.untracked, vec!["a.txt".to_string()]);

        stage(&root, &["a.txt".to_string()]).unwrap();
        let summary = commit(&root, "first").unwrap();
        assert!(summary.contains("first"), "unexpected summary: {summary}");
        let clean = status(&root).unwrap();
        assert!(clean.staged.is_empty());
        assert!(clean.unstaged.is_empty());
        assert!(clean.untracked.is_empty());

        fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        let modified = status(&root).unwrap();
        assert_eq!(modified.unstaged.len(), 1);
        assert_eq!(modified.unstaged[0].status, "M");

        let diff = diff_file(&root, "a.txt", false).unwrap();
        assert!(!diff.truncated);
        assert!(diff.diff.contains("+two"), "unexpected diff: {}", diff.diff);

        stage(&root, &[]).unwrap();
        let restaged = status(&root).unwrap();
        assert_eq!(restaged.staged.len(), 1);
        let cached = diff_file(&root, "a.txt", true).unwrap();
        assert!(cached.diff.contains("+two"));

        unstage(&root, &[]).unwrap();
        let final_status = status(&root).unwrap();
        assert!(final_status.staged.is_empty());
        assert_eq!(final_status.unstaged.len(), 1);

        remove_dir(&dir);
    }

    #[test]
    fn lists_and_switches_branches() {
        let dir = test_dir("branches");
        remove_dir(&dir);
        fs::create_dir_all(&dir).unwrap();
        let root = root_str(&dir);
        git(&dir, &["init"]);
        fs::write(dir.join("a.txt"), "one\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-m", "first"]);

        let before = branches(&root).unwrap();
        assert_eq!(before.branches.len(), 1);
        assert_eq!(before.current, Some(before.branches[0].clone()));

        git(&dir, &["switch", "-c", "feature"]);
        let on_feature = branches(&root).unwrap();
        assert_eq!(on_feature.current.as_deref(), Some("feature"));

        switch(&root, &before.branches[0]).unwrap();
        let back = branches(&root).unwrap();
        assert_eq!(back.current, before.current);
        assert!(switch(&root, "-h").is_err());

        remove_dir(&dir);
    }

    #[test]
    fn tracks_ahead_of_a_local_remote() {
        let dir = test_dir("ahead");
        let bare = test_dir("ahead-bare");
        remove_dir(&dir);
        remove_dir(&bare);
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&bare).unwrap();
        let root = root_str(&dir);
        git(&bare, &["init", "--bare"]);
        git(&dir, &["init"]);
        fs::write(dir.join("a.txt"), "one\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-m", "first"]);
        git(&dir, &["remote", "add", "origin", bare.to_str().unwrap()]);

        push(&root).unwrap_err();
        // The first push has no upstream; publish it, then push cleanly.
        git(&dir, &["push", "-u", "origin", "HEAD"]);
        push(&root).unwrap();

        fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-m", "second"]);
        let ahead = status(&root).unwrap();
        assert_eq!(ahead.ahead, 1);
        assert_eq!(ahead.behind, 0);
        assert!(ahead.upstream.is_some());

        remove_dir(&dir);
        remove_dir(&bare);
    }
}

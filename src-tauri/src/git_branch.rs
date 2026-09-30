//! Best-effort git branch lookup for workspace subtitles.
//! Shells out to the system git CLI so user config and worktrees just work.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static WARNED_UNAVAILABLE: AtomicBool = AtomicBool::new(false);

/// Current branch for `path`, or the short SHA on a detached HEAD.
/// Returns None when the path is not a git checkout (or git is unavailable).
pub fn branch_for(path: &Path) -> Option<String> {
    // TEMP perf instrumentation (removed after the lag audit).
    let timed = matches!(std::env::var("UBRA_PERF_LOG").as_deref(), Ok("1")).then(Instant::now);
    let result = resolve_branch(path);
    if let Some(start) = timed {
        eprintln!(
            "ubra-perf: git_branch {}ms {}",
            start.elapsed().as_millis(),
            path.display()
        );
    }
    result
}

fn resolve_branch(path: &Path) -> Option<String> {
    match run_git(path, &["branch", "--show-current"]) {
        Some(branch) if !branch.is_empty() => Some(branch),
        _ => {
            let sha = run_git(path, &["rev-parse", "--short", "HEAD"])?;
            if sha.is_empty() {
                None
            } else {
                Some(sha)
            }
        }
    }
}

fn run_git(path: &Path, args: &[&str]) -> Option<String> {
    let output = match Command::new("git").arg("-C").arg(path).args(args).output() {
        Ok(output) => output,
        Err(e) => {
            if !WARNED_UNAVAILABLE.swap(true, Ordering::Relaxed) {
                eprintln!("ubra: git unavailable: {e}");
            }
            return None;
        }
    };
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

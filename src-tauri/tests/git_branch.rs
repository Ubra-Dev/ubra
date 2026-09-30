//! Real git CLI against temp checkouts for workspace branch subtitles.
use std::path::{Path, PathBuf};
use std::process::Command;
use ubra_lib::git_branch::branch_for;

fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.email=ubra-test@example.com",
            "-c",
            "user.name=ubra-test",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git CLI must be installed for git_branch tests")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ubra-git-branch-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn commit_one(dir: &Path) {
    assert!(git(dir, &["init"]).status.success());
    std::fs::write(dir.join("file.txt"), "hello").unwrap();
    assert!(git(dir, &["add", "."]).status.success());
    assert!(git(dir, &["commit", "-m", "init"]).status.success());
}

#[test]
fn reports_current_branch() {
    let dir = fixture("branch");
    commit_one(&dir);
    assert_eq!(branch_for(&dir), Some("main".to_string()));
    assert!(git(&dir, &["checkout", "-b", "feature/login"])
        .status
        .success());
    assert_eq!(branch_for(&dir), Some("feature/login".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detached_head_reports_short_sha() {
    let dir = fixture("detached");
    commit_one(&dir);
    assert!(git(&dir, &["checkout", "--detach", "HEAD"])
        .status
        .success());
    let sha = String::from_utf8_lossy(&git(&dir, &["rev-parse", "--short", "HEAD"]).stdout)
        .trim()
        .to_string();
    assert!(!sha.is_empty());
    assert_eq!(branch_for(&dir), Some(sha));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn non_repo_and_missing_dirs_report_none() {
    let dir = fixture("plain");
    assert_eq!(branch_for(&dir), None);
    assert_eq!(branch_for(&dir.join("does-not-exist")), None);
    let _ = std::fs::remove_dir_all(&dir);
}

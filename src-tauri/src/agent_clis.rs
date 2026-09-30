//! Installed agent-CLI detection for onboarding.
//!
//! Scans PATH for the known agent binaries in [`AGENT_TABLE`], then asks the
//! user's login shell about the misses: GUI apps inherit a minimal PATH that
//! omits homebrew/mise/npm install dirs, while the interactive shells we type
//! commands into see the full user PATH.

use crate::agent_watch::AGENT_TABLE;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::sync::mpsc;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DetectedCli {
    pub cli: String,
    pub label: String,
    pub path: String,
}

/// How long to wait for the login shell before falling back to PATH-only.
#[cfg(unix)]
const SHELL_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Ordered scan stems: [`AGENT_TABLE`] families plus official adapter
/// aliases (e.g. `kiro-cli`) that users may have installed instead.
fn scan_stems() -> Vec<&'static str> {
    let mut stems: Vec<&'static str> = AGENT_TABLE.iter().map(|(stem, _)| *stem).collect();
    for adapter in crate::agent_adapters::ADAPTERS {
        for exe in adapter.executables {
            if !stems.contains(exe) {
                stems.push(exe);
            }
        }
    }
    stems
}

fn label_for(stem: &str) -> &'static str {
    if let Some(label) = AGENT_TABLE
        .iter()
        .find(|(name, _)| *name == stem)
        .map(|(_, label)| *label)
    {
        return label;
    }
    crate::agent_adapters::find_by_executable(stem)
        .map(|adapter| adapter.label)
        .unwrap_or("Agent CLI")
}

/// Installed agent CLIs in [`AGENT_TABLE`] order (aliases appended). Never
/// fails; an empty result means nothing was found (or the scan itself
/// failed). `cli` is the actual installed binary name.
pub fn detect() -> Vec<DetectedCli> {
    let stems: Vec<&str> = scan_stems();
    let found = scan_path_env(&stems);
    #[cfg(unix)]
    let found = {
        let mut found = found;
        let missing: Vec<&str> = stems
            .iter()
            .copied()
            .filter(|stem| !found.contains_key(*stem))
            .collect();
        if !missing.is_empty() {
            for (stem, path) in probe_login_shell(&missing) {
                found.entry(stem).or_insert(path);
            }
        }
        found
    };
    stems
        .iter()
        .filter_map(|stem| {
            found.get(*stem).map(|path| DetectedCli {
                cli: (*stem).to_string(),
                label: label_for(stem).to_string(),
                path: path.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

fn scan_path_env(stems: &[&str]) -> BTreeMap<String, PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    scan_path(stems, &path, &exe_extensions())
}

/// Executable suffixes to try per stem: PATHEXT on Windows, none elsewhere.
fn exe_extensions() -> Vec<OsString> {
    #[cfg(windows)]
    {
        std::env::var_os("PATHEXT")
            .map(|raw| {
                raw.to_string_lossy()
                    .split(';')
                    .filter_map(|ext| {
                        let ext = ext.strip_prefix('.').unwrap_or(ext);
                        if ext.is_empty() {
                            None
                        } else {
                            Some(OsString::from(ext))
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// First executable match per stem, in PATH order.
fn scan_path(
    stems: &[&str],
    path_var: &OsStr,
    extensions: &[OsString],
) -> BTreeMap<String, PathBuf> {
    let mut found = BTreeMap::new();
    for dir in std::env::split_paths(path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for stem in stems {
            if found.contains_key(*stem) {
                continue;
            }
            let mut candidates = vec![dir.join(stem)];
            for ext in extensions {
                candidates.push(dir.join(format!("{stem}.{}", ext.to_string_lossy())));
            }
            if let Some(hit) = candidates.into_iter().find(|path| is_executable(path)) {
                found.insert((*stem).to_string(), hit);
            }
        }
        if found.len() == stems.len() {
            break;
        }
    }
    found
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Resolve misses through one login-shell invocation. Slow or broken shell
/// configs must not block onboarding, so the probe runs on a thread with a
/// timeout; on expiry the PATH-only results stand.
#[cfg(unix)]
fn probe_login_shell(missing: &[&str]) -> BTreeMap<String, PathBuf> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string());
    let owned: Vec<String> = missing.iter().map(|stem| (*stem).to_string()).collect();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let stems: Vec<&str> = owned.iter().map(String::as_str).collect();
        let _ = tx.send(shell_probe(&shell, true, None, &stems));
    });
    rx.recv_timeout(SHELL_PROBE_TIMEOUT).unwrap_or_default()
}

/// Run `command -v` for every stem in one shell invocation. `path_override`
/// replaces PATH for the child (used by tests for a hermetic probe).
#[cfg(unix)]
fn shell_probe(
    shell: &str,
    login: bool,
    path_override: Option<&OsStr>,
    stems: &[&str],
) -> BTreeMap<String, PathBuf> {
    let script = format!(
        "for c in {}; do printf '%s=%s\\n' \"$c\" \"$(command -v \"$c\")\"; done",
        stems.join(" ")
    );
    // Resolve before overriding PATH: exec lookup uses the child's PATH.
    let mut cmd = Command::new(resolve_program(shell));
    if login {
        cmd.arg("-l");
    }
    cmd.arg("-c").arg(&script);
    if let Some(path) = path_override {
        cmd.env("PATH", path);
    }
    match cmd.output() {
        Ok(output) => parse_probe_output(&output.stdout, stems),
        Err(_) => BTreeMap::new(),
    }
}

/// Resolve a program against the process PATH. Falls back to the input
/// when it already contains a path separator or is not found.
#[cfg(unix)]
fn resolve_program(program: &str) -> PathBuf {
    if program.contains('/') {
        return PathBuf::from(program);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    scan_path(&[program], &path, &exe_extensions())
        .remove(program)
        .unwrap_or_else(|| PathBuf::from(program))
}

/// Parse `stem=path` lines; only existing files count, which also discards
/// alias/function output from interactive shells.
#[cfg(unix)]
fn parse_probe_output(stdout: &[u8], stems: &[&str]) -> BTreeMap<String, PathBuf> {
    let mut found = BTreeMap::new();
    for line in String::from_utf8_lossy(stdout).lines() {
        let Some((stem, path)) = line.split_once('=') else {
            continue;
        };
        if !stems.contains(&stem) || path.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            found.insert(stem.to_string(), candidate);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn temp_bin(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ubra-agent-clis-{name}-{}-{}",
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

    fn write_bin(dir: &Path, name: &str, executable: bool) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\necho hi\n").unwrap();
        #[cfg(unix)]
        if executable {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = executable;
        path
    }

    fn join_path(dirs: &[PathBuf]) -> OsString {
        std::env::join_paths(dirs.iter().map(|dir| dir.as_os_str())).unwrap()
    }

    #[test]
    fn scan_finds_executables_in_path_order() {
        let first = temp_bin("order-first");
        let second = temp_bin("order-second");
        let wanted = first.join("codex");
        write_bin(&first, "codex", true);
        write_bin(&second, "codex", true);
        let found = scan_path(&["codex"], &join_path(&[first, second.clone()]), &[]);
        assert_eq!(found.get("codex"), Some(&wanted));
        let _ = std::fs::remove_dir_all(&second);
    }

    #[test]
    fn scan_skips_missing_dirs_and_reports_each_stem_once() {
        let bin = temp_bin("multi");
        write_bin(&bin, "codex", true);
        let missing = bin.join("does-not-exist");
        let path = join_path(&[missing, bin.clone()]);
        let found = scan_path(&["codex", "nope"], &path, &[]);
        assert_eq!(found.len(), 1);
        assert!(found.contains_key("codex"));
        let _ = std::fs::remove_dir_all(&bin);
    }

    #[test]
    fn scan_tries_configured_extensions() {
        let bin = temp_bin("ext");
        let wanted = write_bin(&bin, "codex.sh", true);
        let found = scan_path(
            &["codex"],
            &join_path(std::slice::from_ref(&bin)),
            &[OsString::from("sh")],
        );
        assert_eq!(found.get("codex"), Some(&wanted));
        let _ = std::fs::remove_dir_all(&bin);
    }

    #[test]
    #[cfg(unix)]
    fn scan_skips_non_executable_files() {
        let bin = temp_bin("noexec");
        write_bin(&bin, "codex", false);
        let found = scan_path(&["codex"], &join_path(std::slice::from_ref(&bin)), &[]);
        assert!(found.is_empty());
        let _ = std::fs::remove_dir_all(&bin);
    }

    #[test]
    #[cfg(unix)]
    fn parse_keeps_existing_files_and_ignores_noise() {
        let bin = temp_bin("parse");
        let target = write_bin(&bin, "codex", true);
        let stdout = format!(
            "codex={}\nnope=\ngarbage line\nclaude=alias claude='x'\n",
            target.display()
        );
        let found = parse_probe_output(stdout.as_bytes(), &["codex", "nope", "claude"]);
        assert_eq!(found.len(), 1);
        assert_eq!(found.get("codex"), Some(&target));
        let _ = std::fs::remove_dir_all(&bin);
    }

    #[test]
    #[cfg(unix)]
    fn resolve_program_prefers_absolute_paths() {
        assert_eq!(resolve_program("/bin/sh"), PathBuf::from("/bin/sh"));
        assert!(resolve_program("sh").is_absolute());
    }

    #[test]
    #[cfg(unix)]
    fn shell_probe_resolves_with_controlled_path() {
        let bin = temp_bin("shell");
        let wanted = write_bin(&bin, "codex", true);
        let found = shell_probe(
            "sh",
            false,
            Some(bin.as_os_str()),
            &["codex", "definitely-not-a-cli"],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found.get("codex"), Some(&wanted));
        let _ = std::fs::remove_dir_all(&bin);
    }
}

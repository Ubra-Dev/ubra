//! PTY session management (Phase 1: local panes over `portable-pty`).
//!
//! [`PtyManager`] owns every live pane: spawn/write/resize/kill plus a reader
//! thread per pane that forwards output through a [`PtyEventSink`]. The sink
//! abstraction keeps the manager testable without a Tauri runtime.

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Opaque pane identifier handed to the frontend.
pub type PaneId = u32;

/// Output chunk forwarded to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyOutput {
    pub id: PaneId,
    pub data: String,
}

/// Pane exit notification forwarded to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyExit {
    pub id: PaneId,
    pub success: bool,
    pub code: Option<i32>,
}

/// Receives pane events. Implemented by the Tauri event bridge in production
/// and by an in-memory channel in tests.
pub trait PtyEventSink: Send + Sync + 'static {
    fn output(&self, id: PaneId, data: String);
    fn exited(&self, id: PaneId, success: bool, code: Option<i32>);
}

struct Session {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
    /// PID of the process spawned directly in the PTY (0 when unknown).
    root_pid: u32,
}

pub struct PtyManager {
    sink: Arc<dyn PtyEventSink>,
    sessions: Arc<Mutex<HashMap<PaneId, Arc<Session>>>>,
    next_id: AtomicU32,
}

impl PtyManager {
    pub fn new(sink: Arc<dyn PtyEventSink>) -> Self {
        Self {
            sink,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU32::new(1),
        }
    }

    /// Spawn a pane running `shell` (or the platform default) with `args`.
    pub fn spawn(
        &self,
        shell: Option<String>,
        cwd: Option<String>,
        args: Vec<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<PaneId> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(shell.unwrap_or_else(default_shell));
        for arg in &args {
            cmd.arg(arg);
        }
        cmd.cwd(cwd.or_else(default_cwd).unwrap_or_else(|| ".".to_string()));

        let child = pair.slave.spawn_command(cmd)?;
        let root_pid = child.process_id().unwrap_or(0);
        drop(pair.slave);
        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let session = Arc::new(Session {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            root_pid,
        });
        self.sessions
            .lock()
            .unwrap()
            .insert(id, Arc::clone(&session));

        let sink = Arc::clone(&self.sink);
        let sessions = Arc::clone(&self.sessions);
        let thread_session = Arc::clone(&session);
        if let Err(e) = thread::Builder::new()
            .name(format!("ubra-pty-{id}"))
            .spawn(move || reader_loop(id, reader, &thread_session, &sessions, &sink))
        {
            // Never leave a reader-less session behind: drop it and kill the child.
            self.sessions.lock().unwrap().remove(&id);
            let _ = session.child.lock().unwrap().kill();
            return Err(e.into());
        }

        Ok(id)
    }

    fn session(&self, id: PaneId) -> anyhow::Result<Arc<Session>> {
        self.sessions
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no such pane: {id}"))
    }

    /// Write frontend input to the pane.
    pub fn write(&self, id: PaneId, data: &str) -> anyhow::Result<()> {
        let session = self.session(id)?;
        session.writer.lock().unwrap().write_all(data.as_bytes())?;
        Ok(())
    }

    /// Resize the pane's PTY.
    pub fn resize(&self, id: PaneId, cols: u16, rows: u16) -> anyhow::Result<()> {
        let session = self.session(id)?;
        session.master.lock().unwrap().resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    /// Snapshot of live panes and their root PIDs for the agent watcher.
    pub fn pane_roots(&self) -> Vec<crate::agent_watch::PaneRoots> {
        self.sessions
            .lock()
            .unwrap()
            .iter()
            .map(|(id, s)| crate::agent_watch::PaneRoots {
                id: *id,
                root_pid: s.root_pid,
            })
            .collect()
    }

    /// Kill the pane's process. The reader thread observes EOF and emits the
    /// exit event; the session is already removed here so late writes fail fast.
    pub fn kill(&self, id: PaneId) -> anyhow::Result<()> {
        let session = self
            .sessions
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or_else(|| anyhow::anyhow!("no such pane: {id}"))?;
        session.child.lock().unwrap().kill()?;
        Ok(())
    }
}

fn reader_loop(
    id: PaneId,
    mut reader: Box<dyn Read + Send>,
    session: &Session,
    sessions: &Arc<Mutex<HashMap<PaneId, Arc<Session>>>>,
    sink: &Arc<dyn PtyEventSink>,
) {
    let mut decoder = Utf8Splitter::new();
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let text = decoder.push(&buf[..n]);
                if !text.is_empty() {
                    sink.output(id, text);
                }
            }
            // The PTY reader performs a raw read without EINTR retry, so a
            // signal-interrupted read must resume, not end the pane.
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break, // EIO on Unix after child exit; treat as EOF.
        }
    }
    let tail = decoder.flush();
    if !tail.is_empty() {
        sink.output(id, tail);
    }

    // Reap the exit status without blocking forever.
    let mut status = None;
    if let Ok(mut child) = session.child.lock() {
        for _ in 0..20 {
            match child.try_wait() {
                Ok(Some(s)) => {
                    status = Some(s);
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(_) => break,
            }
        }
    }
    sessions.lock().unwrap().remove(&id);
    let (success, code) = match status {
        Some(s) => (s.success(), Some(s.exit_code() as i32)),
        None => (false, None),
    };
    sink.exited(id, success, code);
}

fn default_shell() -> String {
    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.is_empty() {
            return shell;
        }
    }
    #[cfg(windows)]
    return "powershell.exe".to_string();
    #[cfg(target_os = "macos")]
    return "/bin/zsh".to_string();
    #[cfg(not(any(windows, target_os = "macos")))]
    return "/bin/sh".to_string();
}

fn default_cwd() -> Option<String> {
    #[cfg(windows)]
    return std::env::var("USERPROFILE").ok();
    #[cfg(not(windows))]
    return std::env::var("HOME").ok();
}

/// Incrementally decodes PTY byte chunks as UTF-8, holding an incomplete
/// trailing sequence for the next chunk instead of corrupting it.
struct Utf8Splitter {
    pending: Vec<u8>,
}

impl Utf8Splitter {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(s) => {
                    out.push_str(s);
                    self.pending.clear();
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    out.push_str(&String::from_utf8_lossy(&self.pending[..valid]));
                    match e.error_len() {
                        Some(len) => {
                            out.push('\u{FFFD}');
                            self.pending.drain(..valid + len);
                        }
                        None => {
                            // Incomplete sequence at the end; hold it.
                            self.pending.drain(..valid);
                            break;
                        }
                    }
                }
            }
        }
        out
    }

    fn flush(&mut self) -> String {
        String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::Utf8Splitter;

    #[test]
    fn ascii_passthrough() {
        let mut d = Utf8Splitter::new();
        assert_eq!(d.push(b"hello\r\n"), "hello\r\n");
        assert_eq!(d.flush(), "");
    }

    #[test]
    fn split_multibyte_char_across_chunks() {
        let mut d = Utf8Splitter::new();
        // "é" is 0xC3 0xA9; feed one byte at a time.
        assert_eq!(d.push(b"caf\xC3"), "caf");
        assert_eq!(d.push(b"\xA9!"), "é!");
        assert_eq!(d.flush(), "");
    }

    #[test]
    fn invalid_byte_becomes_replacement_char() {
        let mut d = Utf8Splitter::new();
        assert_eq!(d.push(b"a\xFFb"), "a\u{FFFD}b");
    }

    #[test]
    fn flush_replaces_truncated_tail() {
        let mut d = Utf8Splitter::new();
        assert_eq!(d.push(b"ab\xE2\x82"), "ab");
        assert_eq!(d.flush(), "\u{FFFD}");
    }
}

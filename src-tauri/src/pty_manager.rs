//! PTY session management (Phase 1: local panes over `portable-pty`).
//!
//! [`PtyManager`] owns every live pane: spawn/write/resize/kill plus a reader
//! thread per pane that forwards output through a [`PtyEventSink`]. The sink
//! abstraction keeps the manager testable without a Tauri runtime.

use crate::terminal_state::{ScreenState, TerminalQueries};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
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
    pub sequence: u64,
}

/// Pane exit notification forwarded to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyExit {
    pub id: PaneId,
    pub success: bool,
    pub code: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtySnapshot {
    pub data: String,
    pub sequence: u64,
    pub cols: u16,
    pub rows: u16,
}
/// Receives pane events. Implemented by the Tauri event bridge in production
/// and by an in-memory channel in tests.
pub trait PtyEventSink: Send + Sync + 'static {
    fn output(&self, id: PaneId, data: String, sequence: u64);
    fn exited(&self, id: PaneId, success: bool, code: Option<i32>);
}

/// Two columns are required for a wide glyph; cap both axes and total cells
/// before the OS or emulator sees the request.
pub fn validate_dimensions(cols: u16, rows: u16) -> anyhow::Result<()> {
    anyhow::ensure!(
        (2..=1000).contains(&cols)
            && (1..=1000).contains(&rows)
            && u32::from(cols) * u32::from(rows) <= 250_000,
        "terminal dimensions must be 2..=1000 columns, 1..=1000 rows, at most 250000 cells"
    );
    Ok(())
}

struct Session {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
    /// PID of the process spawned directly in the PTY (0 when unknown).
    root_pid: u32,
    /// In-memory emulation of the pane's screen, fed every output chunk.
    /// Backs agent detection snapshots; rendering stays in the frontend.
    screen: Mutex<ScreenState>,
    size: Mutex<(u16, u16)>,
    deliberate_close: AtomicBool,
    owner: crate::process_tree::ProcessOwner,
    headless: bool,
    screen_revision: AtomicU64,
}

/// Lifecycle evidence consumed once by the status worker, never by queries.
#[derive(Debug, Clone)]
pub struct AgentExit {
    pub id: PaneId,
    pub root_pid: u32,
    pub success: Option<bool>,
    pub deliberate: bool,
}

pub struct PtyManager {
    sink: Arc<dyn PtyEventSink>,
    sessions: Arc<Mutex<HashMap<PaneId, Arc<Session>>>>,
    next_id: AtomicU32,
    activity: mpsc::SyncSender<()>,
    activity_rx: Mutex<Option<mpsc::Receiver<()>>>,
    exits: Arc<Mutex<VecDeque<AgentExit>>>,
    headless: bool,
    lifecycle: Mutex<()>,
    closing: AtomicBool,
}

impl PtyManager {
    pub fn new(sink: Arc<dyn PtyEventSink>) -> Self {
        Self::with_headless(sink, false)
    }

    /// A headless pane has one query responder, never competing with xterm.
    pub fn new_headless(sink: Arc<dyn PtyEventSink>) -> Self {
        Self::with_headless(sink, true)
    }

    fn with_headless(sink: Arc<dyn PtyEventSink>, headless: bool) -> Self {
        let (activity, activity_rx) = mpsc::sync_channel(1);
        Self {
            sink,
            headless,
            lifecycle: Mutex::new(()),
            closing: AtomicBool::new(false),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU32::new(1),
            activity,
            activity_rx: Mutex::new(Some(activity_rx)),
            exits: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn take_activity_receiver(&self) -> Option<mpsc::Receiver<()>> {
        self.activity_rx.lock().take()
    }

    pub fn wake_status(&self) {
        let _ = self.activity.try_send(());
    }

    pub fn drain_agent_exits(&self) -> Vec<AgentExit> {
        self.exits.lock().drain(..).collect()
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
        validate_dimensions(cols, rows)?;
        if let Some(cwd) = &cwd {
            match std::fs::metadata(cwd) {
                Ok(metadata) => {
                    anyhow::ensure!(
                        metadata.is_dir(),
                        "Working directory is not a directory: {cwd}"
                    );
                }
                Err(_) => anyhow::bail!("Working directory is unavailable: {cwd}"),
            }
        }
        let _lifecycle = self.lifecycle.lock();
        anyhow::ensure!(
            !self.closing.load(Ordering::Acquire),
            "PTY manager is shutting down"
        );
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

        let mut child = pair.slave.spawn_command(cmd)?;
        let root_pid = child.process_id().unwrap_or(0);
        let owner = match crate::process_tree::ProcessOwner::new(child.as_ref()) {
            Ok(owner) => owner,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        drop(pair.slave);
        let (reader, writer) = match (pair.master.try_clone_reader(), pair.master.take_writer()) {
            (Ok(reader), Ok(writer)) => (reader, writer),
            (reader, writer) => {
                let _ = owner.terminate();
                let _ = child.kill();
                let _ = child.wait();
                return Err(reader.err().or_else(|| writer.err()).unwrap());
            }
        };
        let session = Arc::new(Session {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            root_pid,
            owner,
            headless: self.headless,
            screen: Mutex::new(ScreenState::new(rows, cols)),
            size: Mutex::new((cols, rows)),
            deliberate_close: AtomicBool::new(false),
            screen_revision: AtomicU64::new(0),
        });
        self.sessions.lock().insert(id, Arc::clone(&session));

        let sink = Arc::clone(&self.sink);
        let sessions = Arc::clone(&self.sessions);
        let thread_session = Arc::clone(&session);
        let activity = self.activity.clone();
        let exits = self.exits.clone();
        if let Err(e) = thread::Builder::new()
            .name(format!("ubra-pty-{id}"))
            .spawn(move || {
                reader_loop(
                    id,
                    reader,
                    &thread_session,
                    &sessions,
                    &sink,
                    &activity,
                    &exits,
                )
            })
        {
            // Never leave a reader-less session behind: drop it and kill the child.
            self.sessions.lock().remove(&id);
            let _ = session.owner.terminate();
            let mut child = session.child.lock();
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.into());
        }

        self.wake_status();
        Ok(id)
    }

    fn session(&self, id: PaneId) -> anyhow::Result<Arc<Session>> {
        self.sessions
            .lock()
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no such pane: {id}"))
    }

    /// Write frontend input to the pane.
    pub fn write(&self, id: PaneId, data: &str) -> anyhow::Result<()> {
        let session = self.session(id)?;
        session.writer.lock().write_all(data.as_bytes())?;
        Ok(())
    }

    /// Resize the pane's PTY.
    pub fn resize(&self, id: PaneId, cols: u16, rows: u16) -> anyhow::Result<()> {
        validate_dimensions(cols, rows)?;
        let session = self.session(id)?;
        let mut size = session.size.lock();
        if *size == (cols, rows) {
            return Ok(());
        }
        session.master.lock().resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        session
            .screen
            .lock()
            .parser
            .screen_mut()
            .set_size(rows, cols);
        *size = (cols, rows);
        session.screen_revision.fetch_add(1, Ordering::Release);
        self.wake_status();
        Ok(())
    }

    /// ANSI-formatted snapshot of the pane's emulated screen, suitable for
    /// feeding to a fresh terminal parser to repaint it. Used when a moved
    /// pane's frontend reattaches to its surviving PTY.
    pub fn snapshot(&self, id: PaneId) -> anyhow::Result<PtySnapshot> {
        let session = self.session(id)?;
        let guard = session.screen.lock();
        Ok(guard.snapshot())
    }

    /// Plain-text contents of the pane's emulated screen, for agent
    /// detection. `None` when the pane is gone.
    pub fn screen_text(&self, id: PaneId) -> Option<String> {
        let session = self.session(id).ok()?;
        let guard = session.screen.lock();
        Some(guard.parser.screen().contents())
    }

    pub fn screen_revision(&self, id: PaneId) -> Option<u64> {
        Some(
            self.session(id)
                .ok()?
                .screen_revision
                .load(Ordering::Acquire),
        )
    }

    /// Snapshot of live panes and their root PIDs for the agent watcher.
    pub fn pane_roots(&self) -> Vec<crate::agent_watch::PaneRoots> {
        self.sessions
            .lock()
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
        let session = self.session(id)?;
        session.deliberate_close.store(true, Ordering::SeqCst);
        session.owner.terminate()?;
        self.sessions.lock().remove(&id);
        record_exit(
            &self.exits,
            AgentExit {
                id,
                root_pid: session.root_pid,
                success: None,
                deliberate: true,
            },
        );
        self.wake_status();
        Ok(())
    }

    /// Called before native application exit, which may bypass Rust drops.
    pub fn shutdown(&self) -> anyhow::Result<()> {
        let _lifecycle = self.lifecycle.lock();
        self.closing.store(true, Ordering::Release);
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .iter()
            .map(|(&id, session)| (id, Arc::clone(session)))
            .collect();
        for (_, session) in &sessions {
            session.deliberate_close.store(true, Ordering::SeqCst);
        }
        crate::process_tree::ProcessOwner::terminate_all(
            sessions.iter().map(|(_, session)| &session.owner),
        )?;
        let mut registered = self.sessions.lock();
        for (id, session) in sessions {
            registered.remove(&id);
            record_exit(
                &self.exits,
                AgentExit {
                    id,
                    root_pid: session.root_pid,
                    success: None,
                    deliberate: true,
                },
            );
        }
        self.wake_status();
        Ok(())
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("ubra: pane cleanup on drop failed: {error}");
        }
    }
}

fn record_exit(exits: &Mutex<VecDeque<AgentExit>>, exit: AgentExit) {
    let mut exits = exits.lock();
    // Lifecycle evidence must not be dropped: losing a deliberate-close
    // record could turn it into an apparent unexpected stop. The worker
    // drains this queue each tick; there is at most one record per session.
    exits.push_back(exit);
}

fn publish_output(
    id: PaneId,
    text: String,
    session: &Session,
    sink: &Arc<dyn PtyEventSink>,
    activity: &mpsc::SyncSender<()>,
    queries: &mut TerminalQueries,
) {
    let sequence = {
        let mut state = session.screen.lock();
        state.process(&text);
        if session.headless {
            queries.respond(&text, state.parser.screen(), |reply| {
                let _ = session.writer.lock().write_all(reply.as_bytes());
            });
        }
        state.sequence
    };
    session.screen_revision.fetch_add(1, Ordering::Release);
    let _ = activity.try_send(());
    sink.output(id, text, sequence);
}

fn reader_loop(
    id: PaneId,
    mut reader: Box<dyn Read + Send>,
    session: &Session,
    sessions: &Arc<Mutex<HashMap<PaneId, Arc<Session>>>>,
    sink: &Arc<dyn PtyEventSink>,
    activity: &mpsc::SyncSender<()>,
    exits: &Mutex<VecDeque<AgentExit>>,
) {
    let mut decoder = Utf8Splitter::new();
    let mut queries = TerminalQueries::default();
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let text = decoder.push(&buf[..n]);
                if !text.is_empty() {
                    publish_output(id, text, session, sink, activity, &mut queries);
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
        publish_output(id, tail, session, sink, activity, &mut queries);
    }

    // Reap the exit status without blocking forever.
    let mut status = None;
    {
        let mut child = session.child.lock();
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
    // A root may exit while a resistant child has closed its terminal handles.
    // Natural EOF releases the whole ownership set, not only the root handle.
    // Deliberate close/shutdown already owns cleanup and registry removal.
    if !session.deliberate_close.load(Ordering::SeqCst) {
        if let Err(error) = session.owner.terminate() {
            eprintln!("ubra: pane {id} reader cleanup failed: {error}");
            return; // Keep ownership registered so shutdown can retry.
        }
        if status.is_none() {
            status = session.child.lock().try_wait().ok().flatten();
        }
        sessions.lock().remove(&id);
    }
    let (success, code) = match status {
        Some(s) => (s.success(), Some(s.exit_code() as i32)),
        None => (false, None),
    };
    if !session.deliberate_close.load(Ordering::SeqCst) {
        record_exit(
            exits,
            AgentExit {
                id,
                root_pid: session.root_pid,
                success: code.map(|_| success),
                deliberate: false,
            },
        );
    }
    let _ = activity.try_send(());
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
    fn close_bursts_preserve_every_deliberate_exit_until_consumed() {
        let exits = parking_lot::Mutex::new(std::collections::VecDeque::new());
        for id in 1..=300 {
            super::record_exit(
                &exits,
                super::AgentExit {
                    id,
                    root_pid: id,
                    success: None,
                    deliberate: true,
                },
            );
        }
        let records: Vec<_> = exits.lock().drain(..).collect();
        assert_eq!(records.len(), 300);
        assert!(records
            .iter()
            .enumerate()
            .all(|(at, exit)| exit.id == at as u32 + 1 && exit.deliberate));
    }

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

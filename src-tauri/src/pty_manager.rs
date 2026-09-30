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

/// Output event delivered to the sink, carrying the stable pane identity
/// so stale events cannot affect a replacement process.
#[derive(Debug, Clone)]
pub struct PtyOutputEvent {
    pub id: PaneId,
    pub key: Option<String>,
    pub incarnation: u64,
    pub data: String,
    pub sequence: u64,
}

/// Exit event delivered to the sink.
#[derive(Debug, Clone)]
pub struct PtyExitEvent {
    pub id: PaneId,
    pub key: Option<String>,
    pub incarnation: u64,
    pub success: bool,
    pub code: Option<i32>,
}

/// Launch configuration retained per session for explicit restarts.
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    pub shell: Option<String>,
    pub cwd: Option<String>,
    pub args: Vec<String>,
}

/// History replay included in a live snapshot (bounded; persisted records
/// keep up to [`crate::recovery::MAX_HISTORY_BYTES`]).
pub const SNAPSHOT_HISTORY_BYTES: usize = 512 * 1024;
/// Snapshot page body size; pages must fit [`crate::daemon::MAX_FRAME_BYTES`].
pub const SNAPSHOT_PAGE_BYTES: usize = 192 * 1024;
/// Retained naturally-exited sessions (final screen + history).
pub const MAX_RETAINED_EXITS: usize = 64;

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
    fn output(&self, event: PtyOutputEvent);
    fn exited(&self, event: PtyExitEvent);
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

/// Bounded tail of raw terminal output, backing snapshots and recovery.
#[derive(Debug, Default)]
struct HistoryBuffer {
    chunks: VecDeque<String>,
    bytes: usize,
}

impl HistoryBuffer {
    fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.chunks.push_back(text.to_string());
        self.bytes += text.len();
        while self.bytes > crate::recovery::MAX_HISTORY_BYTES && self.chunks.len() > 1 {
            if let Some(front) = self.chunks.pop_front() {
                self.bytes = self.bytes.saturating_sub(front.len());
            }
        }
        if self.bytes > crate::recovery::MAX_HISTORY_BYTES {
            if let Some(only) = self.chunks.back_mut() {
                crate::recovery::truncate_tail(only, crate::recovery::MAX_HISTORY_BYTES);
                self.bytes = only.len();
            }
        }
    }

    fn contents(&self) -> String {
        let mut out = String::with_capacity(self.bytes);
        for chunk in &self.chunks {
            out.push_str(chunk);
        }
        out
    }
}

struct Session {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
    /// PID of the process spawned directly in the PTY (0 when unknown).
    root_pid: u32,
    /// Stable pane key (layout pane id) for GUI-owned sessions.
    key: Option<String>,
    /// Manager-unique incarnation; never reused within a daemon lifetime.
    incarnation: u64,
    launch: LaunchConfig,
    /// In-memory emulation of the pane's screen, fed every output chunk.
    /// Backs agent detection snapshots; rendering stays in the frontend.
    screen: Mutex<ScreenState>,
    /// Raw output tail feeding snapshots and recovery checkpoints.
    history: Mutex<HistoryBuffer>,
    size: Mutex<(u16, u16)>,
    deliberate_close: AtomicBool,
    /// Natural exit outcome, set before the session is retained.
    exit_status: Mutex<Option<(bool, Option<i32>)>>,
    owner: crate::process_tree::ProcessOwner,
    headless: bool,
    /// A GUI frontend attached to this pane; while any GUI is present the
    /// renderer (xterm) owns terminal query responses, never the daemon.
    gui_marked: AtomicBool,
    screen_revision: AtomicU64,
}

/// Immutable captured snapshot served in bounded pages.
#[derive(Debug, Clone)]
struct CachedSnapshot {
    data: String,
    sequence: u64,
    incarnation: u64,
    cols: u16,
    rows: u16,
}

/// Daemon runtime facts injected into spawned processes for hooks.
#[derive(Debug, Clone, Default)]
struct RuntimeEnv {
    epoch: u64,
    state_dir: Option<String>,
    cli: Option<String>,
}

/// One page of an immutable snapshot.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PagedSnapshot {
    pub id: PaneId,
    pub data: String,
    pub page: usize,
    pub pages: usize,
    pub sequence: u64,
    pub incarnation: u64,
    pub cols: u16,
    pub rows: u16,
}

impl PagedSnapshot {
    fn first(id: PaneId, cached: &CachedSnapshot) -> Self {
        Self::page(id, cached, 0).expect("page 0 always exists")
    }

    fn page(id: PaneId, cached: &CachedSnapshot, page: usize) -> anyhow::Result<Self> {
        let pages = page_count(&cached.data);
        anyhow::ensure!(page < pages, "snapshot page {page} out of range ({pages})");
        Ok(Self {
            id,
            data: page_slice(&cached.data, page),
            page,
            pages,
            sequence: cached.sequence,
            incarnation: cached.incarnation,
            cols: cached.cols,
            rows: cached.rows,
        })
    }
}

pub(crate) fn page_count(data: &str) -> usize {
    data.len().div_ceil(SNAPSHOT_PAGE_BYTES).max(1)
}

/// Byte-bounded page cut on a char boundary; pages may be shorter than the
/// limit near multibyte text.
pub(crate) fn page_slice(data: &str, page: usize) -> String {
    let start = page * SNAPSHOT_PAGE_BYTES;
    if start >= data.len() {
        return String::new();
    }
    let mut end = (start + SNAPSHOT_PAGE_BYTES).min(data.len());
    while end < data.len() && !data.is_char_boundary(end) {
        end += 1;
    }
    let mut actual_start = start;
    while actual_start < end && !data.is_char_boundary(actual_start) {
        // Previous page overran into this one; skip the shared bytes so
        // concatenation never duplicates or splits a character.
        actual_start += 1;
    }
    data[actual_start..end].to_string()
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
    /// Stable pane key to live-or-retained pane id.
    keys: Arc<Mutex<HashMap<String, PaneId>>>,
    /// Naturally-exited sessions retaining their final screen + history.
    exited: Arc<Mutex<HashMap<PaneId, Arc<Session>>>>,
    exited_order: Arc<Mutex<VecDeque<PaneId>>>,
    snapshots: Mutex<HashMap<PaneId, Arc<CachedSnapshot>>>,
    next_id: AtomicU32,
    next_incarnation: AtomicU64,
    runtime: Mutex<RuntimeEnv>,
    activity: mpsc::SyncSender<()>,
    activity_rx: Mutex<Option<mpsc::Receiver<()>>>,
    exits: Arc<Mutex<VecDeque<AgentExit>>>,
    headless: bool,
    /// A GUI frontend currently holds an event subscription. Combined with
    /// per-session marks, this decides terminal query ownership.
    gui_present: Arc<AtomicBool>,
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
            gui_present: Arc::new(AtomicBool::new(false)),
            lifecycle: Mutex::new(()),
            closing: AtomicBool::new(false),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            keys: Arc::new(Mutex::new(HashMap::new())),
            exited: Arc::new(Mutex::new(HashMap::new())),
            exited_order: Arc::new(Mutex::new(VecDeque::new())),
            snapshots: Mutex::new(HashMap::new()),
            next_id: AtomicU32::new(1),
            next_incarnation: AtomicU64::new(1),
            runtime: Mutex::new(RuntimeEnv::default()),
            activity,
            activity_rx: Mutex::new(Some(activity_rx)),
            exits: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Daemon runtime facts injected into spawned processes so hooks can
    /// report sessions back to this daemon lifetime.
    pub fn set_runtime(&self, epoch: u64, state_dir: String, cli: String) {
        *self.runtime.lock() = RuntimeEnv {
            epoch,
            state_dir: Some(state_dir),
            cli: Some(cli),
        };
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
        self.spawn_inner(None, shell, cwd, args, cols, rows)
    }

    /// Spawn a pane owned by a stable key (layout pane id). Fails while
    /// the key still owns a live session; callers attach instead.
    pub fn spawn_keyed(
        &self,
        key: String,
        shell: Option<String>,
        cwd: Option<String>,
        args: Vec<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<PaneId> {
        anyhow::ensure!(!key.is_empty(), "pane key must not be empty");
        self.spawn_inner(Some(key), shell, cwd, args, cols, rows)
    }

    fn spawn_inner(
        &self,
        key: Option<String>,
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
        if let Some(key) = &key {
            if let Some(existing) = self.keys.lock().get(key).copied() {
                if self.sessions.lock().contains_key(&existing) {
                    anyhow::bail!("pane key is already attached: {key}");
                }
            }
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let incarnation = self.next_incarnation.fetch_add(1, Ordering::Relaxed);
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(shell.clone().unwrap_or_else(default_shell));
        for arg in &args {
            cmd.arg(arg);
        }
        cmd.cwd(
            cwd.clone()
                .or_else(default_cwd)
                .unwrap_or_else(|| ".".to_string()),
        );
        // Hooks use these to report the exact session back to this daemon
        // lifetime; stale incarnations are rejected on receipt.
        cmd.env(
            "UBRA_PANE_KEY",
            key.clone().unwrap_or_else(|| id.to_string()),
        );
        cmd.env("UBRA_SESSION_INCARNATION", incarnation.to_string());
        {
            let runtime = self.runtime.lock();
            cmd.env("UBRA_EPOCH", runtime.epoch.to_string());
            if let Some(state_dir) = &runtime.state_dir {
                cmd.env("UBRA_STATE_DIR", state_dir);
            }
            if let Some(cli) = &runtime.cli {
                cmd.env("UBRA_CLI", cli);
            }
        }

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
            key: key.clone(),
            incarnation,
            launch: LaunchConfig { shell, cwd, args },
            owner,
            headless: self.headless,
            screen: Mutex::new(ScreenState::new(rows, cols)),
            history: Mutex::new(HistoryBuffer::default()),
            size: Mutex::new((cols, rows)),
            deliberate_close: AtomicBool::new(false),
            exit_status: Mutex::new(None),
            gui_marked: AtomicBool::new(false),
            screen_revision: AtomicU64::new(0),
        });
        self.sessions.lock().insert(id, Arc::clone(&session));
        if let Some(key) = &key {
            // A retained exited entry for this key is superseded.
            if let Some(stale) = self.keys.lock().insert(key.clone(), id) {
                self.exited.lock().remove(&stale);
                self.exited_order.lock().retain(|kept| *kept != stale);
            }
            self.exited.lock().remove(&id);
        }

        let sink = Arc::clone(&self.sink);
        let sessions = Arc::clone(&self.sessions);
        let exited = Arc::clone(&self.exited);
        let exited_order = Arc::clone(&self.exited_order);
        let thread_session = Arc::clone(&session);
        let activity = self.activity.clone();
        let exits = self.exits.clone();
        let gui_present = Arc::clone(&self.gui_present);
        if let Err(e) = thread::Builder::new()
            .name(format!("ubra-pty-{id}"))
            .spawn(move || {
                reader_loop(
                    id,
                    reader,
                    &thread_session,
                    &sessions,
                    &exited,
                    &exited_order,
                    &sink,
                    &activity,
                    &exits,
                    &gui_present,
                )
            })
        {
            // Never leave a reader-less session behind: drop it and kill the child.
            self.sessions.lock().remove(&id);
            if let Some(key) = &key {
                self.keys.lock().remove(key);
            }
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

    /// Live or retained session (retained sessions keep their final screen).
    fn any_session(&self, id: PaneId) -> anyhow::Result<Arc<Session>> {
        if let Some(session) = self.sessions.lock().get(&id).cloned() {
            return Ok(session);
        }
        self.exited
            .lock()
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no such pane: {id}"))
    }

    /// Pane id currently owning a stable key (live or retained).
    pub fn pane_for_key(&self, key: &str) -> Option<PaneId> {
        self.keys.lock().get(key).copied()
    }

    pub fn key_for(&self, id: PaneId) -> Option<String> {
        self.any_session(id).ok()?.key.clone()
    }

    pub fn incarnation_of(&self, id: PaneId) -> Option<u64> {
        self.any_session(id).ok().map(|s| s.incarnation)
    }

    pub fn is_live(&self, id: PaneId) -> bool {
        self.sessions.lock().contains_key(&id)
    }

    pub fn launch_of(&self, id: PaneId) -> Option<LaunchConfig> {
        self.any_session(id).ok().map(|s| s.launch.clone())
    }

    /// Raw output tail for snapshots and checkpoints. `None` when unknown.
    pub fn history(&self, id: PaneId) -> Option<String> {
        let session = self.any_session(id).ok()?;
        let history = session.history.lock();
        Some(history.contents())
    }

    /// Output sequence watermark (dirty signal for throttled checkpoints).
    pub fn sequence_of(&self, id: PaneId) -> Option<u64> {
        let session = self.any_session(id).ok()?;
        let screen = session.screen.lock();
        Some(screen.sequence)
    }

    /// Current grid size for checkpoints.
    pub fn size_of(&self, id: PaneId) -> Option<(u16, u16)> {
        let session = self.any_session(id).ok()?;
        let size = session.size.lock();
        Some(*size)
    }

    /// Natural exit outcome for retained sessions.
    pub fn exit_status_of(&self, id: PaneId) -> Option<(bool, Option<i32>)> {
        let session = self.any_session(id).ok()?;
        let status = session.exit_status.lock();
        *status
    }

    /// Root PID for agent-report classification.
    pub fn root_pid_of(&self, id: PaneId) -> Option<u32> {
        self.any_session(id).ok().map(|s| s.root_pid)
    }

    /// Mark a pane as GUI-fronted so the renderer owns query responses.
    pub fn set_gui_marked(&self, id: PaneId, marked: bool) {
        if let Ok(session) = self.any_session(id) {
            session.gui_marked.store(marked, Ordering::SeqCst);
        }
    }

    /// Whether any GUI frontend currently subscribes to daemon events.
    /// When no GUI is present the daemon owns query responses everywhere.
    pub fn set_gui_present(&self, present: bool) {
        self.gui_present.store(present, Ordering::SeqCst);
    }

    pub fn gui_present(&self) -> bool {
        self.gui_present.load(Ordering::SeqCst)
    }

    /// Every known stable key (live or retained sessions).
    pub fn all_keys(&self) -> Vec<String> {
        self.keys.lock().keys().cloned().collect()
    }

    /// Drop a retained (naturally-exited) session by id. Live sessions are
    /// never touched; use [`Self::kill`] for those.
    pub fn drop_retained(&self, id: PaneId) -> bool {
        if self.sessions.lock().contains_key(&id) {
            return false;
        }
        let removed = self.exited.lock().remove(&id).is_some();
        self.exited_order.lock().retain(|kept| *kept != id);
        self.snapshots.lock().remove(&id);
        removed
    }

    /// Every known session for checkpointing: (id, live).
    pub fn checkpoint_candidates(&self) -> Vec<(PaneId, bool)> {
        let live = self.sessions.lock();
        let exited = self.exited.lock();
        live.keys()
            .map(|id| (*id, true))
            .chain(exited.keys().map(|id| (*id, false)))
            .collect()
    }

    /// Validate epoch/incarnation on a mutating terminal operation. Stale
    /// requests must never affect a replacement process.
    pub fn check_fresh(
        &self,
        id: PaneId,
        epoch: Option<u64>,
        incarnation: Option<u64>,
    ) -> anyhow::Result<()> {
        let session = self.session(id)?;
        self.check_identity(&session, id, epoch, incarnation)
    }

    /// Validate epoch/incarnation on a read-only operation, accepting
    /// retained naturally-exited sessions.
    pub fn check_fresh_any(
        &self,
        id: PaneId,
        epoch: Option<u64>,
        incarnation: Option<u64>,
    ) -> anyhow::Result<()> {
        let session = self.any_session(id)?;
        self.check_identity(&session, id, epoch, incarnation)
    }

    fn check_identity(
        &self,
        session: &Session,
        id: PaneId,
        epoch: Option<u64>,
        incarnation: Option<u64>,
    ) -> anyhow::Result<()> {
        if let Some(want) = incarnation {
            anyhow::ensure!(
                session.incarnation == want,
                "stale session incarnation for pane {id}"
            );
        }
        if let Some(want) = epoch {
            let current = self.runtime.lock().epoch;
            anyhow::ensure!(current == want, "stale daemon epoch for pane {id}");
        }
        Ok(())
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
    /// pane's frontend reattaches to its surviving PTY. Retained exited
    /// sessions return their final screen.
    pub fn snapshot(&self, id: PaneId) -> anyhow::Result<PtySnapshot> {
        let session = self.any_session(id)?;
        let guard = session.screen.lock();
        Ok(guard.snapshot())
    }

    /// Capture an immutable snapshot (recent history replay plus screen
    /// repaint) and return its first page. Later pages come from
    /// [`Self::snapshot_page`]; the capture stays a consistent cut even as
    /// live output continues, ordered by the sequence watermark.
    pub fn snapshot_paged(&self, id: PaneId) -> anyhow::Result<PagedSnapshot> {
        let session = self.any_session(id)?;
        let (mut replay, screen) = {
            let mut history = session.history.lock().contents();
            crate::recovery::truncate_tail(&mut history, SNAPSHOT_HISTORY_BYTES);
            let guard = session.screen.lock();
            (history, guard.snapshot())
        };
        replay.push_str(&screen.data);
        let cached = Arc::new(CachedSnapshot {
            data: replay,
            sequence: screen.sequence,
            incarnation: session.incarnation,
            cols: screen.cols,
            rows: screen.rows,
        });
        self.snapshots.lock().insert(id, Arc::clone(&cached));
        Ok(PagedSnapshot::first(id, &cached))
    }

    /// Serve one page of the immutable snapshot captured by
    /// [`Self::snapshot_paged`]. A replaced process invalidates the cache.
    pub fn snapshot_page(&self, id: PaneId, page: usize) -> anyhow::Result<PagedSnapshot> {
        let cached = self
            .snapshots
            .lock()
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no snapshot captured for pane {id}"))?;
        let live = self.any_session(id).ok().map(|s| s.incarnation);
        anyhow::ensure!(
            live == Some(cached.incarnation),
            "snapshot expired: pane {id} was replaced"
        );
        PagedSnapshot::page(id, &cached, page)
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
        self.forget(id, session.key.as_deref());
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

    /// Explicit closure by stable key: terminates a live session and drops
    /// any retained state. Returns whether a live process was terminated.
    pub fn kill_by_key(&self, key: &str) -> anyhow::Result<bool> {
        let id = match self.pane_for_key(key) {
            Some(id) => id,
            None => return Ok(false),
        };
        if self.sessions.lock().contains_key(&id) {
            self.kill(id)?;
            return Ok(true);
        }
        self.exited.lock().remove(&id);
        self.exited_order.lock().retain(|kept| *kept != id);
        self.keys.lock().remove(key);
        self.snapshots.lock().remove(&id);
        Ok(false)
    }

    fn forget(&self, id: PaneId, key: Option<&str>) {
        if let Some(key) = key {
            self.keys.lock().remove(key);
        }
        self.exited.lock().remove(&id);
        self.exited_order.lock().retain(|kept| *kept != id);
        self.snapshots.lock().remove(&id);
    }

    /// Terminate every session and drop retained state, keeping the manager
    /// open for new spawns. Returns the number of live panes stopped.
    pub fn stop_all(&self) -> anyhow::Result<usize> {
        let _lifecycle = self.lifecycle.lock();
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
        for (id, session) in &sessions {
            registered.remove(id);
            self.forget(*id, session.key.as_deref());
            record_exit(
                &self.exits,
                AgentExit {
                    id: *id,
                    root_pid: session.root_pid,
                    success: None,
                    deliberate: true,
                },
            );
        }
        self.wake_status();
        Ok(sessions.len())
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
        for (id, session) in &sessions {
            registered.remove(id);
            self.forget(*id, session.key.as_deref());
            record_exit(
                &self.exits,
                AgentExit {
                    id: *id,
                    root_pid: session.root_pid,
                    success: None,
                    deliberate: true,
                },
            );
        }
        self.keys.lock().clear();
        self.exited.lock().clear();
        self.exited_order.lock().clear();
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
    gui_present: &AtomicBool,
) {
    let sequence = {
        let mut state = session.screen.lock();
        state.process(&text);
        // Exactly one query responder: a GUI-fronted pane lets its renderer
        // answer while any GUI is present; headless panes answer here.
        let renderer_owns =
            session.gui_marked.load(Ordering::SeqCst) && gui_present.load(Ordering::SeqCst);
        if session.headless && !renderer_owns {
            queries.respond(&text, state.parser.screen(), |reply| {
                let _ = session.writer.lock().write_all(reply.as_bytes());
            });
        }
        state.sequence
    };
    session.history.lock().push(&text);
    session.screen_revision.fetch_add(1, Ordering::Release);
    let _ = activity.try_send(());
    sink.output(PtyOutputEvent {
        id,
        key: session.key.clone(),
        incarnation: session.incarnation,
        data: text,
        sequence,
    });
}

#[allow(clippy::too_many_arguments)]
fn reader_loop(
    id: PaneId,
    mut reader: Box<dyn Read + Send>,
    session: &Arc<Session>,
    sessions: &Arc<Mutex<HashMap<PaneId, Arc<Session>>>>,
    exited: &Arc<Mutex<HashMap<PaneId, Arc<Session>>>>,
    exited_order: &Arc<Mutex<VecDeque<PaneId>>>,
    sink: &Arc<dyn PtyEventSink>,
    activity: &mpsc::SyncSender<()>,
    exits: &Mutex<VecDeque<AgentExit>>,
    gui_present: &AtomicBool,
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
                    publish_output(id, text, session, sink, activity, &mut queries, gui_present);
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
        publish_output(id, tail, session, sink, activity, &mut queries, gui_present);
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
        // Natural exits retain their final screen + history so reattachment
        // shows the outcome instead of silently rerunning the command.
        exited.lock().insert(id, Arc::clone(session));
        {
            let mut order = exited_order.lock();
            order.push_back(id);
            while order.len() > MAX_RETAINED_EXITS {
                if let Some(oldest) = order.pop_front() {
                    exited.lock().remove(&oldest);
                }
            }
        }
    }
    let (success, code) = match status {
        Some(s) => (s.success(), Some(s.exit_code() as i32)),
        None => (false, None),
    };
    // Record before retention so checkpointing sees the outcome.
    *session.exit_status.lock() = Some((success, code));
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
    sink.exited(PtyExitEvent {
        id,
        key: session.key.clone(),
        incarnation: session.incarnation,
        success,
        code,
    });
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

pub mod agent_clis;
pub mod agent_session;
pub mod agent_status;
pub mod agent_watch;
pub mod cli;
pub mod daemon;
pub mod daemon_client;
pub mod files;
pub mod git;
pub mod git_branch;
pub mod layout_store;
pub mod macos_notify;
mod process_tree;
pub mod pty_manager;
pub mod screen_rules;
pub mod sound;
pub mod telemetry;
mod terminal_state;
pub mod tray;
pub mod usage;
mod window_geometry;

use agent_status::AgentStatusService;
use daemon_client::{connect_or_ensure, daemon_state_dir, DaemonClient, DaemonEvent};
use layout_store::{data_dir, load_layout_from, save_layout_to};
use pty_manager::{
    PaneId, PtyEventSink, PtyExit, PtyManager, PtyOutput, PtySessionInfo, PtySnapshot, SpawnOptions,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;
use tray::{build_tray, show_main, TrayState};

/// Live daemon connection owned by the supervisor thread. Commands fail
/// fast while disconnected; the frontend remounts terminals on reconnect.
#[derive(Default)]
struct DaemonHandle {
    client: parking_lot::Mutex<Option<Arc<DaemonClient>>>,
}

impl DaemonHandle {
    fn set(&self, client: Option<Arc<DaemonClient>>) {
        *self.client.lock() = client;
    }

    fn client(&self) -> Result<Arc<DaemonClient>, String> {
        self.client
            .lock()
            .clone()
            .filter(|client| !client.is_dead())
            .ok_or_else(|| "agent runtime is unavailable".to_string())
    }
}

struct TauriSink(AppHandle);

impl PtyEventSink for TauriSink {
    fn output(&self, id: PaneId, data: String, sequence: u64) {
        let _ = self.0.emit("pty-output", PtyOutput { id, data, sequence });
    }

    fn exited(&self, id: PaneId, success: bool, code: Option<i32>) {
        let _ = self.0.emit("pty-exit", PtyExit { id, success, code });
    }
}

/// Parse a `UBRA_*` kill-switch: exactly "1" enables.
fn kill_switch(var: &Result<String, std::env::VarError>) -> bool {
    matches!(var.as_deref(), Ok("1"))
}

/// Opt-in for the survival architecture: with `UBRA_DAEMON=1`, PTYs live
/// in the background daemon and quitting detaches from them. The default
/// is in-process PTYs: no daemon is spawned or contacted, and quitting
/// stops every pane.
fn daemon_enabled() -> bool {
    kill_switch(&std::env::var("UBRA_DAEMON"))
}

/// Where PTYs live. Local is the default (in-process PTYs, no survival);
/// Daemon is the survival architecture (`UBRA_DAEMON=1`).
enum PtyBackend {
    Local {
        manager: Arc<PtyManager>,
        statuses: AgentStatusService,
    },
    Daemon(DaemonHandle),
}

impl PtyBackend {
    fn is_local(&self) -> bool {
        matches!(self, PtyBackend::Local { .. })
    }

    fn spawn(
        &self,
        shell: Option<String>,
        cwd: Option<String>,
        args: Vec<String>,
        cols: u16,
        rows: u16,
        key: Option<String>,
    ) -> Result<PaneId, String> {
        match self {
            PtyBackend::Local { manager, .. } => manager
                .spawn(SpawnOptions {
                    shell,
                    cwd,
                    args,
                    cols,
                    rows,
                    key,
                    headless: false,
                })
                .map_err(|e| e.to_string()),
            PtyBackend::Daemon(handle) => handle.client()?.spawn(shell, cwd, args, cols, rows, key),
        }
    }

    fn list(&self) -> Result<Vec<PtySessionInfo>, String> {
        match self {
            PtyBackend::Local { manager, .. } => Ok(manager.list()),
            PtyBackend::Daemon(handle) => handle.client()?.list(),
        }
    }

    fn write(&self, id: PaneId, data: &str) -> Result<(), String> {
        match self {
            PtyBackend::Local { manager, .. } => manager.write(id, data).map_err(|e| e.to_string()),
            PtyBackend::Daemon(handle) => handle.client()?.write(id, data),
        }
    }

    fn resize(&self, id: PaneId, cols: u16, rows: u16) -> Result<(), String> {
        match self {
            PtyBackend::Local { manager, .. } => {
                manager.resize(id, cols, rows).map_err(|e| e.to_string())
            }
            PtyBackend::Daemon(handle) => handle.client()?.resize(id, cols, rows),
        }
    }

    fn kill(&self, id: PaneId) -> Result<(), String> {
        match self {
            PtyBackend::Local { manager, .. } => manager.kill(id).map_err(|e| e.to_string()),
            PtyBackend::Daemon(handle) => handle.client()?.kill(id),
        }
    }

    fn snapshot(&self, id: PaneId) -> Result<PtySnapshot, String> {
        match self {
            PtyBackend::Local { manager, .. } => manager.snapshot(id).map_err(|e| e.to_string()),
            PtyBackend::Daemon(handle) => handle.client()?.snapshot(id),
        }
    }

    fn history(&self, key: &str) -> Result<Option<PtySnapshot>, String> {
        match self {
            // No history dir in local mode: every mount starts blank.
            PtyBackend::Local { manager, .. } => Ok(manager.load_history(key)),
            PtyBackend::Daemon(handle) => handle.client()?.history(key),
        }
    }

    fn agent_states(&self) -> Result<serde_json::Value, String> {
        match self {
            PtyBackend::Local { statuses, .. } => {
                let snapshot = statuses.snapshot();
                Ok(serde_json::json!({
                    "revision": snapshot.revision,
                    "states": snapshot.states,
                }))
            }
            PtyBackend::Daemon(handle) => handle.client()?.agent_states(),
        }
    }

    /// Stop in-process panes. Daemon panes are never touched here.
    fn shutdown_local(&self) {
        if let PtyBackend::Local { manager, .. } = self {
            if let Err(error) = manager.shutdown() {
                eprintln!("ubra: local PTY shutdown failed: {error}");
            }
        }
    }
}

fn daemon_status_payload(connected: bool, error: Option<String>) -> serde_json::Value {
    serde_json::json!({"connected": connected, "error": error})
}

/// Supervise the background daemon: ensure one answers, forward its events
/// to the frontend, and keep retrying across disconnects. The daemon owns
/// every PTY, so this process exiting never stops an agent.
fn supervise_daemon(
    app: AppHandle,
    state_dir: std::path::PathBuf,
    rules_dir: Option<std::path::PathBuf>,
) {
    std::thread::Builder::new()
        .name("ubra-daemon-supervisor".to_string())
        .spawn(move || {
            let mut last: Option<(bool, Option<String>)> = None;
            let mut report = |connected: bool, error: Option<String>| {
                let status = (connected, error);
                if last.as_ref() != Some(&status) {
                    let _ = app.emit(
                        "daemon-status",
                        daemon_status_payload(status.0, status.1.clone()),
                    );
                    last = Some(status);
                }
            };
            loop {
                let (tx, rx) = std::sync::mpsc::sync_channel(2048);
                match connect_or_ensure(&state_dir, rules_dir.as_deref(), tx) {
                    Ok(client) => match client.ping() {
                        Ok(_) => {
                            app.state::<DaemonHandle>().set(Some(client));
                            report(true, None);
                            // Ends when the reader drops the sender on death.
                            for event in rx {
                                match event {
                                    DaemonEvent::PtyOutput { id, data, sequence } => {
                                        let _ = app
                                            .emit("pty-output", PtyOutput { id, data, sequence });
                                    }
                                    DaemonEvent::PtyExit { id, success, code } => {
                                        let _ = app.emit("pty-exit", PtyExit { id, success, code });
                                    }
                                    DaemonEvent::AgentUpdate {
                                        revision,
                                        states,
                                        transitions,
                                    } => {
                                        let _ = app.emit(
                                            "agent-state-update",
                                            serde_json::json!({
                                                "revision": revision,
                                                "states": states,
                                                "transitions": transitions,
                                            }),
                                        );
                                    }
                                }
                            }
                            app.state::<DaemonHandle>().set(None);
                            report(false, None);
                        }
                        Err(error) => {
                            eprintln!("ubra: daemon protocol check failed: {error}");
                            app.state::<DaemonHandle>().set(None);
                            report(false, Some(error));
                        }
                    },
                    Err(error) => {
                        eprintln!("ubra: daemon unavailable: {error}");
                        report(false, Some(error));
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        })
        .expect("daemon supervisor thread spawns");
}

#[tauri::command]
fn pty_spawn(
    backend: State<'_, PtyBackend>,
    shell: Option<String>,
    cwd: Option<String>,
    args: Option<Vec<String>>,
    cols: u16,
    rows: u16,
    key: Option<String>,
) -> Result<PaneId, String> {
    backend.spawn(shell, cwd, args.unwrap_or_default(), cols, rows, key)
}

#[tauri::command]
fn pty_list(backend: State<'_, PtyBackend>) -> Result<Vec<PtySessionInfo>, String> {
    backend.list()
}

#[tauri::command]
fn pty_write(backend: State<'_, PtyBackend>, id: PaneId, data: String) -> Result<(), String> {
    backend.write(id, &data)
}

#[tauri::command]
fn pty_resize(
    backend: State<'_, PtyBackend>,
    id: PaneId,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    backend.resize(id, cols, rows)
}

#[tauri::command]
fn pty_kill(backend: State<'_, PtyBackend>, id: PaneId) -> Result<(), String> {
    backend.kill(id)
}

#[tauri::command]
fn pty_snapshot(backend: State<'_, PtyBackend>, id: PaneId) -> Result<PtySnapshot, String> {
    backend.snapshot(id)
}

#[tauri::command]
fn pty_history(backend: State<'_, PtyBackend>, key: String) -> Result<Option<PtySnapshot>, String> {
    backend.history(&key)
}

/// Which backend serves `pty_*`: `"local"` (default, quitting stops
/// panes) or `"daemon"` (survival mode, quitting detaches). The frontend
/// branches quit copy on this; it never changes within a process.
#[tauri::command]
fn pty_backend(backend: State<'_, PtyBackend>) -> &'static str {
    if backend.is_local() {
        "local"
    } else {
        "daemon"
    }
}

#[tauri::command]
fn agent_snapshot(backend: State<'_, PtyBackend>) -> Result<serde_json::Value, String> {
    let snapshot = backend.agent_states()?;
    Ok(serde_json::json!({
        "revision": snapshot["revision"],
        "states": snapshot["states"],
        "transitions": [],
    }))
}

#[tauri::command]
fn git_branch(path: String) -> Option<String> {
    git_branch::branch_for(std::path::Path::new(&path))
}

#[tauri::command]
fn detect_agent_clis() -> Vec<agent_clis::DetectedCli> {
    agent_clis::detect()
}

#[tauri::command]
fn supported_agent_clis() -> Vec<agent_clis::SupportedCli> {
    agent_clis::supported()
}

#[tauri::command]
fn supported_usage_clis() -> Vec<usage::SupportedCli> {
    usage::supported_clis()
}

#[tauri::command]
async fn cli_usage(
    cache: State<'_, usage::UsageCache>,
    cli: String,
    force: bool,
) -> Result<usage::CliUsage, String> {
    Ok(cache.usage(&cli, force).await)
}

#[tauri::command]
fn load_layout(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    let dir = data_dir(&app).map_err(|e| e.to_string())?;
    load_layout_from(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_layout(app: AppHandle, layout: serde_json::Value) -> Result<(), String> {
    let dir = data_dir(&app).map_err(|e| e.to_string())?;
    save_layout_to(&dir, &layout).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_layout(app: AppHandle) -> Result<String, String> {
    let dir = data_dir(&app).map_err(|e| e.to_string())?;
    layout_store::backup_layout_from(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn reset_layout(app: AppHandle, layout: serde_json::Value) -> Result<Option<String>, String> {
    let dir = data_dir(&app).map_err(|e| e.to_string())?;
    layout_store::reset_layout_to(&dir, &layout).map_err(|e| e.to_string())
}

#[tauri::command]
fn fs_list_dir(root: String, path: String) -> Result<files::DirListing, String> {
    files::list_dir(&root, &path)
}

#[tauri::command]
fn fs_read_file(root: String, path: String) -> Result<files::FileContent, String> {
    files::read_file(&root, &path)
}

#[tauri::command]
fn git_status(root: String) -> Result<git::GitStatus, String> {
    git::status(&root)
}

#[tauri::command]
fn git_diff_file(root: String, path: String, staged: bool) -> Result<git::GitDiff, String> {
    git::diff_file(&root, &path, staged)
}

#[tauri::command]
fn git_stage(root: String, paths: Vec<String>) -> Result<String, String> {
    git::stage(&root, &paths)
}

#[tauri::command]
fn git_unstage(root: String, paths: Vec<String>) -> Result<String, String> {
    git::unstage(&root, &paths)
}

#[tauri::command]
fn git_commit(root: String, message: String) -> Result<String, String> {
    git::commit(&root, &message)
}

#[tauri::command]
fn git_push(root: String) -> Result<String, String> {
    git::push(&root)
}

#[tauri::command]
fn git_pull(root: String) -> Result<String, String> {
    git::pull(&root)
}

#[tauri::command]
fn git_branches(root: String) -> Result<git::GitBranches, String> {
    git::branches(&root)
}

#[tauri::command]
fn git_worktrees(root: String) -> Result<Vec<git::GitWorktree>, String> {
    git::worktrees(&root)
}

#[tauri::command]
fn git_switch(root: String, branch: String) -> Result<String, String> {
    git::switch(&root, &branch)
}

#[tauri::command]
fn git_init(root: String) -> Result<String, String> {
    git::init(&root)
}

/// Quit the GUI. In survival mode (`UBRA_DAEMON=1`) this detaches: the
/// background daemon keeps every agent running and reopening reattaches
/// to the same sessions. By default there is no daemon, so local panes
/// stop on exit.
#[tauri::command]
fn quit_app(app: AppHandle) -> Result<(), String> {
    app.state::<ShellState>()
        .quitting
        .store(true, Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

/// Stop every agent via the daemon, then quit. The daemon connection is
/// never spawned here: with no daemon answering there is nothing to stop.
/// In local mode only the in-process panes stop; a stray daemon is untouched.
#[tauri::command]
fn quit_app_and_stop_agents(app: AppHandle) -> Result<(), String> {
    tray::stop_daemon(&app);
    app.state::<ShellState>()
        .quitting
        .store(true, Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn autostart_set(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
struct AppInfo {
    name: String,
    version: String,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Ubra".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// User home directory for the empty-workspace starting point.
#[tauri::command]
fn home_dir() -> Result<String, String> {
    #[cfg(windows)]
    let home = std::env::var("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var("HOME");
    home.map_err(|_| "Home directory is unavailable.".to_string())
}

#[tauri::command]
async fn notify_agent(
    app: AppHandle,
    title: String,
    body: String,
    kind: Option<sound::SoundKind>,
) -> Result<macos_notify::NotifyOutcome, String> {
    if let Some(kind) = kind {
        eprintln!("ubra: agent notification ({kind:?}): {title}");
    }
    #[cfg(target_os = "macos")]
    if macos_notify::is_bundled() {
        return Ok(macos_notify::notify(&title, &body).await);
    }
    // Legacy fire-and-forget path: macOS dev binaries (no bundle proxy for
    // UN) and other desktop platforms. The plugin reports Ok once the payload
    // is queued, so delivery is unconfirmed by construction.
    let result = app.notification().builder().title(title).body(body).show();
    if let Err(e) = &result {
        eprintln!("ubra: system notification failed: {e}");
        return Ok(macos_notify::NotifyOutcome::unavailable(e.to_string()));
    }
    Ok(macos_notify::NotifyOutcome::Attempted)
}

/// Query-only authorization state; never prompts. macOS dev binaries report
/// `Unknown` (UN raises without a bundle), other platforms `Granted`.
#[tauri::command]
async fn notification_permission() -> macos_notify::NotifyPermission {
    macos_notify::permission_state().await
}

/// Ask macOS for permission; shows the OS prompt only while undecided.
/// Call only from the Settings Test button, never from an agent finish.
#[tauri::command]
async fn request_notification_permission() -> macos_notify::NotifyPermission {
    macos_notify::request_permission().await
}

#[tauri::command]
fn play_sound(kind: sound::SoundKind, file: Option<String>) -> Result<(), String> {
    sound::play(kind, file.as_deref()).map_err(|e| e.to_string())
}

#[derive(Default)]
struct ShellState {
    quitting: AtomicBool,
    tray_available: AtomicBool,
    close_warning_pending: AtomicBool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && matches!(payload.event(), PageLoadEvent::Finished) {
                if let Err(error) = window_geometry::keep_main_window_on_screen(&webview.window()) {
                    eprintln!("ubra: could not normalize main window bounds: {error}");
                }
            }
        })
        .setup(|app| {
            let rules_dir = match data_dir(app.handle()) {
                Ok(dir) => Some(dir.join("agent-detection")),
                Err(e) => {
                    eprintln!("ubra: data dir unavailable, bundled detection rules only: {e}");
                    None
                }
            };
            if daemon_enabled() {
                eprintln!("ubra: UBRA_DAEMON=1: PTYs live in the background daemon (survival mode)");
                app.manage(PtyBackend::Daemon(DaemonHandle::default()));
                supervise_daemon(app.handle().clone(), daemon_state_dir(), rules_dir);
            } else {
                eprintln!("ubra: in-process PTYs without survival (set UBRA_DAEMON=1 to opt in)");
                let manager = Arc::new(PtyManager::new(Arc::new(TauriSink(app.handle().clone()))));
                let poll_app = app.handle().clone();
                let statuses = AgentStatusService::start(&manager, rules_dir, move |update| {
                    let _ = poll_app.emit("agent-state-update", &update);
                });
                app.manage(PtyBackend::Local { manager, statuses });
            }
            app.manage(ShellState::default());
            app.manage(TrayState::default());
            app.manage(usage::UsageCache::new());
            match data_dir(app.handle()) {
                Ok(dir) => {
                    telemetry::init_from_disk(&dir);
                }
                Err(e) => eprintln!("ubra: telemetry init skipped: {e}"),
            }
            match build_tray(app.handle()) {
                Ok(()) => app
                    .state::<ShellState>()
                    .tray_available
                    .store(true, Ordering::SeqCst),
                Err(e) => {
                    eprintln!("ubra: tray unavailable; closing quits: {e}");
                    app.dialog()
                        .message(if !daemon_enabled() {
                            "The system tray is unavailable. Closing this window will quit Ubra and stop its terminals. You can also quit from Settings."
                        } else {
                            "The system tray is unavailable. Closing this window will quit Ubra; agents keep running in the background. You can also quit from the app menu."
                        })
                        .title("Tray unavailable")
                        .kind(MessageDialogKind::Warning)
                        .show(|_| {});
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let shell = window.state::<ShellState>();
                if shell.quitting.load(Ordering::SeqCst) {
                    return;
                }
                if shell.close_warning_pending.load(Ordering::SeqCst) {
                    api.prevent_close();
                    return;
                }
                if shell.tray_available.load(Ordering::SeqCst) {
                    match window.hide() {
                        Ok(()) => {
                            api.prevent_close();
                            let _ = window
                                .notification()
                                .builder()
                                .title("Ubra keeps running")
                                .body(
                                    "Agents continue in the tray. Quit from the tray or app menu.",
                                )
                                .show();
                            return;
                        }
                        Err(error) => {
                            eprintln!("ubra: hide failed; closing quits: {error}");
                            api.prevent_close();
                            shell.close_warning_pending.store(true, Ordering::SeqCst);
                            let app = window.app_handle().clone();
                            app.dialog()
                                .message(if !daemon_enabled() {
                                    "Ubra could not hide its window. Closing will quit the app and stop its terminals."
                                } else {
                                    "Ubra could not hide its window. Closing will quit the app; agents keep running in the background."
                                })
                                .title("Unable to hide Ubra")
                                .kind(MessageDialogKind::Warning)
                                .show(move |_| {
                                    app.state::<ShellState>()
                                        .close_warning_pending
                                        .store(false, Ordering::SeqCst);
                                    if let Err(error) = quit_app(app.clone()) {
                                        eprintln!("ubra: close failed: {error}");
                                        app.dialog()
                                            .message(error)
                                            .title("Unable to close Ubra")
                                            .kind(MessageDialogKind::Error)
                                            .show(|_| {});
                                    }
                                });
                            return;
                        }
                    }
                }
                shell.quitting.store(true, Ordering::SeqCst);
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            pty_spawn,
            pty_list,
            pty_write,
            pty_resize,
            pty_kill,
            pty_snapshot,
            pty_history,
            pty_backend,
            agent_snapshot,
            detect_agent_clis,
            supported_agent_clis,
            supported_usage_clis,
            cli_usage,
            git_branch,
            load_layout,
            save_layout,
            export_layout,
            reset_layout,
            fs_list_dir,
            fs_read_file,
            git_status,
            git_diff_file,
            git_stage,
            git_unstage,
            git_commit,
            git_push,
            git_pull,
            git_branches,
            git_worktrees,
            git_switch,
            git_init,
            quit_app,
            quit_app_and_stop_agents,
            autostart_enabled,
            autostart_set,
            telemetry::telemetry_status,
            telemetry::telemetry_set_consent,
            telemetry::telemetry_set_distinct_id,
            telemetry::telemetry_capture,
            telemetry::telemetry_flag,
            notify_agent,
            notification_permission,
            request_notification_permission,
            play_sound,
            tray::tray_update,
            app_info,
            home_dir
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(&event, tauri::RunEvent::Exit) {
                // Daemon mode detaches (agents survive); local mode owns its
                // panes, so stop them on every exit path.
                app.state::<PtyBackend>().shutdown_local();
                telemetry::shutdown_flush();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn kill_switch_accepts_only_one() {
        use std::env::VarError;
        assert!(kill_switch(&Ok("1".to_string())));
        assert!(!kill_switch(&Ok("0".to_string())));
        assert!(!kill_switch(&Ok("true".to_string())));
        assert!(!kill_switch(&Ok(String::new())));
        assert!(!kill_switch(&Err(VarError::NotPresent)));
    }

    enum Event {
        Output(PaneId, String),
        Exit(PaneId, bool),
    }

    struct ChannelSink {
        tx: std::sync::mpsc::Sender<Event>,
    }

    impl PtyEventSink for ChannelSink {
        fn output(&self, id: PaneId, data: String, _sequence: u64) {
            let _ = self.tx.send(Event::Output(id, data));
        }

        fn exited(&self, id: PaneId, success: bool, _code: Option<i32>) {
            let _ = self.tx.send(Event::Exit(id, success));
        }
    }

    fn local_backend(tx: std::sync::mpsc::Sender<Event>) -> PtyBackend {
        let manager = Arc::new(PtyManager::new(Arc::new(ChannelSink { tx })));
        let statuses = AgentStatusService::start(&manager, None, |_| {});
        PtyBackend::Local { manager, statuses }
    }

    fn echo_command() -> (Option<String>, Vec<String>) {
        #[cfg(windows)]
        return (
            Some("cmd.exe".to_string()),
            vec!["/C".to_string(), "echo hello-local".to_string()],
        );
        #[cfg(not(windows))]
        return (
            Some("sh".to_string()),
            vec!["-c".to_string(), "echo hello-local".to_string()],
        );
    }

    fn interactive_shell() -> Option<String> {
        #[cfg(windows)]
        return Some("cmd.exe".to_string());
        #[cfg(not(windows))]
        return Some("sh".to_string());
    }

    /// Local backend drives real PTYs with no daemon involved: a quick
    /// echo pane runs to exit, a live shell lists and kills cleanly,
    /// history stays empty, and agent states keep their wire shape.
    #[test]
    fn local_backend_round_trip_without_daemon() {
        let (tx, rx) = std::sync::mpsc::channel();
        let backend = local_backend(tx);
        assert!(backend.is_local());

        // Quick pane: output event, then a successful exit event.
        let (shell, args) = echo_command();
        let quick = backend
            .spawn(shell, None, args, 80, 24, None)
            .expect("local spawn");
        assert_ne!(quick, 0);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut transcript = String::new();
        let mut handshake_answered = false;
        let exited_success = loop {
            let timeout = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(timeout) {
                Ok(Event::Output(got, data)) => {
                    assert_eq!(got, quick);
                    transcript.push_str(&data);
                    // ConPTY startup query (Windows only): answer it or the
                    // pane withholds all further output. Silent no-op on Unix.
                    if !handshake_answered && transcript.contains("\u{1b}[6n") {
                        handshake_answered = true;
                        let _ = backend.write(quick, "\u{1b}[1;1R");
                    }
                }
                Ok(Event::Exit(got, success)) => {
                    assert_eq!(got, quick);
                    break success;
                }
                Err(_) => panic!("timed out waiting for pane exit; got: {transcript:?}"),
            }
        };
        assert!(exited_success, "pane should exit 0");
        assert!(
            transcript.contains("hello-local"),
            "transcript should contain echo output, got: {transcript:?}"
        );

        // Live pane: listed while alive, gone after kill.
        let live = backend
            .spawn(interactive_shell(), None, Vec::new(), 80, 24, None)
            .expect("local spawn");
        assert!(backend.list().unwrap().iter().any(|s| s.id == live));
        backend
            .snapshot(live)
            .expect("live pane snapshots while alive");
        backend.resize(live, 100, 30).expect("local resize");
        backend.kill(live).expect("local kill");
        assert!(backend.snapshot(live).is_err(), "killed pane must be gone");

        assert!(backend.history("pane-nothing").unwrap().is_none());
        let states = backend.agent_states().unwrap();
        assert!(states["revision"].as_u64().is_some());
        assert!(states["states"].is_object());
        backend.shutdown_local();
    }
}

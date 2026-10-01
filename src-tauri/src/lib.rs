pub mod agent_clis;
pub mod agent_session;
pub mod agent_status;
pub mod agent_watch;
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

struct TauriSink(AppHandle);

impl PtyEventSink for TauriSink {
    fn output(&self, id: PaneId, data: String, sequence: u64) {
        let _ = self.0.emit("pty-output", PtyOutput { id, data, sequence });
    }

    fn exited(&self, id: PaneId, success: bool, code: Option<i32>) {
        let _ = self.0.emit("pty-exit", PtyExit { id, success, code });
    }
}

/// Where PTYs live: always in-process. Quitting stops every pane.
struct PtyBackend {
    manager: Arc<PtyManager>,
    statuses: AgentStatusService,
}

impl PtyBackend {
    fn spawn(
        &self,
        shell: Option<String>,
        cwd: Option<String>,
        args: Vec<String>,
        cols: u16,
        rows: u16,
        key: Option<String>,
    ) -> Result<PaneId, String> {
        self.manager
            .spawn(SpawnOptions {
                shell,
                cwd,
                args,
                cols,
                rows,
                key,
            })
            .map_err(|e| e.to_string())
    }

    fn list(&self) -> Result<Vec<PtySessionInfo>, String> {
        Ok(self.manager.list())
    }

    fn write(&self, id: PaneId, data: &str) -> Result<(), String> {
        self.manager.write(id, data).map_err(|e| e.to_string())
    }

    fn resize(&self, id: PaneId, cols: u16, rows: u16) -> Result<(), String> {
        self.manager
            .resize(id, cols, rows)
            .map_err(|e| e.to_string())
    }

    fn kill(&self, id: PaneId) -> Result<(), String> {
        self.manager.kill(id).map_err(|e| e.to_string())
    }

    fn snapshot(&self, id: PaneId) -> Result<PtySnapshot, String> {
        self.manager.snapshot(id).map_err(|e| e.to_string())
    }

    fn agent_states(&self) -> Result<serde_json::Value, String> {
        let snapshot = self.statuses.snapshot();
        Ok(serde_json::json!({
            "revision": snapshot.revision,
            "states": snapshot.states,
        }))
    }

    /// Stop every pane. Runs on all exit paths.
    fn shutdown(&self) {
        if let Err(error) = self.manager.shutdown() {
            eprintln!("ubra: PTY shutdown failed: {error}");
        }
    }
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

/// Quit the GUI. PTYs run in-process, so quitting stops every pane.
#[tauri::command]
fn quit_app(app: AppHandle) -> Result<(), String> {
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
    let builder = tauri::Builder::default();
    let builder = if std::env::var_os("TAURI_WEBDRIVER_PORT").is_some() {
        builder.plugin(tauri_plugin_wdio_webdriver::init())
    } else {
        builder
    };
    builder
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
            let manager = Arc::new(PtyManager::new(Arc::new(TauriSink(app.handle().clone()))));
            let poll_app = app.handle().clone();
            let statuses = AgentStatusService::start(&manager, rules_dir, move |update| {
                let _ = poll_app.emit("agent-state-update", &update);
            });
            app.manage(PtyBackend { manager, statuses });
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
                    // Native smoke runners may not provide a tray host (notably
                    // Xvfb on Linux). Keep the warning for users, but don't let
                    // a native dialog block automated WebDriver interaction.
                    if std::env::var_os("TAURI_WEBDRIVER_PORT").is_none() {
                        app.dialog()
                            .message("The system tray is unavailable. Closing this window will quit Ubra and stop its terminals. You can also quit from Settings.")
                            .title("Tray unavailable")
                            .kind(MessageDialogKind::Warning)
                            .show(|_| {});
                    }
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
                                .message("Ubra could not hide its window. Closing will quit the app and stop its terminals.")
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
                // The backend owns its panes, so stop them on every exit path.
                app.state::<PtyBackend>().shutdown();
                telemetry::shutdown_flush();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

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
        PtyBackend { manager, statuses }
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

    /// The backend drives real in-process PTYs: a quick echo pane runs
    /// to exit, a live shell lists and kills cleanly, and agent states
    /// keep their wire shape.
    #[test]
    fn backend_round_trip() {
        let (tx, rx) = std::sync::mpsc::channel();
        let backend = local_backend(tx);

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

        let states = backend.agent_states().unwrap();
        assert!(states["revision"].as_u64().is_some());
        assert!(states["states"].is_object());
        backend.shutdown();
    }
}

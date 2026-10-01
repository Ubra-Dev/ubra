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
pub mod usage;
mod window_geometry;

use daemon_client::{connect_or_ensure, daemon_state_dir, DaemonClient, DaemonEvent};
use layout_store::{data_dir, load_layout_from, save_layout_to};
use pty_manager::{PaneId, PtyExit, PtyOutput, PtySessionInfo, PtySnapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;

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
    handle: State<'_, DaemonHandle>,
    shell: Option<String>,
    cwd: Option<String>,
    args: Option<Vec<String>>,
    cols: u16,
    rows: u16,
    key: Option<String>,
) -> Result<PaneId, String> {
    handle
        .client()?
        .spawn(shell, cwd, args.unwrap_or_default(), cols, rows, key)
}

#[tauri::command]
fn pty_list(handle: State<'_, DaemonHandle>) -> Result<Vec<PtySessionInfo>, String> {
    handle.client()?.list()
}

#[tauri::command]
fn pty_write(handle: State<'_, DaemonHandle>, id: PaneId, data: String) -> Result<(), String> {
    handle.client()?.write(id, &data)
}

#[tauri::command]
fn pty_resize(
    handle: State<'_, DaemonHandle>,
    id: PaneId,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    handle.client()?.resize(id, cols, rows)
}

#[tauri::command]
fn pty_kill(handle: State<'_, DaemonHandle>, id: PaneId) -> Result<(), String> {
    handle.client()?.kill(id)
}

#[tauri::command]
fn pty_snapshot(handle: State<'_, DaemonHandle>, id: PaneId) -> Result<PtySnapshot, String> {
    handle.client()?.snapshot(id)
}

#[tauri::command]
fn pty_history(
    handle: State<'_, DaemonHandle>,
    key: String,
) -> Result<Option<PtySnapshot>, String> {
    handle.client()?.history(&key)
}

#[tauri::command]
fn agent_snapshot(handle: State<'_, DaemonHandle>) -> Result<serde_json::Value, String> {
    let snapshot = handle.client()?.agent_states()?;
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

/// Quit the GUI and detach: the background daemon keeps every agent
/// running. Reopening reattaches to the same sessions.
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
#[tauri::command]
fn quit_app_and_stop_agents(app: AppHandle) -> Result<(), String> {
    stop_daemon();
    app.state::<ShellState>()
        .quitting
        .store(true, Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

fn stop_daemon() {
    let (tx, _rx) = std::sync::mpsc::sync_channel(8);
    match DaemonClient::connect(&daemon_state_dir(), tx) {
        Ok(client) => {
            if let Err(error) = client.shutdown_daemon() {
                eprintln!("ubra: daemon shutdown failed: {error}");
            }
        }
        Err(error) => {
            eprintln!("ubra: no daemon to stop: {error}");
        }
    }
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

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(debug_assertions)]
    if std::env::var_os("UBRA_DISABLE_TRAY").as_deref() == Some(std::ffi::OsStr::new("1")) {
        return Err(std::io::Error::other("tray disabled for development smoke").into());
    }
    let show = MenuItem::with_id(app, "show", "Show Ubra", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Ubra", true, None::<&str>)?;
    let quit_stop =
        MenuItem::with_id(app, "quit-stop", "Stop Agents and Quit", true, None::<&str>)?;
    let menu = Menu::new(app)?;
    menu.append(&show)?;
    menu.append(&quit)?;
    menu.append(&quit_stop)?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Ubra")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            "quit" => {
                app.state::<ShellState>()
                    .quitting
                    .store(true, Ordering::SeqCst);
                app.exit(0);
            }
            "quit-stop" => {
                stop_daemon();
                app.state::<ShellState>()
                    .quitting
                    .store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    match window.is_visible() {
                        Ok(true) => {
                            let _ = window.hide();
                        }
                        _ => show_main(app),
                    }
                }
            }
        });

    match app.default_window_icon() {
        Some(icon) => {
            builder = builder.icon(icon.clone());
        }
        None => {
            eprintln!("ubra: no default window icon available; tray icon will be blank");
        }
    }
    builder.build(app)?;
    Ok(())
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
            app.manage(DaemonHandle::default());
            app.manage(ShellState::default());
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
                        .message("The system tray is unavailable. Closing this window will quit Ubra; agents keep running in the background. You can also quit from Settings.")
                        .title("Tray unavailable")
                        .kind(MessageDialogKind::Warning)
                        .show(|_| {});
                }
            }
            let rules_dir = match data_dir(app.handle()) {
                Ok(dir) => Some(dir.join("agent-detection")),
                Err(e) => {
                    eprintln!("ubra: data dir unavailable, bundled detection rules only: {e}");
                    None
                }
            };
            supervise_daemon(app.handle().clone(), daemon_state_dir(), rules_dir);
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
                                    "Agents continue in the tray. Quit from the tray or Settings.",
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
                                .message("Ubra could not hide its window. Closing will quit the app; agents keep running in the background.")
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
            agent_snapshot,
            detect_agent_clis,
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
            app_info,
            home_dir
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // Exiting never stops agents: the daemon owns every PTY.
            if matches!(&event, tauri::RunEvent::Exit) {
                telemetry::shutdown_flush();
            }
        });
}

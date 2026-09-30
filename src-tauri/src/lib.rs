pub mod agent_adapters;
pub mod agent_clis;
pub mod agent_status;
pub mod agent_watch;
pub mod cli;
pub mod daemon;
pub mod daemon_link;
pub mod files;
pub mod git;
pub mod git_branch;
pub mod launch;
pub mod layout_store;
mod process_tree;
pub mod pty_manager;
pub mod recovery;
pub mod screen_rules;
pub mod sound;
pub mod telemetry;
mod terminal_state;
pub mod usage;

use daemon_link::DaemonLink;
use layout_store::{data_dir, load_layout_from, save_layout_to};
use pty_manager::PaneId;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;

/// Attach-or-create result from the daemon. `ok: false` carries the
/// `unavailable` outcome (saved data + retry) rather than a transport error.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AttachResult {
    pub ok: bool,
    pub pane: PaneId,
    pub attached: String,
    pub resumed: bool,
    pub epoch: u64,
    pub incarnation: u64,
    pub note: Option<String>,
    pub error: Option<String>,
    pub exit: Option<ExitInfo>,
    pub replay: Option<String>,
    pub replay_pages: Option<usize>,
    pub replay_truncated: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExitInfo {
    pub success: bool,
    pub code: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotResult {
    pub pane: PaneId,
    pub page: usize,
    pub pages: usize,
    pub data: String,
    pub sequence: u64,
    pub incarnation: u64,
    pub epoch: u64,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayResult {
    pub key: String,
    pub page: usize,
    pub pages: usize,
    pub data: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatusInfo {
    pub connected: bool,
    pub epoch: Option<u64>,
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("bad daemon reply: {e}"))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn pty_spawn(
    link: State<'_, Arc<DaemonLink>>,
    key: Option<String>,
    shell: Option<String>,
    cwd: Option<String>,
    args: Option<Vec<String>>,
    cols: u16,
    rows: u16,
    agent_recovery: Option<bool>,
) -> Result<AttachResult, String> {
    let Some(key) = key.filter(|k| !k.is_empty()) else {
        return Err("missing pane key".to_string());
    };
    let value = link.request_raw(
        "pty_attach",
        serde_json::json!({
            "key": key, "shell": shell, "cwd": cwd,
            "args": args.unwrap_or_default(),
            "cols": cols, "rows": rows,
            "frontend": true, "agent_recovery": agent_recovery,
        }),
    )?;
    decode(value)
}

#[tauri::command]
fn pty_write(
    link: State<'_, Arc<DaemonLink>>,
    id: PaneId,
    data: String,
    epoch: Option<u64>,
    incarnation: Option<u64>,
) -> Result<(), String> {
    link.request(
        "pty_write",
        serde_json::json!({
            "pane": id, "data": data, "epoch": epoch, "incarnation": incarnation,
        }),
    )?;
    Ok(())
}

#[tauri::command]
fn pty_resize(
    link: State<'_, Arc<DaemonLink>>,
    id: PaneId,
    cols: u16,
    rows: u16,
    epoch: Option<u64>,
    incarnation: Option<u64>,
) -> Result<(), String> {
    link.request(
        "pty_resize",
        serde_json::json!({
            "pane": id, "cols": cols, "rows": rows,
            "epoch": epoch, "incarnation": incarnation,
        }),
    )?;
    Ok(())
}

#[tauri::command]
fn pty_kill(
    link: State<'_, Arc<DaemonLink>>,
    id: PaneId,
    epoch: Option<u64>,
    incarnation: Option<u64>,
) -> Result<(), String> {
    link.request(
        "pty_kill",
        serde_json::json!({"pane": id, "epoch": epoch, "incarnation": incarnation}),
    )?;
    Ok(())
}

#[tauri::command]
fn pty_close(
    link: State<'_, Arc<DaemonLink>>,
    id: Option<PaneId>,
    key: Option<String>,
) -> Result<bool, String> {
    let value = link.request("pty_close", serde_json::json!({"pane": id, "key": key}))?;
    Ok(value["closed"].as_bool().unwrap_or(false))
}

#[tauri::command]
fn pty_restart(
    link: State<'_, Arc<DaemonLink>>,
    key: String,
    shell: Option<String>,
    cwd: Option<String>,
    args: Option<Vec<String>>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<AttachResult, String> {
    let value = link.request_raw(
        "pty_restart",
        serde_json::json!({
            "key": key, "shell": shell, "cwd": cwd,
            "args": args, "cols": cols, "rows": rows, "frontend": true,
        }),
    )?;
    decode(value)
}

#[tauri::command]
fn pty_snapshot(
    link: State<'_, Arc<DaemonLink>>,
    id: PaneId,
    epoch: Option<u64>,
    incarnation: Option<u64>,
) -> Result<SnapshotResult, String> {
    let value = link.request(
        "pty_snapshot",
        serde_json::json!({"pane": id, "epoch": epoch, "incarnation": incarnation}),
    )?;
    decode(value)
}

#[tauri::command]
fn pty_snapshot_page(
    link: State<'_, Arc<DaemonLink>>,
    id: PaneId,
    page: usize,
    epoch: Option<u64>,
    incarnation: Option<u64>,
) -> Result<SnapshotResult, String> {
    let value = link.request(
        "pty_snapshot_page",
        serde_json::json!({
            "pane": id, "page": page, "epoch": epoch, "incarnation": incarnation,
        }),
    )?;
    decode(value)
}

#[tauri::command]
fn pty_replay_page(
    link: State<'_, Arc<DaemonLink>>,
    key: String,
    page: usize,
) -> Result<ReplayResult, String> {
    let value = link.request(
        "pty_replay_page",
        serde_json::json!({"key": key, "page": page}),
    )?;
    decode(value)
}

#[tauri::command]
fn daemon_status(link: State<'_, Arc<DaemonLink>>) -> DaemonStatusInfo {
    match link.request_existing("ping", serde_json::json!({})) {
        Ok(value) => DaemonStatusInfo {
            connected: true,
            epoch: value["epoch"].as_u64().or_else(|| link.epoch()),
        },
        Err(_) => DaemonStatusInfo {
            connected: false,
            epoch: link.epoch(),
        },
    }
}

#[tauri::command]
fn daemon_set_options(
    link: State<'_, Arc<DaemonLink>>,
    save_history: bool,
) -> Result<bool, String> {
    let value = link.request(
        "set_options",
        serde_json::json!({"save_history": save_history}),
    )?;
    Ok(value["saveHistory"].as_bool().unwrap_or(save_history))
}

#[tauri::command]
fn agent_snapshot(link: State<'_, Arc<DaemonLink>>) -> Result<serde_json::Value, String> {
    let mut value = link.request("agent_states", serde_json::json!({}))?;
    value["transitions"] = serde_json::json!([]);
    Ok(value)
}

fn cli_home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/"))
}

#[tauri::command]
fn integrations_status() -> Vec<agent_adapters::IntegrationStatus> {
    agent_adapters::integration_status(&cli_home_dir())
}

#[tauri::command]
fn integrations_install(family: Option<String>) -> Result<bool, String> {
    if let Some(family) = &family {
        if family != "claude" {
            return Err(format!("no verified hook mechanism for family: {family}"));
        }
    }
    let cli = crate::launch::cli_binary().to_string_lossy().into_owned();
    agent_adapters::install_claude_hooks(&cli_home_dir(), &cli).map_err(|e| e.to_string())
}

#[tauri::command]
fn integrations_remove(family: Option<String>) -> Result<bool, String> {
    if let Some(family) = &family {
        if family != "claude" {
            return Err(format!("no verified hook mechanism for family: {family}"));
        }
    }
    agent_adapters::remove_claude_hooks(&cli_home_dir()).map_err(|e| e.to_string())
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
    // Infallible by design: every failure mode is a CliUsage status.
    Ok(cache.usage(&cli, force).await)
}

#[tauri::command]
fn load_layout(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    let dir = data_dir(&app).map_err(|e| e.to_string())?;
    load_layout_from(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_layout(
    app: AppHandle,
    link: State<'_, Arc<DaemonLink>>,
    layout: serde_json::Value,
) -> Result<(), String> {
    // Route layout commits through the daemon so removals close their
    // runtime sessions and recovery records, including in hidden tabs.
    // The local write stays authoritative; a failed forward only defers
    // runtime cleanup to the next commit.
    if let Err(error) = link.request("layout_commit", serde_json::json!({"layout": layout})) {
        eprintln!("ubra: layout_commit failed ({error}); runtime cleanup deferred");
    }
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
fn load_saved_setups(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    let dir = data_dir(&app).map_err(|e| e.to_string())?;
    layout_store::load_saved_setups_from(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_saved_setups(app: AppHandle, setups: serde_json::Value) -> Result<(), String> {
    let dir = data_dir(&app).map_err(|e| e.to_string())?;
    layout_store::save_saved_setups_to(&dir, &setups).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_saved_setups(app: AppHandle) -> Result<String, String> {
    let dir = data_dir(&app).map_err(|e| e.to_string())?;
    layout_store::backup_saved_setups_from(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn reset_saved_setups(app: AppHandle, setups: serde_json::Value) -> Result<Option<String>, String> {
    let dir = data_dir(&app).map_err(|e| e.to_string())?;
    layout_store::reset_saved_setups_to(&dir, &setups).map_err(|e| e.to_string())
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

fn begin_quit(app: &AppHandle) {
    app.state::<ShellState>()
        .quitting
        .store(true, Ordering::SeqCst);
    app.state::<Arc<DaemonLink>>().stop();
}

/// Quit the desktop UI, leaving terminal processes running in the
/// background daemon. The frontend flushes pending layout changes first.
#[tauri::command]
fn quit_app(app: AppHandle, link: State<'_, Arc<DaemonLink>>) -> Result<(), String> {
    // Best-effort final checkpoint; never start a daemon just to quit.
    let _ = link.request_existing("flush", serde_json::json!({}));
    begin_quit(&app);
    app.exit(0);
    Ok(())
}

/// Confirmed destructive quit: terminate every terminal session, drop
/// automatic recovery records, then quit. The saved layout is retained.
#[tauri::command]
fn stop_all_terminals_and_quit(
    app: AppHandle,
    link: State<'_, Arc<DaemonLink>>,
) -> Result<(), String> {
    match link.request_existing("stop_all", serde_json::json!({})) {
        Ok(_) => {}
        Err(error) => {
            // No daemon means nothing to stop; anything else is reported.
            if !error.contains("missing or unsafe daemon port file")
                && !error.contains("Connection refused")
            {
                return Err(format!("could not stop terminals: {error}"));
            }
        }
    }
    begin_quit(&app);
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

#[tauri::command]
fn notify_agent(
    app: AppHandle,
    title: String,
    body: String,
    kind: Option<sound::SoundKind>,
) -> Result<(), String> {
    if let Some(kind) = kind {
        eprintln!("ubra: agent notification ({kind:?}): {title}");
    }
    let result = app.notification().builder().title(title).body(body).show();
    if let Err(e) = &result {
        eprintln!("ubra: system notification failed: {e}");
    }
    result.map_err(|e| e.to_string())
}

#[tauri::command]
fn play_sound(
    kind: sound::SoundKind,
    style: Option<sound::ChimeStyle>,
    file: Option<String>,
) -> Result<(), String> {
    sound::play(kind, style.unwrap_or_default(), file.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn check_sound_file(path: String) -> bool {
    sound::file_decodes(&path)
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
    let stop_quit = MenuItem::with_id(
        app,
        "stop-quit",
        "Stop all terminals and quit",
        true,
        None::<&str>,
    )?;
    let menu = Menu::new(app)?;
    menu.append(&show)?;
    menu.append(&quit)?;
    menu.append(&stop_quit)?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Ubra")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            // The frontend flushes pending layout changes, then quits.
            "quit" => {
                let _ = app.emit("request-quit", ());
            }
            // The frontend confirms, then stops terminals and quits.
            "stop-quit" => {
                show_main(app);
                let _ = app.emit("request-stop-all-quit", ());
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
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let state_dir = std::env::var_os("UBRA_STATE_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(daemon::default_state_dir);
            app.manage(DaemonLink::start(app.handle().clone(), state_dir));
            app.manage(ShellState::default());
            match build_tray(app.handle()) {
                Ok(()) => app
                    .state::<ShellState>()
                    .tray_available
                    .store(true, Ordering::SeqCst),
                Err(e) => {
                    eprintln!("ubra: tray unavailable; closing quits: {e}");
                    app.dialog()
                        .message("The system tray is unavailable. Closing this window will quit Ubra; terminals keep running in the background. Stop them from Settings.")
                        .title("Tray unavailable")
                        .kind(MessageDialogKind::Warning)
                        .show(|_| {});
                }
            }
            app.manage(usage::UsageCache::new());
            match layout_store::data_dir(app.handle()) {
                Ok(dir) => {
                    telemetry::init_from_disk(&dir);
                }
                Err(e) => eprintln!("ubra: telemetry init skipped: {e}"),
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
                                    "Terminals keep running in the tray. Quitting also leaves them running.",
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
                                .message("Ubra could not hide its window. Closing will quit Ubra; terminals keep running in the background.")
                                .title("Unable to hide Ubra")
                                .kind(MessageDialogKind::Warning)
                                .show(move |_| {
                                    app.state::<ShellState>()
                                        .close_warning_pending
                                        .store(false, Ordering::SeqCst);
                                    // The frontend flushes layout changes, then quits.
                                    let _ = app.emit("request-quit", ());
                                });
                            return;
                        }
                    }
                }
                // No tray: ask the frontend to flush layout changes, then quit.
                // Terminals keep running in the background daemon.
                api.prevent_close();
                let _ = window.app_handle().emit("request-quit", ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            pty_spawn,
            pty_write,
            pty_resize,
            pty_kill,
            pty_close,
            pty_restart,
            pty_snapshot,
            pty_snapshot_page,
            pty_replay_page,
            daemon_status,
            daemon_set_options,
            agent_snapshot,
            integrations_status,
            integrations_install,
            integrations_remove,
            detect_agent_clis,
            supported_usage_clis,
            cli_usage,
            git_branch,
            load_layout,
            save_layout,
            export_layout,
            reset_layout,
            load_saved_setups,
            save_saved_setups,
            export_saved_setups,
            reset_saved_setups,
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
            stop_all_terminals_and_quit,
            autostart_enabled,
            autostart_set,
            telemetry::telemetry_status,
            telemetry::telemetry_set_consent,
            telemetry::telemetry_set_distinct_id,
            telemetry::telemetry_capture,
            telemetry::telemetry_flag,
            notify_agent,
            play_sound,
            check_sound_file,
            app_info
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Quitting disconnects the UI; terminals keep running in the
            // daemon. Route through the frontend so pending layout changes
            // flush first. An in-progress quit is never intercepted.
            match event {
                tauri::RunEvent::ExitRequested { api, .. } => {
                    if !app
                        .state::<ShellState>()
                        .quitting
                        .load(Ordering::SeqCst)
                    {
                        api.prevent_exit();
                        let _ = app.emit("request-quit", ());
                    }
                }
                tauri::RunEvent::Exit => {
                    telemetry::shutdown_flush();
                }
                _ => {}
            }
        });
}

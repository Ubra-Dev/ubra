pub mod agent_clis;
pub mod agent_status;
pub mod agent_watch;
pub mod cli;
pub mod daemon;
pub mod git_branch;
pub mod layout_store;
mod process_tree;
pub mod pty_manager;
pub mod screen_rules;
pub mod sound;
mod terminal_state;

use agent_status::{AgentStatusService, AgentUpdate};
use layout_store::{data_dir, load_layout_from, save_layout_to};
use pty_manager::{PaneId, PtyEventSink, PtyExit, PtyManager, PtyOutput, PtySnapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;

struct TauriSink(AppHandle);

impl PtyEventSink for TauriSink {
    fn output(&self, id: PaneId, data: String, sequence: u64) {
        let _ = self.0.emit("pty-output", PtyOutput { id, data, sequence });
    }

    fn exited(&self, id: PaneId, success: bool, code: Option<i32>) {
        let _ = self.0.emit("pty-exit", PtyExit { id, success, code });
    }
}

#[tauri::command]
fn pty_spawn(
    manager: State<'_, Arc<PtyManager>>,
    shell: Option<String>,
    cwd: Option<String>,
    args: Option<Vec<String>>,
    cols: u16,
    rows: u16,
) -> Result<PaneId, String> {
    manager
        .spawn(shell, cwd, args.unwrap_or_default(), cols, rows)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn pty_write(manager: State<'_, Arc<PtyManager>>, id: PaneId, data: String) -> Result<(), String> {
    manager.write(id, &data).map_err(|e| e.to_string())
}

#[tauri::command]
fn pty_resize(
    manager: State<'_, Arc<PtyManager>>,
    id: PaneId,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    manager.resize(id, cols, rows).map_err(|e| e.to_string())
}

#[tauri::command]
fn pty_kill(manager: State<'_, Arc<PtyManager>>, id: PaneId) -> Result<(), String> {
    manager.kill(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn pty_snapshot(manager: State<'_, Arc<PtyManager>>, id: PaneId) -> Result<PtySnapshot, String> {
    manager.snapshot(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn agent_snapshot(service: State<'_, AgentStatusService>) -> AgentUpdate {
    service.snapshot()
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
fn quit_app(app: AppHandle, manager: State<'_, Arc<PtyManager>>) -> Result<(), String> {
    manager.shutdown().map_err(|e| e.to_string())?;
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
    let menu = Menu::new(app)?;
    menu.append(&show)?;
    menu.append(&quit)?;

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let manager = Arc::new(PtyManager::new(Arc::new(TauriSink(app.handle().clone()))));
            app.manage(manager.clone());
            app.manage(ShellState::default());
            match build_tray(app.handle()) {
                Ok(()) => app
                    .state::<ShellState>()
                    .tray_available
                    .store(true, Ordering::SeqCst),
                Err(e) => {
                    eprintln!("ubra: tray unavailable; closing quits: {e}");
                    app.dialog()
                        .message("The system tray is unavailable. Closing this window will quit Ubra and stop its terminals. You can also quit from Settings.")
                        .title("Tray unavailable")
                        .kind(MessageDialogKind::Warning)
                        .show(|_| {});
                }
            }
            let poll_app = app.handle().clone();
            let rules_dir = match data_dir(app.handle()) {
                Ok(dir) => Some(dir.join("agent-detection")),
                Err(e) => {
                    eprintln!("ubra: data dir unavailable, bundled detection rules only: {e}");
                    None
                }
            };
            let service = AgentStatusService::start(&manager, rules_dir, move |update| {
                let _ = poll_app.emit("agent-state-update", &update);
            });
            app.manage(service);
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
                                .message("Ubra could not hide its window. Closing will quit the app and stop its terminals.")
                                .title("Unable to hide Ubra")
                                .kind(MessageDialogKind::Warning)
                                .show(move |_| {
                                    app.state::<ShellState>()
                                        .close_warning_pending
                                        .store(false, Ordering::SeqCst);
                                    if let Err(error) =
                                        quit_app(app.clone(), app.state::<Arc<PtyManager>>())
                                    {
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
                if let Err(error) = window.state::<Arc<PtyManager>>().shutdown() {
                    api.prevent_close();
                    eprintln!("ubra: close failed: {error}");
                    let _ = window
                        .notification()
                        .builder()
                        .title("Unable to close Ubra")
                        .body(error.to_string())
                        .show();
                    return;
                }
                shell.quitting.store(true, Ordering::SeqCst);
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            pty_spawn,
            pty_write,
            pty_resize,
            pty_kill,
            pty_snapshot,
            agent_snapshot,
            detect_agent_clis,
            git_branch,
            load_layout,
            save_layout,
            export_layout,
            reset_layout,
            load_saved_setups,
            save_saved_setups,
            export_saved_setups,
            reset_saved_setups,
            quit_app,
            autostart_enabled,
            autostart_set,
            notify_agent,
            play_sound,
            check_sound_file,
            app_info
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                if let Err(error) = app.state::<Arc<PtyManager>>().shutdown() {
                    eprintln!("ubra: shutdown failed: {error}");
                }
            }
        });
}

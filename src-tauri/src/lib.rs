pub mod agent_watch;
pub mod cli;
pub mod daemon;
pub mod layout_store;
pub mod pty_manager;
pub mod screen_rules;
pub mod sound;

use agent_watch::Watcher;
use layout_store::{data_dir, load_layout_from, save_layout_to};
use pty_manager::{PaneId, PtyEventSink, PtyExit, PtyManager, PtyOutput};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_notification::NotificationExt;

struct TauriSink(AppHandle);

impl PtyEventSink for TauriSink {
    fn output(&self, id: PaneId, data: String) {
        let _ = self.0.emit("pty-output", PtyOutput { id, data });
    }

    fn exited(&self, id: PaneId, success: bool, code: Option<i32>) {
        let _ = self.0.emit("pty-exit", PtyExit { id, success, code });
    }
}

#[tauri::command]
fn pty_spawn(
    manager: State<'_, PtyManager>,
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
fn pty_write(manager: State<'_, PtyManager>, id: PaneId, data: String) -> Result<(), String> {
    manager.write(id, &data).map_err(|e| e.to_string())
}

#[tauri::command]
fn pty_resize(
    manager: State<'_, PtyManager>,
    id: PaneId,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    manager.resize(id, cols, rows).map_err(|e| e.to_string())
}

#[tauri::command]
fn pty_kill(manager: State<'_, PtyManager>, id: PaneId) -> Result<(), String> {
    manager.kill(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn pty_snapshot(manager: State<'_, PtyManager>, id: PaneId) -> Result<String, String> {
    manager.snapshot(id).map_err(|e| e.to_string())
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
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
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
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            app.manage(PtyManager::new(Arc::new(TauriSink(app.handle().clone()))));
            app.manage(ShellState::default());
            if let Err(e) = build_tray(app.handle()) {
                eprintln!("ubra: failed to build tray icon: {e}");
            }
            let poll_app = app.handle().clone();
            let rules_dir = match data_dir(app.handle()) {
                Ok(dir) => Some(dir.join("agent-detection")),
                Err(e) => {
                    eprintln!("ubra: data dir unavailable, bundled detection rules only: {e}");
                    None
                }
            };
            std::thread::Builder::new()
                .name("ubra-agent-watch".to_string())
                .spawn(move || {
                    let mut watcher = match rules_dir {
                        Some(dir) => Watcher::with_dir(dir),
                        None => Watcher::bundled(),
                    };
                    let mut last = String::new();
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        let states = {
                            let manager = poll_app.state::<PtyManager>();
                            watcher.poll(&manager)
                        };
                        let json = serde_json::to_string(&states).unwrap_or_default();
                        if json != last {
                            eprintln!("ubra: agent states changed: {json}");
                            last = json;
                            let _ = poll_app.emit("agent-states", &states);
                        }
                    }
                })
                .expect("failed to spawn agent watcher");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let quitting = window.state::<ShellState>().quitting.load(Ordering::SeqCst);
                if quitting {
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
                // Hiding instead of quitting surprises Cmd+W/Cmd+Q muscle
                // memory; say where the app went.
                let _ = window
                    .notification()
                    .builder()
                    .title("Ubra keeps running")
                    .body("Agents continue in the tray. Quit from the tray menu.")
                    .show();
            }
        })
        .invoke_handler(tauri::generate_handler![
            pty_spawn,
            pty_write,
            pty_resize,
            pty_kill,
            pty_snapshot,
            load_layout,
            save_layout,
            autostart_enabled,
            autostart_set,
            notify_agent,
            play_sound,
            check_sound_file,
            app_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

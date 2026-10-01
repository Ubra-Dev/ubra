//! Live menu-bar/tray status surface.
//!
//! The tray icon doubles as a macOS menu-bar item: the frontend pushes a
//! [`TraySummary`] whenever agent state, layout, or tray prefs change, and
//! [`tray_update`] rebuilds the menu plus the menu-bar title and tooltip.
//! Agent-row clicks route back through the `tray-focus-pane` event so the
//! frontend can reveal and focus the pane.

use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};

pub const TRAY_ID: &str = "main";
const AGENT_ITEM_PREFIX: &str = "tray-agent-";
/// Menu rows per update; the frontend caps lower, this is a backstop.
const MAX_MENU_ROWS: usize = 64;
const MAX_TITLE_CHARS: usize = 32;
const MAX_TOOLTIP_CHARS: usize = 512;
const MAX_LINE_CHARS: usize = 200;
const MAX_ROW_CHARS: usize = 160;

/// One agent row in the tray menu. Labels arrive prebuilt from the frontend,
/// which owns workspace/tab names and the effective rollup.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayAgentRow {
    pub node_id: String,
    pub agent: String,
    pub context: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraySummary {
    /// Menu-bar text (`set_title`); `None` clears it. macOS/Linux only,
    /// unsupported on Windows where the call is a no-op.
    pub title: Option<String>,
    pub tooltip: String,
    pub header: String,
    pub agents: Vec<TrayAgentRow>,
    pub overflow: u32,
    pub show_agents: bool,
}

/// Last applied payload, so identical pushes skip the rebuild.
#[derive(Default)]
pub struct TrayState {
    last_payload: parking_lot::Mutex<Option<String>>,
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn static_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "show", "Show Ubra", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Ubra", true, None::<&str>)?;
    let menu = Menu::new(app)?;
    menu.append(&show)?;
    menu.append(&quit)?;
    Ok(menu)
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(debug_assertions)]
    if std::env::var_os("UBRA_DISABLE_TRAY").as_deref() == Some(std::ffi::OsStr::new("1")) {
        return Err(std::io::Error::other("tray disabled for development smoke").into());
    }
    let menu = static_menu(app)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("Ubra")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "show" => show_main(app),
                "quit" => {
                    // Route through the frontend so tray Quit shares the
                    // confirmation dialog (and remembered choice) with menus.
                    show_main(app);
                    let _ = app.emit("quit-requested", ());
                }
                _ => {
                    if let Some(node) = node_id_from_menu_id(id) {
                        show_main(app);
                        let _ = app.emit("tray-focus-pane", node);
                    }
                }
            }
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

/// Apply a frontend status push. Ok-noop when the tray is unavailable, and a
/// skip (not a rebuild) when the payload matches the last applied one.
#[tauri::command]
pub fn tray_update(
    app: AppHandle,
    tray: State<'_, TrayState>,
    summary: TraySummary,
) -> Result<(), String> {
    if !app
        .state::<crate::ShellState>()
        .tray_available
        .load(Ordering::SeqCst)
    {
        return Ok(());
    }
    let Some(icon) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let fingerprint = serde_json::to_string(&summary).map_err(|e| e.to_string())?;
    {
        let mut last = tray.last_payload.lock();
        if last.as_ref() == Some(&fingerprint) {
            return Ok(());
        }
        *last = Some(fingerprint);
    }
    let title = summary
        .title
        .as_deref()
        .map(|text| truncate(text, MAX_TITLE_CHARS));
    icon.set_title(title).map_err(|e| e.to_string())?;
    icon.set_tooltip(Some(truncate(&summary.tooltip, MAX_TOOLTIP_CHARS)))
        .map_err(|e| e.to_string())?;
    let menu = dynamic_menu(&app, &summary).map_err(|e| e.to_string())?;
    icon.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    Ok(())
}

fn dynamic_menu(app: &AppHandle, summary: &TraySummary) -> tauri::Result<Menu<tauri::Wry>> {
    if !summary.show_agents {
        return static_menu(app);
    }
    let menu = Menu::new(app)?;
    let header = MenuItem::with_id(
        app,
        "tray-header",
        truncate(&summary.header, MAX_LINE_CHARS),
        false,
        None::<&str>,
    )?;
    menu.append(&header)?;
    let shown = summary.agents.iter().take(MAX_MENU_ROWS);
    for row in shown {
        let item = MenuItem::with_id(
            app,
            menu_id_for_node(&row.node_id),
            format_row(row),
            true,
            None::<&str>,
        )?;
        menu.append(&item)?;
    }
    if summary.agents.is_empty() {
        let empty = MenuItem::with_id(app, "tray-empty", "No agents running", false, None::<&str>)?;
        menu.append(&empty)?;
    } else if summary.overflow > 0 {
        let more = MenuItem::with_id(
            app,
            "tray-overflow",
            format!("+{} more", summary.overflow),
            false,
            None::<&str>,
        )?;
        menu.append(&more)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let show = MenuItem::with_id(app, "show", "Show Ubra", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Ubra", true, None::<&str>)?;
    menu.append(&show)?;
    menu.append(&quit)?;
    Ok(menu)
}

fn menu_id_for_node(node_id: &str) -> String {
    format!("{AGENT_ITEM_PREFIX}{node_id}")
}

fn node_id_from_menu_id(menu_id: &str) -> Option<String> {
    let node = menu_id.strip_prefix(AGENT_ITEM_PREFIX)?;
    if node.is_empty() {
        return None;
    }
    Some(node.to_string())
}

fn glyph_for_status(status: &str) -> char {
    match status {
        "working" => '●',
        "blocked" => '■',
        "attention" => '!',
        "done" => '○',
        "idle" => '·',
        _ => '?',
    }
}

fn format_row(row: &TrayAgentRow) -> String {
    let agent = truncate(&row.agent, 60);
    let context = truncate(&row.context, 80);
    truncate(
        &format!("{} {agent} — {context}", glyph_for_status(&row.status)),
        MAX_ROW_CHARS,
    )
}

/// Char-boundary-safe truncation; menus must never see a sliced codepoint.
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(node_id: &str, agent: &str, context: &str, status: &str) -> TrayAgentRow {
        TrayAgentRow {
            node_id: node_id.into(),
            agent: agent.into(),
            context: context.into(),
            status: status.into(),
        }
    }

    fn summary() -> TraySummary {
        TraySummary {
            title: Some("● 2".into()),
            tooltip: "2 working".into(),
            header: "2 working".into(),
            agents: vec![
                row("pane-a", "Claude Code", "ws · tab", "working"),
                row("pane-b", "Codex", "ws · tab 2", "done"),
            ],
            overflow: 0,
            show_agents: true,
        }
    }

    #[test]
    fn menu_ids_round_trip_node_ids() {
        for node in ["pane-123", "pane-a-b-c", "tray-agent-pane-x"] {
            let menu_id = menu_id_for_node(node);
            assert_eq!(node_id_from_menu_id(&menu_id).as_deref(), Some(node));
        }
        assert_eq!(node_id_from_menu_id("show"), None);
        assert_eq!(node_id_from_menu_id("tray-agent-"), None);
        assert_eq!(node_id_from_menu_id(""), None);
    }

    #[test]
    fn rows_carry_a_status_glyph() {
        let cases = [
            ("working", '●'),
            ("blocked", '■'),
            ("attention", '!'),
            ("done", '○'),
            ("idle", '·'),
            ("unknown", '?'),
            ("bogus", '?'),
        ];
        for (status, glyph) in cases {
            let label = format_row(&row("pane-a", "Claude Code", "ws · tab", status));
            assert_eq!(
                label,
                format!("{glyph} Claude Code — ws · tab"),
                "status {status}"
            );
        }
    }

    #[test]
    fn truncation_never_splits_codepoints() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("●●●●", 2), "●●");
        assert_eq!(truncate("Claude…", 20).chars().count(), 7);
        let long_agent = "x".repeat(200);
        let label = format_row(&row("pane-a", &long_agent, "ctx", "working"));
        assert!(label.chars().count() <= MAX_ROW_CHARS);
    }

    #[test]
    fn summary_matches_the_frontend_wire_shape() {
        let parsed: TraySummary = serde_json::from_value(serde_json::json!({
            "title": "● 2",
            "tooltip": "2 working",
            "header": "2 working",
            "agents": [
                {"nodeId": "pane-a", "agent": "Claude Code", "context": "ws · tab", "status": "working"},
            ],
            "overflow": 0,
            "showAgents": true,
        }))
        .expect("frontend payload deserializes");
        assert_eq!(parsed.agents.len(), 1);
        let cleared: TraySummary = serde_json::from_value(serde_json::json!({
            "title": null,
            "tooltip": "No agents running",
            "header": "No agents running",
            "agents": [],
            "overflow": 0,
            "showAgents": true,
        }))
        .expect("null title deserializes");
        assert_eq!(cleared.title, None);
        assert!(cleared.agents.is_empty());
        // Fingerprints distinguish payloads so identical pushes skip.
        let mut other = summary();
        assert_eq!(
            serde_json::to_string(&summary()).unwrap(),
            serde_json::to_string(&summary()).unwrap()
        );
        other.title = None;
        assert_ne!(
            serde_json::to_string(&summary()).unwrap(),
            serde_json::to_string(&other).unwrap()
        );
    }
}

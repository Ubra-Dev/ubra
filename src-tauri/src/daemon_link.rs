//! Desktop-side daemon connection: proxies terminal commands to the
//! background daemon and bridges daemon events into Tauri events.
//!
//! Commands use short-lived authenticated connections (one request, one
//! response). A background bridge thread holds the GUI event subscription,
//! forwarding `pty-output`, `pty-exit`, and `agent-state-update` events,
//! and emits `daemon-status` on connectivity changes so the frontend can
//! reattach panes after transport loss or a daemon restart.

use crate::daemon::{read_frame, write_frame, MAX_FRAME_BYTES};
use crate::launch::launch_or_connect;
use std::io::BufReader;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Connectivity state broadcast to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatusEvent {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch: Option<u64>,
    #[serde(default)]
    pub reconnected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct DaemonLink {
    app: AppHandle,
    state_dir: PathBuf,
    epoch: parking_lot::Mutex<Option<u64>>,
    next_id: AtomicU64,
    shutdown: Arc<AtomicBool>,
}

impl DaemonLink {
    /// Start the event bridge; commands connect on demand.
    pub fn start(app: AppHandle, state_dir: PathBuf) -> Arc<Self> {
        let link = Arc::new(Self {
            app,
            state_dir,
            epoch: parking_lot::Mutex::new(None),
            next_id: AtomicU64::new(1),
            shutdown: Arc::new(AtomicBool::new(false)),
        });
        let bridge = link.clone();
        if std::thread::Builder::new()
            .name("ubra-daemon-bridge".to_string())
            .spawn(move || bridge.run())
            .is_err()
        {
            eprintln!("ubra: daemon bridge thread failed to start");
        }
        link
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    pub fn epoch(&self) -> Option<u64> {
        *self.epoch.lock()
    }

    fn emit_status(&self, event: DaemonStatusEvent) {
        if event.connected {
            *self.epoch.lock() = event.epoch;
        }
        let _ = self.app.emit("daemon-status", &event);
    }

    /// One request without `ok` enforcement, for calls whose `ok: false`
    /// responses carry payloads (attach `unavailable` outcomes).
    pub fn request_raw(
        &self,
        op: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.request_once(op, body, false)
    }

    /// One request over a fresh connection, with a single reconnect-and-retry
    /// on transport failure. Responses with `ok: false` become `Err`.
    pub fn request(
        &self,
        op: &str,
        mut body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        match self.request_once(op, body.clone(), true) {
            Ok(value) => Ok(value),
            Err(error) if is_transport_error(&error) => {
                body["retried"] = true.into();
                self.request_once(op, body, true)
            }
            Err(error) => Err(error),
        }
    }

    fn request_once(
        &self,
        op: &str,
        mut body: serde_json::Value,
        enforce_ok: bool,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        body["op"] = op.into();
        body["id"] = id.into();
        let encoded = body.to_string();
        if encoded.len() + 1 > MAX_FRAME_BYTES {
            return Err("request exceeds frame byte limit".into());
        }
        let mut stream =
            launch_or_connect(&self.state_dir).map_err(|e| format!("transport: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(30);
        write_frame(
            &mut stream,
            &encoded,
            deadline.min(Instant::now() + Duration::from_secs(5)),
        )
        .map_err(|e| format!("transport: lost daemon connection: {e}"))?;
        let mut reader = BufReader::new(stream.try_clone().map_err(|e| format!("transport: {e}"))?);
        loop {
            let line = read_frame(&mut reader, deadline)
                .map_err(|e| format!("transport: lost daemon connection: {e}"))?;
            let value: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| format!("transport: bad daemon reply: {e}"))?;
            if value["event"].is_string() || value["id"] != id {
                continue;
            }
            if enforce_ok && value["ok"] != true {
                return Err(value["error"]
                    .as_str()
                    .unwrap_or("daemon operation failed")
                    .to_string());
            }
            if let Some(epoch) = value["epoch"].as_u64() {
                *self.epoch.lock() = Some(epoch);
            }
            return Ok(value);
        }
    }

    /// Best-effort request that never starts a daemon (quit paths).
    pub fn request_existing(
        &self,
        op: &str,
        mut body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        body["op"] = op.into();
        body["id"] = id.into();
        let mut stream =
            crate::daemon::connect_authenticated(&self.state_dir, Duration::from_secs(2))
                .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(5);
        write_frame(&mut stream, &body.to_string(), deadline).map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        loop {
            let line = read_frame(&mut reader, deadline).map_err(|e| e.to_string())?;
            let value: serde_json::Value =
                serde_json::from_str(&line).map_err(|e| e.to_string())?;
            if value["event"].is_string() || value["id"] != id {
                continue;
            }
            if value["ok"] != true {
                return Err(value["error"]
                    .as_str()
                    .unwrap_or("daemon operation failed")
                    .to_string());
            }
            return Ok(value);
        }
    }

    fn run(&self) {
        let mut ever_connected = false;
        let mut backoff = Duration::from_millis(500);
        let mut last_connected: Option<bool> = None;
        while !self.shutdown.load(Ordering::Acquire) {
            let mut stream = match launch_or_connect(&self.state_dir) {
                Ok(stream) => stream,
                Err(error) => {
                    if last_connected != Some(false) {
                        self.emit_status(DaemonStatusEvent {
                            connected: false,
                            epoch: self.epoch(),
                            reconnected: false,
                            error: Some(error),
                        });
                        last_connected = Some(false);
                    }
                    std::thread::sleep(backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(5));
                    continue;
                }
            };
            match subscribe_gui(&mut stream) {
                Ok(epoch) => {
                    self.emit_status(DaemonStatusEvent {
                        connected: true,
                        epoch: Some(epoch),
                        reconnected: ever_connected,
                        error: None,
                    });
                    ever_connected = true;
                    last_connected = Some(true);
                    backoff = Duration::from_millis(500);
                    self.forward_events(stream);
                    if last_connected != Some(false) {
                        self.emit_status(DaemonStatusEvent {
                            connected: false,
                            epoch: self.epoch(),
                            reconnected: false,
                            error: Some("daemon connection lost".to_string()),
                        });
                        last_connected = Some(false);
                    }
                }
                Err(error) => {
                    if last_connected != Some(false) {
                        self.emit_status(DaemonStatusEvent {
                            connected: false,
                            epoch: self.epoch(),
                            reconnected: false,
                            error: Some(error),
                        });
                        last_connected = Some(false);
                    }
                    std::thread::sleep(backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(5));
                }
            }
        }
    }

    fn forward_events(&self, stream: TcpStream) {
        let mut reader = BufReader::new(stream);
        loop {
            if self.shutdown.load(Ordering::Acquire) {
                return;
            }
            let line = match read_frame(&mut reader, Instant::now() + Duration::from_secs(60)) {
                Ok(line) => line,
                Err(_) => return,
            };
            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            forward_event(&self.app, &value);
        }
    }
}

fn is_transport_error(error: &str) -> bool {
    error.starts_with("transport: ")
}

fn subscribe_gui(stream: &mut TcpStream) -> Result<u64, String> {
    let id = 1_u64;
    let encoded = serde_json::json!({"op": "gui_subscribe", "id": id}).to_string();
    let deadline = Instant::now() + Duration::from_secs(10);
    write_frame(stream, &encoded, deadline).map_err(|e| format!("subscribe failed: {e}"))?;
    // The daemon pushes state snapshots before our reply; events seen here
    // are dropped because the GUI fetches snapshots per pane after
    // connecting and the daemon replays agent states on every subscription.
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    loop {
        let line =
            read_frame(&mut reader, deadline).map_err(|e| format!("subscribe failed: {e}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&line).map_err(|e| format!("subscribe failed: {e}"))?;
        if value["id"] == id {
            if value["ok"] != true {
                return Err(value["error"]
                    .as_str()
                    .unwrap_or("daemon operation failed")
                    .to_string());
            }
            return value["epoch"]
                .as_u64()
                .ok_or_else(|| "daemon omitted epoch".to_string());
        }
        // Event frames arriving early are dropped here; the daemon replays
        // agent states on every new subscription and the GUI fetches
        // snapshots per pane after connecting.
    }
}

fn forward_event(app: &AppHandle, value: &serde_json::Value) {
    match value["event"].as_str() {
        Some("pty_output") => {
            if let (Some(pane), Some(data), Some(sequence)) = (
                value["pane"].as_u64(),
                value["data"].as_str(),
                value["sequence"].as_u64(),
            ) {
                let _ = app.emit(
                    "pty-output",
                    crate::pty_manager::PtyOutput {
                        id: pane as u32,
                        data: data.to_string(),
                        sequence,
                    },
                );
            }
        }
        Some("pty_exit") => {
            if let (Some(pane), Some(success)) =
                (value["pane"].as_u64(), value["success"].as_bool())
            {
                let _ = app.emit(
                    "pty-exit",
                    crate::pty_manager::PtyExit {
                        id: pane as u32,
                        success,
                        code: value["code"].as_i64().map(|code| code as i32),
                    },
                );
            }
        }
        Some("agent-state-update") => {
            let _ = app.emit(
                "agent-state-update",
                serde_json::json!({
                    "revision": value["revision"],
                    "states": value["states"],
                    "transitions": value["transitions"],
                }),
            );
        }
        _ => {}
    }
}

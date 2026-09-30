//! Opt-in PostHog telemetry: consent-gated analytics and Rust panic capture.
//!
//! Design notes:
//!
//! - The module is a no-op when built without `POSTHOG_KEY`. Release
//!   packaging injects `POSTHOG_KEY`/`POSTHOG_HOST` into the build
//!   environment; community builds omit them and get dead code paths that
//!   always report "unsupported".
//! - Consent lives in `{data_dir}/telemetry.json`, owned by Rust so the
//!   panic hook and client init stay gated before the frontend loads. An
//!   absent or unreadable file means declined.
//! - Analytics event names and property keys are allowlisted here; the
//!   frontend cannot invent events, and property values are validated.
//! - Capture is fire-and-forget; [`shutdown_flush`] runs on app exit.
//!   Panics install our own hook (not the SDK's): it queues the exception
//!   and then flushes synchronously with a timeout, so abort-profile
//!   crashes still deliver instead of dying with a full queue.
//! - Revoking consent stops new capture immediately and restores the
//!   previous panic hook. At most one batch queued before revocation may
//!   still deliver; the worker is never torn down so re-granting cannot
//!   fail on client re-initialization.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::agent_watch::AGENT_TABLE;

/// Build-time project key. Absent (or empty) disables telemetry.
const BUILD_KEY: Option<&str> = option_env!("POSTHOG_KEY");
/// Build-time ingest host; the US cloud default matches the frontend lane.
const BUILD_HOST: &str = match option_env!("POSTHOG_HOST") {
    Some(host) => host,
    None => "https://us.i.posthog.com",
};

pub const TELEMETRY_FILE: &str = "telemetry.json";
pub const FLAGS_FILE: &str = "telemetry-flags.json";
const MAX_DOC_BYTES: u64 = 8192;
const MAX_SCRUB_CHARS: usize = 2000;
const MAX_PROP_CHARS: usize = 128;
const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// Kill-switch flag for the agent-events pipeline. Fails open: when flags
/// cannot be evaluated the feature behaves as if enabled.
pub const FLAG_AGENT_EVENTS: &str = "agent-events-enabled";

pub const EVENT_AGENT_STARTED: &str = "agent_cli_started";
pub const EVENT_AGENT_ENDED: &str = "agent_cli_ended";

const DURATION_BUCKETS: &[&str] = &["under_minute", "under_5m", "under_30m", "over_30m"];
const END_OUTCOMES: &[&str] = &["completed", "stopped", "closed"];

/// Persisted consent state. `Default` is declined with no identity.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Consent {
    pub consented: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distinct_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryStatus {
    pub supported: bool,
    pub consented: bool,
    pub active: bool,
}

/// True when the build carries a project key.
pub fn supported() -> bool {
    BUILD_KEY.is_some_and(|key| !key.is_empty())
}

/// True when capture is currently allowed (key present and consented).
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

pub fn consent_path(dir: &Path) -> PathBuf {
    dir.join(TELEMETRY_FILE)
}

fn flags_path(dir: &Path) -> PathBuf {
    dir.join(FLAGS_FILE)
}

/// Load consent; absent, oversized, or corrupt files all mean declined.
/// Telemetry must never break app startup.
pub fn load_consent(dir: &Path) -> Consent {
    let bytes = match std::fs::read(consent_path(dir)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Consent::default(),
        Err(e) => {
            eprintln!("ubra: telemetry consent unreadable: {e}");
            return Consent::default();
        }
    };
    if bytes.len() as u64 > MAX_DOC_BYTES {
        eprintln!("ubra: telemetry consent file too large; treating as declined");
        return Consent::default();
    }
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save_consent(dir: &Path, consent: &Consent) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(consent).map_err(|e| e.to_string())?;
    std::fs::write(consent_path(dir), bytes).map_err(|e| e.to_string())
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static CLIENT: OnceLock<Mutex<posthog_rs::Client>> = OnceLock::new();
static DISTINCT_ID: Mutex<Option<String>> = Mutex::new(None);

type HookFn = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;
static SAVED_HOOK: Mutex<Option<HookFn>> = Mutex::new(None);

/// Run `f` against the client when capture is allowed. Never blocks
/// indefinitely and never propagates client errors to callers.
fn with_client(f: impl FnOnce(&posthog_rs::Client)) -> bool {
    if !is_active() {
        return false;
    }
    let Some(mutex) = CLIENT.get() else {
        return false;
    };
    let Ok(client) = mutex.try_lock() else {
        return false;
    };
    f(&client);
    true
}

/// Activate telemetry: build the client (once per process), install the
/// panic hook, and cache the distinct id. Idempotent.
pub fn activate(distinct_id: Option<String>) -> bool {
    if !supported() {
        return false;
    }
    if ACTIVE.load(Ordering::SeqCst) {
        return true;
    }
    if CLIENT.get().is_none() {
        let key = BUILD_KEY.unwrap_or_default();
        if key.is_empty() {
            return false;
        }
        let client = posthog_rs::client((key, BUILD_HOST));
        if CLIENT.set(Mutex::new(client)).is_err() {
            return false;
        }
    }
    if let Ok(mut slot) = DISTINCT_ID.lock() {
        *slot = distinct_id.filter(|id| is_valid_distinct_id(id));
    }
    install_hook();
    ACTIVE.store(true, Ordering::SeqCst);
    true
}

/// Deactivate telemetry: no new capture, previous panic hook restored.
/// The background worker is intentionally left running (see module docs).
pub fn deactivate() {
    ACTIVE.store(false, Ordering::SeqCst);
    restore_hook();
}

/// Update the cached frontend distinct id used to correlate Rust
/// exceptions with the web session. Invalid ids are ignored.
pub fn set_distinct_id(id: &str) {
    if !is_valid_distinct_id(id) {
        return;
    }
    if let Ok(mut slot) = DISTINCT_ID.lock() {
        *slot = Some(id.to_string());
    }
}

fn cached_distinct_id() -> Option<String> {
    DISTINCT_ID.lock().ok().and_then(|slot| slot.clone())
}

fn is_valid_distinct_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_PROP_CHARS
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Capture an allowlisted analytics event as an anonymous event (no person
/// profiles). Returns true when the event was queued.
pub fn capture_event(event: &str, properties: &HashMap<String, String>) -> bool {
    if !is_active() {
        return false;
    }
    let schema: &[&str] = match event {
        EVENT_AGENT_STARTED => &["cli"],
        EVENT_AGENT_ENDED => &["cli", "duration", "outcome"],
        _ => return false,
    };
    if !flag_enabled(FLAG_AGENT_EVENTS) {
        return false;
    }
    let mut validated: HashMap<&str, &str> = HashMap::new();
    for key in schema {
        let Some(value) = properties.get(*key) else {
            return false;
        };
        if !is_valid_property(key, value) {
            return false;
        }
        validated.insert(*key, value.as_str());
    }
    if properties.len() != validated.len() {
        return false;
    }
    with_client(|client| {
        let mut posthog_event = posthog_rs::Event::new_anon(event);
        for (key, value) in &validated {
            let _ = posthog_event.insert_prop(*key, *value);
        }
        client.capture(posthog_event);
    })
}

fn is_valid_property(key: &str, value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_PROP_CHARS {
        return false;
    }
    match key {
        "cli" => AGENT_TABLE.iter().any(|(stem, _)| *stem == value),
        "duration" => DURATION_BUCKETS.contains(&value),
        "outcome" => END_OUTCOMES.contains(&value),
        _ => false,
    }
}

#[derive(Debug)]
struct TelemetryError {
    message: String,
}

impl std::fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TelemetryError {}

fn install_hook() {
    let mut saved = match SAVED_HOOK.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if saved.is_some() {
        return;
    }
    *saved = Some(std::panic::take_hook());
    std::panic::set_hook(Box::new(|info| {
        // A panicking hook aborts the process, so contain SDK failures and
        // always chain to the previously installed hook afterwards.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            capture_panic(info);
            flush_with_timeout(FLUSH_TIMEOUT);
        }));
        if let Ok(guard) = SAVED_HOOK.lock() {
            if let Some(previous) = guard.as_ref() {
                previous(info);
            }
        }
    }));
}

fn restore_hook() {
    let previous = match SAVED_HOOK.lock() {
        Ok(mut guard) => guard.take(),
        Err(_) => return,
    };
    if let Some(previous) = previous {
        std::panic::set_hook(previous);
    }
}

fn capture_panic(info: &std::panic::PanicHookInfo) {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload");
    let location = info
        .location()
        .map(|location| {
            // Build paths embed usernames; keep only the file name.
            let file = Path::new(location.file())
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("?");
            format!("{file}:{}", location.line())
        })
        .unwrap_or_else(|| "?".to_string());
    let error = TelemetryError {
        message: scrub(&format!("panic at {location}: {message}")),
    };
    with_client(|client| match cached_distinct_id() {
        Some(id) => {
            let _ = client.capture_exception_with(
                &error,
                posthog_rs::CaptureExceptionOptions::new().distinct_id(id),
            );
        }
        None => {
            let _ = client.capture_exception(&error);
        }
    });
}

/// Flush queued events, giving up after `timeout`. Returns true when the
/// flush completed. Used by the panic hook and clean shutdown.
pub fn flush_with_timeout(timeout: Duration) -> bool {
    run_with_timeout(timeout, || {
        if let Some(mutex) = CLIENT.get() {
            if let Ok(client) = mutex.try_lock() {
                client.flush();
            }
        }
    })
}

fn run_with_timeout<F: FnOnce() + Send + 'static>(timeout: Duration, f: F) -> bool {
    let (sender, receiver) = std::sync::mpsc::channel();
    let spawned = std::thread::spawn(move || {
        f();
        let _ = sender.send(());
    });
    let completed = receiver.recv_timeout(timeout).is_ok();
    // A timed-out worker is abandoned, never joined: joining could block
    // shutdown on the hung network call the timeout exists to escape.
    if completed {
        let _ = spawned.join();
    }
    completed
}

/// Best-effort delivery of queued events during clean shutdown.
pub fn shutdown_flush() {
    if CLIENT.get().is_none() {
        return;
    }
    flush_with_timeout(FLUSH_TIMEOUT);
    with_client(|client| client.shutdown());
}

/// Flags evaluated into the persisted snapshot. Keep this list tight:
/// each entry costs one `$feature_flag_called` exposure event per refresh.
const KNOWN_FLAGS: &[&str] = &[FLAG_AGENT_EVENTS];

/// Evaluate a feature flag. Fails open (true) when telemetry is inactive,
/// the snapshot is missing, or the flag is unknown.
pub fn flag_enabled(name: &str) -> bool {
    if !is_active() {
        return true;
    }
    let guard = match FLAG_SNAPSHOT.lock() {
        Ok(guard) => guard,
        Err(_) => return true,
    };
    guard
        .as_ref()
        .and_then(|snapshot| snapshot.get(name).copied())
        .unwrap_or(true)
}

static FLAG_SNAPSHOT: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

/// Refresh the persisted flag snapshot when online and consented.
/// Failures keep the previous snapshot (or fail-open defaults).
pub fn refresh_flags(dir: &Path) -> bool {
    if !is_active() {
        return false;
    }
    let Some(distinct_id) = cached_distinct_id().filter(|id| !id.is_empty()) else {
        return false;
    };
    let snapshot = with_client_value(|client| {
        client.evaluate_flags(distinct_id, posthog_rs::EvaluateFlagsOptions::default())
    });
    let Some(Ok(snapshot)) = snapshot else {
        return false;
    };
    let mut flags = HashMap::new();
    for key in KNOWN_FLAGS {
        flags.insert((*key).to_string(), snapshot.is_enabled(key));
    }
    if let Ok(mut current) = FLAG_SNAPSHOT.lock() {
        *current = Some(flags.clone());
    }
    persist_flags(dir, &flags)
}

/// Refresh flags off the calling thread; startup and consent grants must
/// never block on a network round-trip.
pub fn spawn_flag_refresh(dir: PathBuf) {
    std::thread::spawn(move || {
        refresh_flags(&dir);
    });
}

fn with_client_value<T>(f: impl FnOnce(&posthog_rs::Client) -> T) -> Option<T> {
    if !is_active() {
        return None;
    }
    let mutex = CLIENT.get()?;
    let client = mutex.try_lock().ok()?;
    Some(f(&client))
}

fn persist_flags(dir: &Path, flags: &HashMap<String, bool>) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let Ok(bytes) = serde_json::to_vec(flags) else {
        return false;
    };
    std::fs::write(flags_path(dir), bytes).is_ok()
}

/// Load the persisted flag snapshot from a previous run (offline support).
pub fn load_flags(dir: &Path) -> bool {
    let bytes = match std::fs::read(flags_path(dir)) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    if bytes.len() as u64 > MAX_DOC_BYTES {
        return false;
    }
    let Ok(flags) = serde_json::from_slice::<HashMap<String, bool>>(&bytes) else {
        return false;
    };
    match FLAG_SNAPSHOT.lock() {
        Ok(mut current) => {
            *current = Some(flags);
            true
        }
        Err(_) => false,
    }
}

/// Scrub free-text telemetry: home directory becomes `~`, secret-shaped
/// assignments and PostHog keys are masked, output is length-capped.
pub fn scrub(input: &str) -> String {
    let mut output = input.to_string();
    if let Some(home) = home_dir() {
        if home.len() > 1 {
            output = output.replace(&home, "~");
        }
    }
    output = mask_assignments(&output);
    output = mask_posthog_keys(&output);
    if output.len() > MAX_SCRUB_CHARS {
        let mut truncated = output.chars().take(MAX_SCRUB_CHARS).collect::<String>();
        truncated.push_str("…[truncated]");
        output = truncated;
    }
    output
}

fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .filter(|home| !home.is_empty())
}

const SECRET_KEYS: &[&str] = &[
    "token",
    "apikey",
    "api_key",
    "api-key",
    "secret",
    "password",
    "passwd",
    "pwd",
    "auth",
    "credential",
    "bearer",
];

fn mask_assignments(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        let rest = &input[index..];
        if let Some((key_len, sep_len)) = match_secret_assignment(rest) {
            output.push_str(&rest[..key_len + sep_len]);
            output.push_str("[redacted]");
            index += key_len + sep_len;
            index += consume_secret_value(&bytes[index..]);
        } else {
            let ch = rest.chars().next().unwrap_or_default();
            output.push(ch);
            index += ch.len_utf8();
        }
    }
    output
}

/// Match `key =` / `key:` prefixes case-insensitively. Returns the key and
/// separator lengths (including surrounding whitespace) on match.
fn match_secret_assignment(rest: &str) -> Option<(usize, usize)> {
    for key in SECRET_KEYS {
        // `get` keeps multibyte input panic-free; a failed boundary is a miss.
        let Some(prefix) = rest.get(..key.len()) else {
            continue;
        };
        if !prefix.eq_ignore_ascii_case(key) {
            continue;
        }
        let after_key = &rest[key.len()..];
        let trimmed = after_key.trim_start();
        let Some(sep) = trimmed.chars().next() else {
            continue;
        };
        if sep != '=' && sep != ':' {
            continue;
        }
        let sep_len = after_key.len() - trimmed.len() + sep.len_utf8();
        let after_sep = &trimmed[sep.len_utf8()..];
        let value_start = after_sep.len() - after_sep.trim_start().len();
        return Some((key.len(), sep_len + value_start));
    }
    None
}

fn consume_secret_value(bytes: &[u8]) -> usize {
    let mut len = 0;
    if bytes.first().is_some_and(|b| *b == b'"' || *b == b'\'') {
        let quote = bytes[0];
        len += 1;
        while len < bytes.len() && bytes[len] != quote {
            len += 1;
        }
        if len < bytes.len() {
            len += 1;
        }
        return len;
    }
    while len < bytes.len() {
        let b = bytes[len];
        if b.is_ascii_whitespace() || b == b',' || b == b';' || b == b'"' || b == b'\'' {
            break;
        }
        len += 1;
    }
    len
}

fn mask_posthog_keys(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if input[index..].starts_with("phc_") {
            output.push_str("[redacted-key]");
            index += 4;
            while index < bytes.len() && bytes[index].is_ascii_alphanumeric() {
                index += 1;
            }
        } else {
            let ch = input[index..].chars().next().unwrap_or_default();
            output.push(ch);
            index += ch.len_utf8();
        }
    }
    output
}

#[tauri::command]
pub fn telemetry_status(app: tauri::AppHandle) -> Result<TelemetryStatus, String> {
    let dir = crate::layout_store::data_dir(&app).map_err(|e| e.to_string())?;
    let consent = load_consent(&dir);
    Ok(TelemetryStatus {
        supported: supported(),
        consented: consent.consented,
        active: is_active(),
    })
}

#[tauri::command]
pub fn telemetry_set_consent(app: tauri::AppHandle, consented: bool) -> Result<bool, String> {
    let dir = crate::layout_store::data_dir(&app).map_err(|e| e.to_string())?;
    let mut consent = load_consent(&dir);
    consent.consented = consented;
    if !consented {
        consent.distinct_id = None;
    }
    save_consent(&dir, &consent)?;
    if consented {
        load_flags(&dir);
        let active = activate(consent.distinct_id);
        if active {
            spawn_flag_refresh(dir);
        }
        Ok(active)
    } else {
        deactivate();
        Ok(false)
    }
}

#[tauri::command]
pub fn telemetry_set_distinct_id(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if !is_valid_distinct_id(&id) {
        return Err("invalid distinct id".to_string());
    }
    let dir = crate::layout_store::data_dir(&app).map_err(|e| e.to_string())?;
    let mut consent = load_consent(&dir);
    consent.distinct_id = Some(id.clone());
    save_consent(&dir, &consent)?;
    set_distinct_id(&id);
    Ok(())
}

#[tauri::command]
pub fn telemetry_capture(
    event: String,
    properties: HashMap<String, String>,
) -> Result<bool, String> {
    Ok(capture_event(&event, &properties))
}

#[tauri::command]
pub fn telemetry_flag(name: String) -> Result<bool, String> {
    Ok(flag_enabled(&name))
}

/// Activate from persisted consent during app startup. Returns active state.
pub fn init_from_disk(dir: &Path) -> bool {
    let consent = load_consent(dir);
    if !consent.consented {
        return false;
    }
    load_flags(dir);
    let active = activate(consent.distinct_id);
    if active {
        spawn_flag_refresh(dir.to_path_buf());
    }
    active
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static TEST_DIRS: AtomicU64 = AtomicU64::new(0);

    fn test_dir(name: &str) -> PathBuf {
        let id = TEST_DIRS.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "ubra-telemetry-test-{}-{}-{}",
            std::process::id(),
            name,
            id
        ));
        std::fs::create_dir_all(&dir).expect("test dir");
        dir
    }

    #[test]
    fn missing_consent_file_means_declined() {
        let dir = test_dir("missing");
        assert_eq!(load_consent(&dir), Consent::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn consent_round_trip() {
        let dir = test_dir("roundtrip");
        let consent = Consent {
            consented: true,
            distinct_id: Some("abc-123_X".to_string()),
        };
        save_consent(&dir, &consent).expect("save");
        assert_eq!(load_consent(&dir), consent);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_consent_file_means_declined() {
        let dir = test_dir("corrupt");
        std::fs::write(consent_path(&dir), b"{not json").expect("write");
        assert_eq!(load_consent(&dir), Consent::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn capture_without_key_is_noop() {
        // Tests build without POSTHOG_KEY, so capture always declines.
        assert!(!supported());
        assert!(!is_active());
        let mut props = HashMap::new();
        props.insert("cli".to_string(), "claude".to_string());
        assert!(!capture_event(EVENT_AGENT_STARTED, &props));
    }

    #[test]
    fn unknown_events_are_rejected() {
        assert!(!capture_event("user did a thing", &HashMap::new()));
    }

    #[test]
    fn property_validation_rejects_unknown_cli() {
        assert!(is_valid_property("cli", "claude"));
        assert!(!is_valid_property("cli", "rm -rf /"));
        assert!(!is_valid_property("cli", ""));
        assert!(is_valid_property("duration", "under_5m"));
        assert!(!is_valid_property("duration", "forever"));
        assert!(is_valid_property("outcome", "completed"));
        assert!(!is_valid_property("cmd", "claude"));
    }

    #[test]
    fn flags_fail_open_without_snapshot() {
        assert!(flag_enabled("anything-at-all"));
    }

    #[test]
    fn flags_round_trip() {
        let dir = test_dir("flags");
        let mut flags = HashMap::new();
        flags.insert(FLAG_AGENT_EVENTS.to_string(), false);
        assert!(persist_flags(&dir, &flags));
        assert!(load_flags(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn distinct_id_validation() {
        assert!(is_valid_distinct_id("abc-123_X"));
        assert!(!is_valid_distinct_id(""));
        assert!(!is_valid_distinct_id("user@example.com"));
        assert!(!is_valid_distinct_id("has space"));
    }

    #[test]
    fn scrub_masks_home_and_secrets() {
        let home = home_dir().unwrap_or_else(|| "/Users/tester".to_string());
        let input = format!("failed reading {home}/proj: token=abc123 password: hunter2");
        let scrubbed = scrub(&input);
        assert!(scrubbed.contains("~/proj"), "{scrubbed}");
        assert!(scrubbed.contains("token=[redacted]"), "{scrubbed}");
        assert!(scrubbed.contains("password: [redacted]"), "{scrubbed}");
        assert!(!scrubbed.contains("abc123"), "{scrubbed}");
    }

    #[test]
    fn scrub_masks_posthog_keys_and_is_case_insensitive() {
        let scrubbed = scrub("key phc_abc123XYZ and TOKEN=\"quoted\"");
        assert!(scrubbed.contains("[redacted-key]"), "{scrubbed}");
        assert!(scrubbed.contains("TOKEN=[redacted]"), "{scrubbed}");
    }

    #[test]
    fn scrub_passes_plain_text_through() {
        assert_eq!(scrub("plain message"), "plain message");
    }

    #[test]
    fn scrub_truncates_long_input() {
        let long = "x".repeat(MAX_SCRUB_CHARS + 100);
        let scrubbed = scrub(&long);
        assert!(scrubbed.ends_with("…[truncated]"), "{scrubbed}");
        assert_eq!(
            scrubbed.chars().count(),
            MAX_SCRUB_CHARS + "…[truncated]".chars().count()
        );
    }

    #[test]
    fn timeout_runner_reports_completion() {
        assert!(run_with_timeout(Duration::from_secs(5), || {}));
        assert!(!run_with_timeout(Duration::from_millis(10), || {
            std::thread::sleep(Duration::from_millis(300));
        }));
    }
}

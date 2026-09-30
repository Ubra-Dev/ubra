//! Per-CLI plan-usage providers (CodexBar-style).
//!
//! Each provider reuses the CLI's own local login: it reads the OAuth token
//! from the credential file the CLI itself maintains and queries the
//! provider's usage endpoint. Tokens never leave this module — only usage
//! numbers, reset times, and source labels are serialized to the frontend.
//! Credential files are read-only; a stale token surfaces `Expired` and the
//! user re-logs-in through the CLI, which owns refresh.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// One usage lane (session / weekly / monthly, whatever the source has).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub label: String,
    /// Percent of the window consumed, 0..=100.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_used: Option<f64>,
    /// Reset as unix epoch seconds, when the source reports one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
}

/// Why a CLI has no usage to show. Messages must never contain secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageStatus {
    Ready,
    NotLoggedIn,
    Expired,
    Offline,
    Unsupported,
    ParseError,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub cli: String,
    /// Human source label, e.g. "Codex OAuth".
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    /// Capture time as unix epoch seconds.
    pub fetched_at: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliUsage {
    pub cli: String,
    pub status: UsageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<UsageSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl CliUsage {
    pub fn unsupported(cli: &str) -> Self {
        Self {
            cli: cli.to_string(),
            status: UsageStatus::Unsupported,
            snapshot: None,
            message: Some("No usage source yet for this CLI.".to_string()),
        }
    }
}

// One CLI's usage source lives in its own module below with a
// `pub async fn fetch(client: &reqwest::Client) -> CliUsage`.
// Adding a provider = adding a module and one line in `fetch_usage`.

/// First non-empty string at any of these JSON pointers. Credential and
/// usage shapes vary by CLI version, so every field is probed, never
/// assumed. Values are returned, never logged.
fn string_at(root: &serde_json::Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|p| {
        root.pointer(p)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    })
}

/// Percent probe with a sanity range: token counts and other magnitudes
/// must never render as a percentage.
fn percent_at(root: &serde_json::Value, pointers: &[&str]) -> Option<f64> {
    pointers.iter().find_map(|p| {
        root.pointer(p)
            .and_then(|v| v.as_f64())
            .filter(|v| (0.0..=100.0).contains(v))
    })
}

/// Epoch seconds from a numeric-or-numeric-string reset field. Values above
/// 1e12 are treated as milliseconds. ISO-8601 strings are not parsed: no
/// provider has shown one yet (see live-probe notes in fixtures/usage).
fn epoch_at(root: &serde_json::Value, pointers: &[&str]) -> Option<i64> {
    let normalize = |v: i64| {
        if v > 1_000_000_000_000 {
            v / 1000
        } else {
            v
        }
    };
    pointers.iter().find_map(|p| match root.pointer(p)? {
        serde_json::Value::Number(n) => n.as_i64().map(normalize),
        serde_json::Value::String(s) => s.parse::<i64>().ok().map(normalize),
        _ => None,
    })
}

mod codex {
    use super::{epoch_at, now_epoch, percent_at, string_at, CliUsage, UsageSnapshot, UsageStatus};

    pub(super) const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

    /// Credential file candidates in probe order: the documented location,
    /// the newer dotted filename seen in the wild, then `$CODEX_HOME`.
    pub(super) fn credential_files() -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        if let Some(home) = super::home_dir() {
            files.push(home.join(".codex/auth.json"));
            files.push(home.join(".codex/.credentials.json"));
        }
        if let Ok(custom) = std::env::var("CODEX_HOME") {
            if !custom.trim().is_empty() {
                files.push(std::path::PathBuf::from(custom).join("auth.json"));
            }
        }
        files
    }

    pub(super) fn access_token(root: &serde_json::Value) -> Option<String> {
        string_at(
            root,
            &[
                "/tokens/access_token",
                "/tokens/accessToken",
                "/access_token",
                "/accessToken",
                "/token",
            ],
        )
    }

    fn parse_window(
        body: &serde_json::Value,
        pointer: &str,
        label: &str,
    ) -> Option<super::UsageWindow> {
        let window = body.pointer(pointer)?;
        if !window.is_object() {
            return None;
        }
        let percent_used = percent_at(
            window,
            &[
                "/used_percent",
                "/percent_used",
                "/utilization",
                "/percent",
                "/used",
            ],
        );
        let resets_at = epoch_at(
            window,
            &[
                "/resets_at",
                "/reset_at",
                "/resetsAt",
                "/resetAt",
                "/reset",
                "/expires_at",
            ],
        );
        if percent_used.is_none() && resets_at.is_none() {
            return None;
        }
        Some(super::UsageWindow {
            label: label.to_string(),
            percent_used,
            resets_at,
        })
    }

    /// `rate_limit.primary_window` → session, `secondary_window` → weekly.
    /// Returns `None` when no recognizable window is present.
    pub(super) fn parse_usage(body: &serde_json::Value) -> Option<UsageSnapshot> {
        let mut windows = Vec::new();
        for (pointer, label) in [
            ("/rate_limit/primary_window", "Session"),
            ("/rate_limit/secondary_window", "Weekly"),
        ] {
            if let Some(window) = parse_window(body, pointer, label) {
                windows.push(window);
            }
        }
        if windows.is_empty() {
            return None;
        }
        let plan = string_at(
            body,
            &["/plan_type", "/plan", "/plan_name", "/subscription/plan"],
        );
        Some(UsageSnapshot {
            cli: "codex".to_string(),
            source: "Codex OAuth".to_string(),
            plan,
            windows,
            fetched_at: now_epoch(),
        })
    }

    pub async fn fetch(client: &reqwest::Client) -> CliUsage {
        let not_logged_in = || CliUsage {
            cli: "codex".to_string(),
            status: UsageStatus::NotLoggedIn,
            snapshot: None,
            message: Some("Log in through the Codex CLI, then refresh.".to_string()),
        };
        let path = credential_files().into_iter().find(|p| p.is_file());
        let Some(path) = path else {
            return not_logged_in();
        };
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return not_logged_in(),
        };
        let creds: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(creds) => creds,
            Err(_) => {
                return CliUsage {
                    cli: "codex".to_string(),
                    status: UsageStatus::ParseError,
                    snapshot: None,
                    message: Some("Couldn't read the Codex credentials file.".to_string()),
                };
            }
        };
        let Some(token) = access_token(&creds) else {
            return not_logged_in();
        };
        let response = match client.get(USAGE_URL).bearer_auth(token).send().await {
            Ok(response) => response,
            Err(_) => {
                return CliUsage {
                    cli: "codex".to_string(),
                    status: UsageStatus::Offline,
                    snapshot: None,
                    message: Some(
                        "Couldn't reach chatgpt.com — check your connection.".to_string(),
                    ),
                };
            }
        };
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return CliUsage {
                cli: "codex".to_string(),
                status: UsageStatus::Expired,
                snapshot: None,
                message: Some(
                    "Codex session expired — log in again through the Codex CLI, then refresh."
                        .to_string(),
                ),
            };
        }
        if !status.is_success() {
            return CliUsage {
                cli: "codex".to_string(),
                status: UsageStatus::ParseError,
                snapshot: None,
                message: Some(format!(
                    "Codex usage request failed (HTTP {}).",
                    status.as_u16()
                )),
            };
        }
        let body: serde_json::Value = match response.json().await {
            Ok(body) => body,
            Err(_) => {
                return CliUsage {
                    cli: "codex".to_string(),
                    status: UsageStatus::ParseError,
                    snapshot: None,
                    message: Some("Codex returned an unreadable usage response.".to_string()),
                };
            }
        };
        match parse_usage(&body) {
            Some(snapshot) => CliUsage {
                cli: "codex".to_string(),
                status: UsageStatus::Ready,
                snapshot: Some(snapshot),
                message: None,
            },
            None => CliUsage {
                cli: "codex".to_string(),
                status: UsageStatus::ParseError,
                snapshot: None,
                message: Some("Codex usage response had no recognizable windows.".to_string()),
            },
        }
    }
}

mod claude {
    use super::{epoch_at, now_epoch, percent_at, string_at, CliUsage, UsageSnapshot, UsageStatus};

    pub(super) const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
    pub(super) const BETA_HEADER: &str = "oauth-2025-04-20";

    pub(super) fn credential_file() -> Option<std::path::PathBuf> {
        super::home_dir().map(|home| home.join(".claude/.credentials.json"))
    }

    pub(super) fn access_token(root: &serde_json::Value) -> Option<String> {
        string_at(
            root,
            &[
                "/claudeAiOauth/accessToken",
                "/claudeAiOauth/access_token",
                "/accessToken",
                "/access_token",
                "/token",
            ],
        )
    }

    /// Stored expiry, if the file carries one. Only a confidently-past
    /// timestamp counts; anything ambiguous falls through to a live call.
    pub(super) fn is_expired(root: &serde_json::Value) -> bool {
        let expires_at = epoch_at(
            root,
            &[
                "/claudeAiOauth/expiresAt",
                "/claudeAiOauth/expires_at",
                "/expiresAt",
                "/expires_at",
            ],
        );
        matches!(expires_at, Some(at) if at < now_epoch())
    }

    fn parse_window(
        body: &serde_json::Value,
        pointer: &str,
        label: &str,
    ) -> Option<super::UsageWindow> {
        let window = body.pointer(pointer)?;
        if !window.is_object() {
            return None;
        }
        let percent_used = percent_at(
            window,
            &[
                "/utilization",
                "/used_percent",
                "/percent_used",
                "/percent",
                "/used",
            ],
        );
        let resets_at = epoch_at(
            window,
            &[
                "/resets_at",
                "/reset_at",
                "/resetsAt",
                "/resetAt",
                "/reset",
                "/expires_at",
            ],
        );
        if percent_used.is_none() && resets_at.is_none() {
            return None;
        }
        Some(super::UsageWindow {
            label: label.to_string(),
            percent_used,
            resets_at,
        })
    }

    /// `five_hour` → session, `seven_day` → weekly, plus model-scoped
    /// weeklies when present. Returns `None` without a recognizable window.
    pub(super) fn parse_usage(body: &serde_json::Value) -> Option<UsageSnapshot> {
        let mut windows = Vec::new();
        for (pointer, label) in [
            ("/five_hour", "Session"),
            ("/seven_day", "Weekly"),
            ("/seven_day_sonnet", "Weekly · Sonnet"),
            ("/seven_day_opus", "Weekly · Opus"),
        ] {
            if let Some(window) = parse_window(body, pointer, label) {
                windows.push(window);
            }
        }
        if windows.is_empty() {
            return None;
        }
        let plan = string_at(
            body,
            &["/plan_type", "/plan", "/plan_name", "/subscription/plan"],
        );
        Some(UsageSnapshot {
            cli: "claude".to_string(),
            source: "Claude OAuth".to_string(),
            plan,
            windows,
            fetched_at: now_epoch(),
        })
    }

    /// True when Claude Code is configured with an API key instead of OAuth:
    /// a set auth env var, or an `apiKey` entry in settings. Only key names
    /// and env presence are inspected — values are never read.
    fn uses_api_key() -> bool {
        for var in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"] {
            if std::env::var(var)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            {
                return true;
            }
        }
        let Some(home) = super::home_dir() else {
            return false;
        };
        let raw = std::fs::read_to_string(home.join(".claude/settings.json"));
        let Ok(raw) = raw else { return false };
        let Ok(settings) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return false;
        };
        settings.get("apiKey").is_some() || settings.get("api_key").is_some()
    }

    pub async fn fetch(client: &reqwest::Client) -> CliUsage {
        let not_logged_in = || CliUsage {
            cli: "claude".to_string(),
            status: UsageStatus::NotLoggedIn,
            snapshot: None,
            message: Some(if uses_api_key() {
                "Claude Code is using API-key auth — usage needs an OAuth login.".to_string()
            } else {
                "Log in through Claude Code, then refresh.".to_string()
            }),
        };
        let expired = || CliUsage {
            cli: "claude".to_string(),
            status: UsageStatus::Expired,
            snapshot: None,
            message: Some(
                "Claude session expired — log in again through Claude Code, then refresh."
                    .to_string(),
            ),
        };
        let path = credential_file().filter(|p| p.is_file());
        let Some(path) = path else {
            return not_logged_in();
        };
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return not_logged_in(),
        };
        let creds: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(creds) => creds,
            Err(_) => {
                return CliUsage {
                    cli: "claude".to_string(),
                    status: UsageStatus::ParseError,
                    snapshot: None,
                    message: Some("Couldn't read the Claude credentials file.".to_string()),
                };
            }
        };
        if is_expired(&creds) {
            return expired();
        }
        let Some(token) = access_token(&creds) else {
            return not_logged_in();
        };
        let response = match client
            .get(USAGE_URL)
            .bearer_auth(token)
            .header("anthropic-beta", BETA_HEADER)
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => {
                return CliUsage {
                    cli: "claude".to_string(),
                    status: UsageStatus::Offline,
                    snapshot: None,
                    message: Some(
                        "Couldn't reach api.anthropic.com — check your connection.".to_string(),
                    ),
                };
            }
        };
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return expired();
        }
        if !status.is_success() {
            return CliUsage {
                cli: "claude".to_string(),
                status: UsageStatus::ParseError,
                snapshot: None,
                message: Some(format!(
                    "Claude usage request failed (HTTP {}).",
                    status.as_u16()
                )),
            };
        }
        let body: serde_json::Value = match response.json().await {
            Ok(body) => body,
            Err(_) => {
                return CliUsage {
                    cli: "claude".to_string(),
                    status: UsageStatus::ParseError,
                    snapshot: None,
                    message: Some("Claude returned an unreadable usage response.".to_string()),
                };
            }
        };
        match parse_usage(&body) {
            Some(snapshot) => CliUsage {
                cli: "claude".to_string(),
                status: UsageStatus::Ready,
                snapshot: Some(snapshot),
                message: None,
            },
            None => CliUsage {
                cli: "claude".to_string(),
                status: UsageStatus::ParseError,
                snapshot: None,
                message: Some("Claude usage response had no recognizable windows.".to_string()),
            },
        }
    }
}

/// Fetch fresh usage for one CLI stem. Unknown stems never touch the network.
pub async fn fetch_usage(cli: &str, client: &reqwest::Client) -> CliUsage {
    match cli {
        "codex" => codex::fetch(client).await,
        "claude" => claude::fetch(client).await,
        _ => CliUsage::unsupported(cli),
    }
}

/// How long a `Ready` snapshot is served from memory.
const CACHE_TTL: Duration = Duration::from_secs(5 * 60);

/// Shared HTTP client plus an in-memory cache. Only `Ready` snapshots are
/// kept: transient failures must never stick.
pub struct UsageCache {
    client: reqwest::Client,
    entries: Mutex<HashMap<String, (Instant, CliUsage)>>,
}

impl UsageCache {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            client,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Cached-or-fresh usage for one CLI stem. `force` skips the cache
    /// for manual refreshes.
    pub async fn usage(&self, cli: &str, force: bool) -> CliUsage {
        if !force {
            if let Some(cached) = self.lookup(cli) {
                return cached;
            }
        }
        let fresh = fetch_usage(cli, &self.client).await;
        if fresh.status == UsageStatus::Ready {
            self.store(cli, fresh.clone());
        }
        fresh
    }

    fn lookup(&self, cli: &str) -> Option<CliUsage> {
        let entries = self.entries.lock().ok()?;
        let (at, cached) = entries.get(cli)?;
        if at.elapsed() < CACHE_TTL {
            Some(cached.clone())
        } else {
            None
        }
    }

    fn store(&self, cli: &str, usage: CliUsage) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(cli.to_string(), (Instant::now(), usage));
        }
    }
}

impl Default for UsageCache {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

pub(crate) fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_cli_is_unsupported_without_network() {
        let usage = CliUsage::unsupported("opencode");
        assert_eq!(usage.status, UsageStatus::Unsupported);
        assert!(usage.snapshot.is_none());
        assert!(usage.message.is_some());
    }

    #[test]
    fn status_serializes_camel_case() {
        assert_eq!(
            serde_json::to_value(UsageStatus::NotLoggedIn).unwrap(),
            serde_json::Value::String("notLoggedIn".to_string())
        );
        assert_eq!(
            serde_json::to_value(UsageStatus::ParseError).unwrap(),
            serde_json::Value::String("parseError".to_string())
        );
    }

    #[test]
    fn codex_parses_session_and_weekly_windows() {
        let body: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/usage/codex_wham.json")).unwrap();
        let snapshot = super::codex::parse_usage(&body).unwrap();
        assert_eq!(snapshot.cli, "codex");
        assert_eq!(snapshot.source, "Codex OAuth");
        assert_eq!(snapshot.plan.as_deref(), Some("plus"));
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].label, "Session");
        assert_eq!(snapshot.windows[0].percent_used, Some(32.0));
        assert_eq!(snapshot.windows[0].resets_at, Some(1791062262));
        assert_eq!(snapshot.windows[1].label, "Weekly");
        assert_eq!(snapshot.windows[1].percent_used, Some(12.5));
    }

    #[test]
    fn codex_omits_null_weekly_window() {
        let body: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/usage/codex_wham_no_weekly.json"))
                .unwrap();
        let snapshot = super::codex::parse_usage(&body).unwrap();
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].label, "Session");
    }

    #[test]
    fn codex_finds_token_in_observed_cred_shape() {
        let creds: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/usage/codex_auth_shape.json")).unwrap();
        assert_eq!(
            super::codex::access_token(&creds).as_deref(),
            Some("<redacted>")
        );
    }

    #[test]
    fn codex_rejects_unrecognizable_body() {
        let body = serde_json::json!({"unexpected": {"shape": true}});
        assert!(super::codex::parse_usage(&body).is_none());
    }

    #[test]
    fn claude_parses_session_weekly_and_scoped_windows() {
        let body: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/usage/claude_oauth.json")).unwrap();
        let snapshot = super::claude::parse_usage(&body).unwrap();
        assert_eq!(snapshot.cli, "claude");
        assert_eq!(snapshot.source, "Claude OAuth");
        let labels: Vec<&str> = snapshot.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["Session", "Weekly", "Weekly · Sonnet"]);
        assert_eq!(snapshot.windows[0].percent_used, Some(45.0));
        assert_eq!(snapshot.windows[0].resets_at, Some(1790760069));
    }

    #[test]
    fn claude_finds_token_in_binary_derived_cred_shape() {
        let creds: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/usage/claude_creds_shape.json"))
                .unwrap();
        assert_eq!(
            super::claude::access_token(&creds).as_deref(),
            Some("<redacted>")
        );
    }

    #[test]
    fn claude_expiry_only_counts_confidently_past_timestamps() {
        let past = serde_json::json!({"claudeAiOauth": {"expiresAt": 1}});
        assert!(super::claude::is_expired(&past));
        let future = serde_json::json!({"claudeAiOauth": {"expiresAt": 9_999_999_999i64}});
        assert!(!super::claude::is_expired(&future));
        let missing = serde_json::json!({"claudeAiOauth": {}});
        assert!(!super::claude::is_expired(&missing));
    }

    #[test]
    fn percent_probe_ignores_non_percent_magnitudes() {
        let window = serde_json::json!({"used": 150000});
        assert_eq!(super::percent_at(&window, &["/used"]), None);
        let window = serde_json::json!({"used_percent": 102.5});
        assert_eq!(super::percent_at(&window, &["/used_percent"]), None);
    }

    #[test]
    fn epoch_probe_normalizes_milliseconds() {
        let window = serde_json::json!({"resets_at": 1791062262000i64});
        assert_eq!(super::epoch_at(&window, &["/resets_at"]), Some(1791062262));
        let window = serde_json::json!({"resetAt": "1791062262"});
        assert_eq!(super::epoch_at(&window, &["/resetAt"]), Some(1791062262));
    }

    #[test]
    fn cache_serves_stored_ready_snapshot() {
        let cache = UsageCache::new();
        let usage = CliUsage {
            cli: "codex".to_string(),
            status: UsageStatus::Ready,
            snapshot: Some(UsageSnapshot {
                cli: "codex".to_string(),
                source: "Codex OAuth".to_string(),
                plan: None,
                windows: vec![],
                fetched_at: now_epoch(),
            }),
            message: None,
        };
        assert!(cache.lookup("codex").is_none());
        cache.store("codex", usage.clone());
        assert_eq!(cache.lookup("codex"), Some(usage));
    }
}

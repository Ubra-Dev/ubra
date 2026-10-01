//! macOS system notifications through `UNUserNotificationCenter`.
//!
//! Why this exists instead of `tauri-plugin-notification`: the plugin's
//! desktop path delivers through the deprecated `NSUserNotificationCenter`,
//! always reports permission as granted, and drops the delivery result on a
//! background task — so the Settings Test button succeeds while nothing
//! appears, and a real macOS denial is invisible. The UN API gives a genuine
//! permission prompt, a queryable authorization state, and delivery errors.
//!
//! UN requires a bundled process (an `.app`): a bare dev binary has no bundle
//! proxy and `currentNotificationCenter` raises. Every UN entry point is
//! gated on [`is_bundled`]; dev builds fall back to the legacy plugin path in
//! `notify_agent`, which reports [`NotifyOutcome::Attempted`].

/// Authorization state surfaced to the Settings UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyPermission {
    Granted,
    Denied,
    Prompt,
    Unknown,
}

/// What a `notify_agent` call actually achieved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum NotifyOutcome {
    /// The system accepted the notification for delivery.
    Delivered,
    /// macOS authorization is denied; the user must allow it in System Settings.
    Denied,
    /// Legacy fire-and-forget path (dev builds, non-macOS): sent, unconfirmed.
    Attempted,
    /// Not delivered. `reason` is a machine-readable code or OS error text.
    Unavailable { reason: String },
}

impl NotifyOutcome {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        NotifyOutcome::Unavailable {
            reason: reason.into(),
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{NotifyOutcome, NotifyPermission};
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
        UNNotificationRequest, UNNotificationSettings, UNUserNotificationCenter,
    };
    use std::path::Path;
    use std::ptr::NonNull;
    use std::time::Duration;

    /// Fixed request id so repeated agent finishes replace each other instead
    /// of stacking in Notification Center.
    const REQUEST_ID: &str = "com.nemoryoliver.ubra.agent";

    /// Pure bundle check over an executable path, so it can be unit-tested
    /// without depending on how the test runner itself was launched.
    pub fn is_bundled_exe(exe: &Path) -> bool {
        // A bundled executable lives at `.../Ubra.app/Contents/MacOS/ubra`.
        exe.components().any(|c| {
            c.as_os_str()
                .to_str()
                .is_some_and(|s| s.ends_with(".app"))
        })
    }

    pub fn is_bundled() -> bool {
        std::env::current_exe().is_ok_and(|exe| is_bundled_exe(&exe))
    }

    /// Map by raw value so future OS additions degrade to `Unknown` instead
    /// of a missed match: 0 not-determined, 1 denied, 2 authorized,
    /// 3 provisional, 4 ephemeral.
    fn map_status(status: UNAuthorizationStatus) -> NotifyPermission {
        match status.0 {
            0 => NotifyPermission::Prompt,
            1 => NotifyPermission::Denied,
            2..=4 => NotifyPermission::Granted,
            _ => NotifyPermission::Unknown,
        }
    }

    fn describe_error(error: *mut NSError) -> String {
        unsafe { error.as_ref() }
            .map(|e| e.localizedDescription().to_string())
            .unwrap_or_else(|| "unknown error".to_string())
    }

    /// Query authorization without prompting. `None` means the query itself
    /// failed (always call [`is_bundled`] first; UN raises without a bundle).
    async fn query_status() -> Option<UNAuthorizationStatus> {
        if !is_bundled() {
            return None;
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            // ObjC handles are not `Send`: initiate the call, then drop them
            // before the first await. The framework retains its own copy of
            // the completion block.
            let center = UNUserNotificationCenter::currentNotificationCenter();
            // Blocks must be `Fn`; the one-shot sender can only fire once.
            let tx = std::sync::Mutex::new(Some(tx));
            let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
                let status = unsafe { settings.as_ref() }.authorizationStatus();
                if let Some(tx) = tx.lock().ok().and_then(|mut tx| tx.take()) {
                    let _ = tx.send(status);
                }
            });
            center.getNotificationSettingsWithCompletionHandler(&block);
        }
        tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .ok()?
            .ok()
    }

    pub async fn permission_state() -> NotifyPermission {
        if !is_bundled() {
            return NotifyPermission::Unknown;
        }
        match query_status().await {
            Some(status) => map_status(status),
            None => {
                eprintln!("ubra: notify: authorization query timed out");
                NotifyPermission::Unknown
            }
        }
    }

    /// Ask macOS for permission. Shows the OS prompt only while undecided;
    /// an existing denial returns `Denied` immediately. Call only from an
    /// explicit user gesture (the Settings Test button), never from an agent
    /// finish. A timeout leaves `Prompt`: the dialog may still be up.
    pub async fn request_permission() -> NotifyPermission {
        if !is_bundled() {
            return NotifyPermission::Unknown;
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let center = UNUserNotificationCenter::currentNotificationCenter();
            let tx = std::sync::Mutex::new(Some(tx));
            let block = RcBlock::new(move |granted: Bool, error: *mut NSError| {
                let failure = (!error.is_null()).then(|| describe_error(error));
                if let Some(tx) = tx.lock().ok().and_then(|mut tx| tx.take()) {
                    let _ = tx.send((bool::from(granted), failure));
                }
            });
            center.requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
                &block,
            );
        }
        match tokio::time::timeout(Duration::from_secs(120), rx).await {
            Ok(Ok((true, _))) => NotifyPermission::Granted,
            Ok(Ok((false, failure))) => {
                if let Some(message) = failure {
                    eprintln!("ubra: notify: authorization request failed: {message}");
                    NotifyPermission::Unknown
                } else {
                    NotifyPermission::Denied
                }
            }
            _ => NotifyPermission::Prompt,
        }
    }

    /// Deliver one notification. Never prompts: an undecided authorization
    /// reports `Unavailable { not-determined }` so the UI can point at the
    /// Settings Test button, which is the one place that asks.
    pub async fn notify(title: &str, body: &str) -> NotifyOutcome {
        if !is_bundled() {
            return NotifyOutcome::unavailable("dev-binary");
        }
        match query_status().await {
            Some(status) if status.0 == 1 => {
                eprintln!("ubra: notify: not delivered, authorization denied");
                return NotifyOutcome::Denied;
            }
            Some(status) if status.0 == 0 => {
                return NotifyOutcome::unavailable("not-determined");
            }
            None => return NotifyOutcome::unavailable("settings-query-timed-out"),
            _ => {}
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let center = UNUserNotificationCenter::currentNotificationCenter();
            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str(title));
            content.setBody(&NSString::from_str(body));
            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &NSString::from_str(REQUEST_ID),
                &content,
                None,
            );
            let tx = std::sync::Mutex::new(Some(tx));
            let block = RcBlock::new(move |error: *mut NSError| {
                let failure = (!error.is_null()).then(|| describe_error(error));
                if let Some(tx) = tx.lock().ok().and_then(|mut tx| tx.take()) {
                    let _ = tx.send(failure);
                }
            });
            center.addNotificationRequest_withCompletionHandler(&request, Some(&block));
        }
        match tokio::time::timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(None)) => {
                eprintln!("ubra: notify: delivered");
                NotifyOutcome::Delivered
            }
            Ok(Ok(Some(message))) => {
                eprintln!("ubra: notify: delivery failed: {message}");
                NotifyOutcome::unavailable(message)
            }
            _ => {
                eprintln!("ubra: notify: delivery timed out");
                NotifyOutcome::unavailable("delivery-timed-out")
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::{is_bundled, notify, permission_state, request_permission};

/// Other platforms keep the plugin's historical answer: desktop permission
/// is not brokered there, and delivery stays fire-and-forget.
#[cfg(not(target_os = "macos"))]
pub async fn permission_state() -> NotifyPermission {
    NotifyPermission::Granted
}

#[cfg(not(target_os = "macos"))]
pub async fn request_permission() -> NotifyPermission {
    NotifyPermission::Granted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn bundled_detection_matches_app_layout() {
        use std::path::Path;
        assert!(super::imp::is_bundled_exe(Path::new(
            "/Applications/Ubra.app/Contents/MacOS/ubra"
        )));
        assert!(!super::imp::is_bundled_exe(Path::new(
            "/Users/test/herdr-desktop/src-tauri/target/debug/ubra"
        )));
        assert!(!super::imp::is_bundled_exe(Path::new("/usr/local/bin/ubra")));
    }

    #[test]
    fn outcome_serializes_with_status_tag() {
        let value = serde_json::to_value(NotifyOutcome::Delivered).unwrap();
        assert_eq!(value, serde_json::json!({"status": "delivered"}));
        let value = serde_json::to_value(NotifyOutcome::Denied).unwrap();
        assert_eq!(value, serde_json::json!({"status": "denied"}));
        let value = serde_json::to_value(NotifyOutcome::Attempted).unwrap();
        assert_eq!(value, serde_json::json!({"status": "attempted"}));
        let value =
            serde_json::to_value(NotifyOutcome::unavailable("not-determined")).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"status": "unavailable", "reason": "not-determined"})
        );
    }

    #[test]
    fn permission_serializes_lowercase() {
        let value = serde_json::to_value(NotifyPermission::Granted).unwrap();
        assert_eq!(value, serde_json::json!("granted"));
        let value = serde_json::to_value(NotifyPermission::Prompt).unwrap();
        assert_eq!(value, serde_json::json!("prompt"));
    }
}

//! Notification sounds, Herdr-style (`[ui.sound]`).
//!
//! There is exactly one chime: an audio file (mp3/ogg/wav/flac) whose path
//! the frontend always sends. When it is missing or unreadable, a
//! synthesized fallback plays so the finish signal is never silent.
//! Playback runs on a dedicated audio thread holding the output stream, so
//! sounds play even when the window is hidden to the tray.

use std::fs::File;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use rodio::source::{SineWave, Source};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};

/// Notification sound kind. `Request` is reserved for Phase 4
/// blocked-detection; only `Done` fires today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoundKind {
    Done,
    Request,
}

/// One fallback chime note: (frequency Hz, length).
pub fn chime_notes(kind: SoundKind) -> Vec<(f32, Duration)> {
    let ms = |n: u64| Duration::from_millis(n);
    match kind {
        SoundKind::Request => vec![(880.0, ms(110)), (659.25, ms(110)), (880.0, ms(240))],
        SoundKind::Done => vec![(659.25, ms(120)), (880.0, ms(220))],
    }
}

fn append_chime(player: &Player, kind: SoundKind) {
    for (freq, len) in chime_notes(kind) {
        player.append(SineWave::new(freq).take_duration(len).amplify(0.2));
    }
}

struct PlayMsg {
    kind: SoundKind,
    file: Option<PathBuf>,
}

/// Expand a leading `~/` against `$HOME`; anything else passes through.
fn expand_tilde(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(raw)
}

/// Resolve the chime path: `None`/blank means "synthesized fallback".
pub fn resolve_override(file: Option<&str>) -> Option<PathBuf> {
    let raw = file?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(expand_tilde(raw))
}

fn play_on(player: &Player, msg: &PlayMsg) {
    if let Some(path) = msg.file.as_ref() {
        match File::open(path).map(Decoder::try_from) {
            Ok(Ok(source)) => {
                player.append(source);
                return;
            }
            _ => {
                eprintln!("ubra: sound file unreadable, using fallback chime: {path:?}");
            }
        }
    }
    append_chime(player, msg.kind);
}

fn audio_loop(rx: mpsc::Receiver<PlayMsg>) {
    let mut sink: Option<MixerDeviceSink> = None;
    let mut player: Option<Player> = None;
    for msg in rx {
        // (Re)open lazily per message so a missing device at startup doesn't
        // mute the app forever.
        if player.is_none() {
            match DeviceSinkBuilder::open_default_sink() {
                Ok(opened) => {
                    player = Some(Player::connect_new(opened.mixer()));
                    sink = Some(opened);
                }
                Err(e) => {
                    eprintln!("ubra: no audio output, dropping notification sound: {e}");
                    continue;
                }
            }
        }
        if let Some(player) = player.as_ref() {
            play_on(player, &msg);
        }
        let _ = &sink;
    }
}

static SENDER: OnceLock<Sender<PlayMsg>> = OnceLock::new();

fn sender() -> &'static Sender<PlayMsg> {
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("ubra-sound".to_string())
            .spawn(move || audio_loop(rx))
            .expect("failed to spawn audio thread");
        tx
    })
}

/// Queue a notification sound. Best-effort: failures are logged on the audio
/// thread, never surfaced to the notify flow.
pub fn play(kind: SoundKind, file: Option<&str>) -> anyhow::Result<()> {
    sender()
        .send(PlayMsg {
            kind,
            file: resolve_override(file),
        })
        .map_err(|e| anyhow::anyhow!("audio thread gone: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{chime_notes, resolve_override, SoundKind};

    #[test]
    fn chimes_differ_per_kind() {
        let done = chime_notes(SoundKind::Done);
        let request = chime_notes(SoundKind::Request);
        assert!(!done.is_empty() && !request.is_empty());
        assert_ne!(done, request);
        for (_, len) in done.iter().chain(request.iter()) {
            assert!(!len.is_zero());
        }
    }

    #[test]
    fn blank_override_means_fallback() {
        assert_eq!(resolve_override(None), None);
        assert_eq!(resolve_override(Some("")), None);
        assert_eq!(resolve_override(Some("   ")), None);
        assert!(resolve_override(Some("/tmp/x.mp3")).is_some());
    }
}

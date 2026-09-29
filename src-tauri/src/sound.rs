//! Notification sounds, Herdr-style (`[ui.sound]`).
//!
//! The default chimes are synthesized at runtime (no audio assets to bundle);
//! each kind can be overridden with a user audio file (mp3/ogg/wav/flac).
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

/// Built-in chime style for the Done signal. Request keeps its own urgent
/// triple regardless of style (it only fires once blocked-detection lands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChimeStyle {
    #[default]
    Default,
    Bright,
    Soft,
    Pop,
}

/// One chime note: (frequency Hz, length).
pub fn chime_notes(kind: SoundKind) -> Vec<(f32, Duration)> {
    chime_notes_styled(kind, ChimeStyle::Default)
}

pub fn chime_notes_styled(kind: SoundKind, style: ChimeStyle) -> Vec<(f32, Duration)> {
    let ms = |n: u64| Duration::from_millis(n);
    match kind {
        SoundKind::Request => vec![(880.0, ms(110)), (659.25, ms(110)), (880.0, ms(240))],
        SoundKind::Done => match style {
            // Default: rising two-note chime.
            ChimeStyle::Default => vec![(659.25, ms(120)), (880.0, ms(220))],
            ChimeStyle::Bright => vec![(880.0, ms(90)), (1174.66, ms(90)), (1567.98, ms(200))],
            ChimeStyle::Soft => vec![(523.25, ms(160)), (392.0, ms(260))],
            ChimeStyle::Pop => vec![(1200.0, ms(70)), (800.0, ms(100))],
        },
    }
}

fn append_chime(player: &Player, kind: SoundKind, style: ChimeStyle) {
    for (freq, len) in chime_notes_styled(kind, style) {
        player.append(SineWave::new(freq).take_duration(len).amplify(0.2));
    }
}

struct PlayMsg {
    kind: SoundKind,
    style: ChimeStyle,
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

/// Resolve a user override path: `None`/blank means "synthesized default".
pub fn resolve_override(file: Option<&str>) -> Option<PathBuf> {
    let raw = file?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(expand_tilde(raw))
}

/// True when the path opens and decodes as audio (header sniff, no playback).
pub fn file_decodes(path: &str) -> bool {
    let path = expand_tilde(path.trim());
    File::open(&path)
        .ok()
        .and_then(|f| Decoder::try_from(f).ok())
        .is_some()
}

fn play_on(player: &Player, msg: &PlayMsg) {
    if let Some(path) = msg.file.as_ref() {
        match File::open(path).map(Decoder::try_from) {
            Ok(Ok(source)) => {
                player.append(source);
                return;
            }
            _ => {
                eprintln!("ubra: sound file unreadable, using default chime: {path:?}");
            }
        }
    }
    append_chime(player, msg.kind, msg.style);
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
pub fn play(kind: SoundKind, style: ChimeStyle, file: Option<&str>) -> anyhow::Result<()> {
    sender()
        .send(PlayMsg {
            kind,
            style,
            file: resolve_override(file),
        })
        .map_err(|e| anyhow::anyhow!("audio thread gone: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{
        chime_notes, chime_notes_styled, file_decodes, resolve_override, ChimeStyle, SoundKind,
    };

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
    fn chime_styles_differ_for_done() {
        let styles = [
            ChimeStyle::Default,
            ChimeStyle::Bright,
            ChimeStyle::Soft,
            ChimeStyle::Pop,
        ];
        let seqs: Vec<_> = styles
            .iter()
            .map(|s| chime_notes_styled(SoundKind::Done, *s))
            .collect();
        for seq in &seqs {
            assert!(!seq.is_empty());
            for (_, len) in seq {
                assert!(!len.is_zero());
            }
        }
        for (i, a) in seqs.iter().enumerate() {
            for b in &seqs[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn request_ignores_style() {
        let expected = chime_notes(SoundKind::Request);
        for style in [
            ChimeStyle::Default,
            ChimeStyle::Bright,
            ChimeStyle::Soft,
            ChimeStyle::Pop,
        ] {
            assert_eq!(chime_notes_styled(SoundKind::Request, style), expected);
        }
    }

    #[test]
    fn blank_override_means_default() {
        assert_eq!(resolve_override(None), None);
        assert_eq!(resolve_override(Some("")), None);
        assert_eq!(resolve_override(Some("   ")), None);
        assert!(resolve_override(Some("/tmp/x.mp3")).is_some());
    }

    #[test]
    fn missing_file_does_not_decode() {
        assert!(!file_decodes("/nonexistent-ubra-sound-12345.mp3"));
        assert!(!file_decodes(""));
    }
}

//! Serializable terminal state and chunk-safe headless query handling.
use crate::pty_manager::PtySnapshot;

pub(crate) struct ScreenState {
    pub parser: vt100::Parser,
    pub sequence: u64,
    control: ControlTail,
}

impl ScreenState {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows, cols, 200),
            sequence: 0,
            control: ControlTail::default(),
        }
    }

    pub fn process(&mut self, text: &str) {
        self.parser.process(text.as_bytes());
        for ch in text.chars() {
            if self.control.push(ch) {
                self.control.pending.clear();
            }
        }
        self.sequence += 1;
    }

    pub fn snapshot(&self) -> PtySnapshot {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let mut data = String::from("\x1bc");
        // History first, as plain text: replaying the scrollback before the
        // visible screen lets the fresh renderer accumulate it in its own
        // scroll buffer (the state dump below only homes and erases the
        // visible screen, `\x1b[H\x1b[J`, which preserves scrollback).
        // Empty when there is no history, keeping the byte stream identical
        // to snapshots without scrollback.
        data.push_str(&scrollback_segment(screen));
        if screen.alternate_screen() {
            // Copy only on attachment: switching a cloned screen preserves the
            // primary buffer without mutating the live emulator/parser state.
            let mut primary = vt100::Parser::new(rows, cols, 0);
            *primary.screen_mut() = screen.clone();
            primary.process(b"\x1b[?1049l");
            data.push_str(&String::from_utf8_lossy(
                &primary.screen().state_formatted(),
            ));
            data.push_str("\x1b[?1049h");
        }
        data.push_str(&String::from_utf8_lossy(&screen.state_formatted()));
        // A watermark can bisect an escape sequence. Recreate the unfinished
        // parser input after painting, so the first newer chunk completes it.
        data.push_str(&self.control.pending);
        PtySnapshot {
            data,
            sequence: self.sequence,
            cols,
            rows,
        }
    }
}

/// Plain-text scrollback (oldest row first) for snapshot replay.
/// Formatting is intentionally dropped: history stays readable without
/// risking escape-sequence hazards in the replay stream.
fn scrollback_segment(screen: &vt100::Screen) -> String {
    if screen.alternate_screen() {
        // Alternate grids carry no history; read the primary grid the same
        // way the snapshot repaints it below.
        let (rows, cols) = screen.size();
        let mut primary = vt100::Parser::new(rows, cols, 0);
        *primary.screen_mut() = screen.clone();
        primary.process(b"\x1b[?1049l");
        return scrollback_text(primary.screen());
    }
    scrollback_text(screen)
}

/// Page the view offset through the history buffer: vt100 exposes only
/// visible rows, so the top visible row at each offset walks the scrollback
/// from oldest (highest offset) to newest (offset 1).
///
/// The segment ends on a fresh row plus `rows - 1` blank scrolls, which push
/// every replayed row off the visible screen and into the fresh renderer's
/// scroll buffer *before* the state dump below erases the screen. Without
/// that, short histories would sit on visible rows and be erased instead of
/// retained. Wrapped rows rejoin (no newline), so long lines re-wrap at the
/// renderer's width exactly as before.
fn scrollback_text(screen: &vt100::Screen) -> String {
    let (rows, cols) = screen.size();
    let mut view = screen.clone();
    view.set_scrollback(usize::MAX);
    let total = view.scrollback();
    if total == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut wrapping = false;
    for offset in (1..=total).rev() {
        view.set_scrollback(offset);
        let row = view.rows(0, cols).next().unwrap_or_default();
        if row.is_empty() && wrapping {
            out.push('\n');
        } else {
            out.push_str(&row);
        }
        let wrapped = view.row_wrapped(0);
        if !wrapped {
            out.push_str("\r\n");
        }
        wrapping = wrapped;
    }
    if wrapping {
        out.push_str("\r\n");
    }
    for _ in 1..rows {
        out.push_str("\r\n");
    }
    out
}

#[derive(Default)]
struct ControlTail {
    pending: String,
    state: u8,
}

impl ControlTail {
    // 0 ground, 1 ESC, 2 CSI, 3 string, 4 string ESC, 5 ESC intermediate.
    fn push(&mut self, ch: char) -> bool {
        if self.state == 0 {
            if ch == '\x1b' {
                self.pending.clear();
                self.pending.push(ch);
                self.state = 1;
            }
            return false;
        }
        self.pending.push(ch);
        if ch == '\x18' || ch == '\x1a' {
            self.state = 0;
        } else {
            self.state = match self.state {
                1 => match ch {
                    '[' => 2,
                    ']' | 'P' | '^' | '_' | 'X' => 3,
                    '\x20'..='\x2f' => 5,
                    '\x1b' => 1,
                    _ => 0,
                },
                2 => {
                    if ('\x40'..='\x7e').contains(&ch) {
                        0
                    } else if ch == '\x1b' {
                        1
                    } else {
                        2
                    }
                }
                3 => {
                    if ch == '\x07' {
                        0
                    } else if ch == '\x1b' {
                        4
                    } else {
                        3
                    }
                }
                4 => {
                    if ch == '\\' {
                        0
                    } else if ch == '\x1b' {
                        4
                    } else {
                        3
                    }
                }
                5 => {
                    if ('\x30'..='\x7e').contains(&ch) {
                        0
                    } else {
                        5
                    }
                }
                _ => 0,
            };
        }
        if self.state == 1 && ch == '\x1b' {
            self.pending.clear();
            self.pending.push(ch);
        }
        self.state == 0
    }
}

#[derive(Default)]
pub(crate) struct TerminalQueries {
    control: ControlTail,
}

impl TerminalQueries {
    pub fn respond(&mut self, text: &str, screen: &vt100::Screen, mut reply: impl FnMut(&str)) {
        for ch in text.chars() {
            if self.control.push(ch) {
                match self.control.pending.as_str() {
                    "\x1b[6n" | "\x1b[?6n" => {
                        let (row, col) = screen.cursor_position();
                        let private = if self.control.pending == "\x1b[?6n" {
                            "?"
                        } else {
                            ""
                        };
                        reply(&format!("\x1b[{private}{};{}R", row + 1, col + 1));
                    }
                    "\x1b[5n" => reply("\x1b[0n"),
                    "\x1b[c" | "\x1b[0c" => reply("\x1b[?1;2c"),
                    _ => {}
                }
                self.control.pending.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_restores_primary_alternate_cursor_modes_and_partial_csi() {
        let mut state = ScreenState::new(12, 30);
        state.process("primary\x1b[?1049h\x1b[?1h\x1b[?2004h\x1b[4;5Halt\x1b[2;");
        let snap = state.snapshot();
        assert_eq!(snap.sequence, 1);
        let mut restored = vt100::Parser::new(12, 30, 0);
        restored.process(snap.data.as_bytes());
        let later = "3H!";
        state.process(later);
        restored.process(later.as_bytes());
        assert_eq!(
            restored.screen().contents(),
            state.parser.screen().contents()
        );
        assert_eq!(
            restored.screen().cursor_position(),
            state.parser.screen().cursor_position()
        );
        assert!(restored.screen().alternate_screen());
        assert!(restored.screen().application_cursor());
        assert!(restored.screen().bracketed_paste());
        restored.process(b"\x1b[?1049l");
        state.process("\x1b[?1049l");
        assert_eq!(
            restored.screen().contents(),
            state.parser.screen().contents()
        );
        assert!(restored.screen().contents().contains("primary"));
    }
    /// Rows currently retained in the history buffer (oldest first).
    fn history_rows(screen: &vt100::Screen) -> Vec<String> {
        let (_, cols) = screen.size();
        let mut view = screen.clone();
        view.set_scrollback(usize::MAX);
        let total = view.scrollback();
        let mut rows = Vec::with_capacity(total);
        for offset in (1..=total).rev() {
            view.set_scrollback(offset);
            rows.push(view.rows(0, cols).next().unwrap_or_default());
        }
        rows
    }

    #[test]
    fn snapshot_replays_scrollback_before_visible_screen() {
        let mut state = ScreenState::new(10, 80);
        let mut script = String::new();
        for i in 1..=30 {
            script.push_str(&format!("line-{i}\r\n"));
        }
        state.process(&script);
        let live = state.parser.screen();
        let live_history = history_rows(live);
        assert!(live_history.len() > 10, "script must overflow the screen");
        assert_eq!(live_history[0], "line-1");

        let snap = state.snapshot();
        // History precedes the state dump in stream order.
        let dump_at = snap
            .data
            .find("\x1b[H\x1b[J")
            .expect("state dump must home and erase");
        let history = &snap.data["\x1bc".len()..dump_at];
        for row in &live_history {
            assert!(
                history.contains(row.as_str()),
                "history segment must carry {row:?}"
            );
        }

        // A fresh renderer replays the history into its own scroll buffer
        // before the visible screen is repainted over it.
        let mut restored = vt100::Parser::new(10, 80, 500);
        restored.process(snap.data.as_bytes());
        assert_eq!(
            restored.screen().contents(),
            live.contents(),
            "visible screen must repaint exactly"
        );
        // The trailing blank scrolls push every history row off the
        // visible screen before the repaint, so the round trip is exact.
        assert_eq!(history_rows(restored.screen()), live_history);
    }

    #[test]
    fn snapshot_preserves_primary_scrollback_behind_alternate_screen() {
        let mut state = ScreenState::new(10, 80);
        let mut script = String::new();
        for i in 1..=15 {
            script.push_str(&format!("line-{i}\r\n"));
        }
        script.push_str("\x1b[?1049h_alt-view");
        state.process(&script);
        assert!(state.parser.screen().alternate_screen());

        let snap = state.snapshot();
        assert!(
            snap.data.contains("line-1"),
            "primary history must survive behind alt screen"
        );
        let mut restored = vt100::Parser::new(10, 80, 500);
        restored.process(snap.data.as_bytes());
        assert!(restored.screen().alternate_screen());
        assert!(restored.screen().contents().contains("alt-view"));
        restored.process(b"\x1b[?1049l");
        state.process("\x1b[?1049l");
        assert_eq!(
            restored.screen().contents(),
            state.parser.screen().contents(),
            "primary screen must match after leaving alt"
        );
        assert!(
            history_rows(restored.screen()).contains(&"line-1".to_string()),
            "primary history must survive the round trip"
        );
    }

    #[test]
    fn query_responder_handles_split_and_repeated_queries_without_osc_false_positive() {
        let parser = vt100::Parser::new(24, 80, 0);
        let mut queries = TerminalQueries::default();
        let mut replies = Vec::new();
        for chunk in ["\x1b[", "6n\x1b[6n", "\x1b]title \x1b[6n\x07", "\x1b[5n"] {
            queries.respond(chunk, parser.screen(), |s| replies.push(s.to_string()));
        }
        assert_eq!(replies, ["\x1b[1;1R", "\x1b[1;1R", "\x1b[0n"]);
    }
}

#[cfg(test)]
mod geometry_tests {
    #[test]
    fn wide_glyph_at_minimum_geometry_remains_renderable() {
        let mut state = super::ScreenState::new(1, 2);
        state.process("界");
        let mut restored = vt100::Parser::new(1, 2, 0);
        restored.process(state.snapshot().data.as_bytes());
        assert_eq!(restored.screen().contents(), "界");
    }
}

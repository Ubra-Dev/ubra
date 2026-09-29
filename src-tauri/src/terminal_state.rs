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

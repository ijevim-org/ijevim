use crate::editor::Editor;
use crate::types::Position;

/// Compute the point after inserting `text` at `start`.
///
/// Emacs point is a buffer position, so a newline moves it to column 0 of the
/// next line rather than advancing a column count.
fn point_after(start: Position, text: &str) -> Position {
    let mut pos = start;
    for ch in text.chars() {
        if ch == '\n' {
            pos.line += 1;
            pos.col = 0;
        } else {
            pos.col += 1;
        }
    }
    pos
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

mod case;
mod editing;
mod kill;
mod region;
mod yank;

impl Editor {
    pub fn clear_screen(&mut self) {
        let _ = self.terminal.clear_screen();
        self.needs_render = true;
    }

    pub fn abort(&mut self) {
        self.engine.state.command_buffer.clear();
        self.engine.state.command_cursor_pos = 0;
        if self.engine.state.mode == crate::types::Mode::Command {
            self.engine.state.mode = crate::types::Mode::Normal;
            self.needs_render = true;
        }
    }

    /// Push kill text onto the kill ring, appending when the previous Emacs
    /// command was also a kill.
    fn kill_to_ring(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.engine.operator_state.last_command_was_kill {
            self.engine.kill_ring.push_append(text);
        } else {
            self.engine.kill_ring.push(text);
        }
        self.engine.operator_state.last_command_was_kill = true;

        // Keep the unnamed register in sync with the whole logical kill.
        if let Some(top) = self.engine.kill_ring.yank() {
            let top = top.to_string();
            self.engine.register.set('"', &top);
        }
    }

    /// Span from point to the end of the next word (Emacs `M-d`, `M-u`, ...).
    ///
    /// Point inside a word transforms only the remainder; point on whitespace
    /// skips to the next word first. Returns the start and exclusive end.
    fn forward_word_bounds(&self, start: Position) -> Option<(Position, Position)> {
        let last = self.engine.buffer.content_line_count().max(1);
        let mut line = start.line.clamp(1, last);
        let mut col = start.col;
        let mut chars = self.engine.buffer.line_chars(line);
        let mut len = self.engine.buffer.line_content_len(line);

        let on_word_at_point = col < len && is_word_char(chars[col]);
        if !on_word_at_point {
            loop {
                while col < len && chars[col].is_whitespace() {
                    col += 1;
                }
                if col < len {
                    break;
                }
                if line >= last {
                    return None;
                }
                line += 1;
                chars = self.engine.buffer.line_chars(line);
                len = self.engine.buffer.line_content_len(line);
                col = 0;
            }
        }

        let word_start = Position { line, col };
        if col < len {
            if is_word_char(chars[col]) {
                while col < len && is_word_char(chars[col]) {
                    col += 1;
                }
            } else {
                // Punctuation: treat the non-space run as the word.
                while col < len && !chars[col].is_whitespace() {
                    col += 1;
                }
            }
        }

        let start = if on_word_at_point { start } else { word_start };
        let end = Position { line, col };
        if start == end {
            None
        } else {
            Some((start, end))
        }
    }
}

use crate::editor::Editor;
use crate::types::Position;
use crossterm::event::KeyCode;

impl Editor {
    /// Resolve `i`/`a` text objects into a visual selection.
    pub(super) fn visual_text_object(&mut self, inner: bool, key: KeyCode) {
        let KeyCode::Char(c) = key else {
            return;
        };
        let cursor = self.engine.state.cursor;
        let line = cursor.line;
        let col = cursor.col;
        let mut range: Option<(Position, Position)> = None;

        match c {
            'w' | 'W' => {
                let (word, start, end) = if c == 'W' {
                    self.engine.buffer.get_word_range_upper(line, col)
                } else {
                    self.engine.buffer.get_word_range(line, col)
                };
                if !word.is_empty() {
                    let chars = self.engine.buffer.line_chars(line);
                    let (mut s, mut e) = (start, end);
                    if !inner {
                        while s > 0 && chars[s - 1].is_whitespace() {
                            s -= 1;
                        }
                        while e < chars.len() && chars[e].is_whitespace() {
                            e += 1;
                        }
                    }
                    range = Some((
                        Position { line, col: s },
                        Position {
                            line,
                            col: e.saturating_sub(1),
                        },
                    ));
                }
            }
            '"' | '\'' => {
                let chars = self.engine.buffer.line_chars(line);
                let mut open = None;
                for i in (0..col).rev() {
                    if chars.get(i) == Some(&c) {
                        open = Some(i);
                        break;
                    }
                }
                let mut close = None;
                if let Some(i) = chars.iter().skip(col + 1).position(|&ch| ch == c) {
                    close = Some(col + 1 + i);
                }
                if let (Some(o), Some(cl)) = (open, close) {
                    let (s, e) = if inner { (o + 1, cl) } else { (o, cl + 1) };
                    if e > s {
                        range = Some((Position { line, col: s }, Position { line, col: e - 1 }));
                    }
                }
            }
            '(' | ')' | 'b' | '[' | ']' | '{' | '}' | 'B' | '<' | '>' => {
                let (open, close) = match c {
                    '(' | ')' | 'b' => ('(', ')'),
                    '[' | ']' => ('[', ']'),
                    '{' | '}' | 'B' => ('{', '}'),
                    _ => ('<', '>'),
                };
                if let Some((s, e)) = self.find_enclosing_pair(open, close) {
                    let (s, e) = if inner {
                        (
                            Position {
                                line: s.0,
                                col: s.1 + 1,
                            },
                            Position {
                                line: e.0,
                                col: e.1.saturating_sub(1),
                            },
                        )
                    } else {
                        (
                            Position {
                                line: s.0,
                                col: s.1,
                            },
                            Position {
                                line: e.0,
                                col: e.1,
                            },
                        )
                    };
                    if (e.line, e.col) >= (s.line, s.col) {
                        range = Some((s, e));
                    }
                }
            }
            'p' => {
                let mut s = line;
                while s > 1 && !self.engine.buffer.get_line(s - 1).trim().is_empty() {
                    s -= 1;
                }
                let mut e = line;
                let last = self.engine.buffer.content_line_count();
                while e < last && !self.engine.buffer.get_line(e + 1).trim().is_empty() {
                    e += 1;
                }
                let start_col = if inner {
                    self.first_non_blank_col(s)
                } else {
                    0
                };
                range = Some((
                    Position {
                        line: s,
                        col: start_col,
                    },
                    Position {
                        line: e,
                        col: self.line_content_len(e).saturating_sub(1),
                    },
                ));
            }
            _ => {}
        }

        if let Some((start, end)) = range {
            self.select_range(start, end);
        }
    }

    pub(super) fn find_enclosing_pair(
        &self,
        open: char,
        close: char,
    ) -> Option<((usize, usize), (usize, usize))> {
        let cursor = self.engine.state.cursor;
        let mut open_pos = None;
        'outer: for line in (1..=cursor.line).rev() {
            let chars = self.engine.buffer.line_chars(line);
            let start_col = if line == cursor.line {
                cursor.col.min(chars.len())
            } else {
                chars.len()
            };
            for i in (0..start_col).rev() {
                if chars.get(i) == Some(&open) {
                    open_pos = Some((line, i));
                    break 'outer;
                }
                if chars.get(i) == Some(&close) {
                    // A closer before the cursor on the same line means the
                    // pair opened earlier; keep scanning left for the opener.
                }
            }
        }
        let (open_line, open_col) = open_pos?;

        let mut depth = 0usize;
        for line in open_line..=self.engine.buffer.content_line_count() {
            let chars = self.engine.buffer.line_chars(line);
            let start = if line == open_line { open_col } else { 0 };
            for (i, &ch) in chars.iter().enumerate().skip(start) {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(((open_line, open_col), (line, i)));
                    }
                }
            }
        }
        None
    }
}

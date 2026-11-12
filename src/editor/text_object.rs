use crate::editor::Editor;
use crate::undo::{Edit, EditType};
use crossterm::event::KeyCode;

impl Editor {
    /// Indent or unindent `first..=last`, one undo group.
    ///
    /// Vim moves the cursor to the first non-blank afterwards, which is what
    /// `>>` on a tab-indented line relies on.
    pub(crate) fn indent_lines(&mut self, first: usize, last: usize, indent: bool) {
        let first = first.max(1);
        let last = last.max(first).min(self.engine.buffer.content_line_count());
        let sw = self.engine.state.shiftwidth.max(1);
        let indent_str = if self.engine.state.expandtab {
            " ".repeat(sw)
        } else {
            "\t".to_string()
        };

        let mut edits = Vec::new();
        for line in first..=last {
            let content = self.engine.buffer.get_line(line);
            let mod_count = self.engine.buffer.modification_count();

            if indent {
                self.engine.buffer.insert(line, 0, &indent_str);
                edits.push(Edit {
                    edit_type: EditType::Insert {
                        line,
                        col: 0,
                        text: indent_str.clone(),
                    },
                    cursor_before: self.engine.state.cursor,
                    cursor_after: self.engine.state.cursor,
                    modification_count: mod_count,
                });
                continue;
            }

            // Unindent one shiftwidth: a leading tab, or up to `sw` spaces.
            let removed = if content.starts_with('\t') {
                "\t".to_string()
            } else {
                let count = content.chars().take(sw).take_while(|c| *c == ' ').count();
                " ".repeat(count)
            };
            if removed.is_empty() {
                continue;
            }
            for _ in 0..removed.chars().count() {
                self.engine.buffer.delete(line, 0);
            }
            edits.push(Edit {
                edit_type: EditType::Delete {
                    line,
                    col: 0,
                    text: removed,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
        }

        self.push_undo_group(edits);
        let line = self.engine.state.cursor.line;
        self.engine.state.cursor.col = self.first_non_blank_col(line);
        self.engine.dot_last_action = Some(crate::types::DotAction::Indent { unindent: !indent });
        self.needs_render = true;
    }

    pub(crate) async fn handle_text_object_change(
        &mut self,
        key: KeyCode,
        register: char,
        inner: bool,
    ) {
        if !matches!(key, KeyCode::Char(_)) {
            return;
        }

        // The delete path already selects the right range for every text
        // object (word/WORD/quotes/brackets) and records the undo entry, so
        // change is "delete the object, then insert".
        let marker = self.undo_marker();
        self.handle_text_object(key, register, inner).await;
        self.defer_undo_since(marker);

        let prev_mode = self.engine.state.mode;
        self.engine.state.mode = crate::types::Mode::Insert;
        self.engine.insert_state.accum.clear();
        self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
        self.engine
            .plugin_manager
            .emit(crate::types::PluginEvent::ModeChange {
                from: prev_mode,
                to: crate::types::Mode::Insert,
            });
        self.on_buffer_modified();
    }

    #[allow(clippy::single_match)]
    pub(crate) async fn handle_text_object(&mut self, key: KeyCode, register: char, inner: bool) {
        match key {
            KeyCode::Char('w') => {
                if inner {
                    let (word, start, _) = self.engine.buffer.get_word_range(
                        self.engine.state.cursor.line,
                        self.engine.state.cursor.col,
                    );
                    if !word.is_empty() {
                        let word_chars = word.chars().count();
                        let char_start = self
                            .engine
                            .buffer
                            .line_to_char(self.engine.state.cursor.line - 1)
                            + start;
                        let char_end = char_start + word_chars;
                        let content = self.engine.buffer.get_char_range(
                            self.engine.state.cursor.line,
                            start,
                            self.engine.state.cursor.line,
                            start + word_chars,
                        );
                        let mod_count = self.engine.buffer.modification_count();
                        self.engine.register.set(register, &content);
                        self.engine.buffer.remove_range(char_start, char_end);
                        self.engine.undo_manager.push(Edit {
                            edit_type: EditType::Delete {
                                line: self.engine.state.cursor.line,
                                col: start,
                                text: content,
                            },
                            cursor_before: self.engine.state.cursor,
                            cursor_after: self.engine.state.cursor,
                            modification_count: mod_count,
                        });
                    }
                } else {
                    let (word, start, end) = self.engine.buffer.get_word_range(
                        self.engine.state.cursor.line,
                        self.engine.state.cursor.col,
                    );
                    if !word.is_empty() {
                        let line = self.engine.buffer.get_line(self.engine.state.cursor.line);
                        let line_chars: Vec<char> = line.chars().collect();
                        let mut aw_start = start;
                        while aw_start > 0 {
                            if line_chars[aw_start - 1] == ' ' || line_chars[aw_start - 1] == '\t' {
                                aw_start -= 1;
                            } else {
                                break;
                            }
                        }
                        let mut aw_end = end;
                        while aw_end < line_chars.len() {
                            if line_chars[aw_end] == ' ' || line_chars[aw_end] == '\t' {
                                aw_end += 1;
                            } else {
                                break;
                            }
                        }
                        let char_start = self
                            .engine
                            .buffer
                            .line_to_char(self.engine.state.cursor.line - 1)
                            + aw_start;
                        let char_end = char_start + (aw_end - aw_start);
                        let content = self.engine.buffer.get_char_range(
                            self.engine.state.cursor.line,
                            aw_start,
                            self.engine.state.cursor.line,
                            aw_end,
                        );
                        let mod_count = self.engine.buffer.modification_count();
                        self.engine.register.set(register, &content);
                        self.engine.buffer.remove_range(char_start, char_end);
                        self.engine.undo_manager.push(Edit {
                            edit_type: EditType::Delete {
                                line: self.engine.state.cursor.line,
                                col: aw_start,
                                text: content,
                            },
                            cursor_before: self.engine.state.cursor,
                            cursor_after: self.engine.state.cursor,
                            modification_count: mod_count,
                        });
                    }
                }
            }
            KeyCode::Char('"') | KeyCode::Char('\'') => {
                if let KeyCode::Char(quote) = key {
                    self.handle_quote_text_object(register, quote, inner);
                }
            }
            KeyCode::Char('(') | KeyCode::Char(')') | KeyCode::Char('b') => {
                self.handle_bracket_text_object(register, '(', ')', inner);
            }
            KeyCode::Char('[') | KeyCode::Char(']') => {
                self.handle_bracket_text_object(register, '[', ']', inner);
            }
            KeyCode::Char('{') | KeyCode::Char('}') | KeyCode::Char('B') => {
                self.handle_bracket_text_object(register, '{', '}', inner);
            }
            KeyCode::Char('<') | KeyCode::Char('>') => {
                self.handle_bracket_text_object(register, '<', '>', inner);
            }
            KeyCode::Char('W') => {
                self.handle_word_text_object(register, inner);
            }
            _ => {}
        }
        self.needs_render = true;
    }

    /// Handle `iW` (inner WORD) and `aW` (a WORD) text objects.
    /// WORDs are whitespace-delimited sequences (Vim's `W`/`B`).
    fn handle_word_text_object(&mut self, register: char, inner: bool) {
        let (word, start, end) = self
            .engine
            .buffer
            .get_word_range_upper(self.engine.state.cursor.line, self.engine.state.cursor.col);
        if word.is_empty() {
            return;
        }

        let line = self.engine.buffer.get_line(self.engine.state.cursor.line);
        let line_chars: Vec<char> = line.chars().collect();

        let (content_start, content_end) = if inner {
            (start, end)
        } else {
            // `aW` includes surrounding whitespace.
            let mut aw_start = start;
            while aw_start > 0 && line_chars[aw_start - 1].is_whitespace() {
                aw_start -= 1;
            }
            let mut aw_end = end;
            while aw_end < line_chars.len() && line_chars[aw_end].is_whitespace() {
                aw_end += 1;
            }
            (aw_start, aw_end)
        };

        let content_chars = content_end - content_start;
        let content: String = line_chars[content_start..content_end].iter().collect();
        let char_start = self
            .engine
            .buffer
            .line_to_char(self.engine.state.cursor.line - 1)
            + content_start;
        let char_end = char_start + content_chars;
        let mod_count = self.engine.buffer.modification_count();

        self.engine.register.set(register, &content);
        self.engine.buffer.remove_range(char_start, char_end);
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Delete {
                line: self.engine.state.cursor.line,
                col: content_start,
                text: content,
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
    }

    fn handle_quote_text_object(&mut self, register: char, quote: char, inner: bool) {
        let line = self.engine.buffer.get_line(self.engine.state.cursor.line);
        let chars: Vec<char> = line.chars().collect();
        let col = self.engine.state.cursor.col;

        let mut open_pos: Option<usize> = None;
        for i in (0..col).rev() {
            if chars[i] == quote {
                open_pos = Some(i);
                break;
            }
        }

        let mut close_pos: Option<usize> = None;
        for (i, &c) in chars.iter().enumerate().skip(col + 1) {
            if c == quote {
                close_pos = Some(i);
                break;
            }
        }

        if let Some(start) = open_pos {
            if let Some(end) = close_pos {
                let (content_start, content_end) = if inner {
                    (start + 1, end)
                } else {
                    (start, end + 1)
                };

                let content: String = chars[content_start..content_end].iter().collect();
                let char_start = self
                    .engine
                    .buffer
                    .line_to_char(self.engine.state.cursor.line - 1)
                    + content_start;
                let char_end = char_start + content.chars().count();
                let mod_count = self.engine.buffer.modification_count();

                self.engine.register.set(register, &content);
                self.engine.buffer.remove_range(char_start, char_end);
                self.engine.undo_manager.push(Edit {
                    edit_type: EditType::Delete {
                        line: self.engine.state.cursor.line,
                        col: content_start,
                        text: content,
                    },
                    cursor_before: self.engine.state.cursor,
                    cursor_after: self.engine.state.cursor,
                    modification_count: mod_count,
                });
            }
        }
    }

    fn handle_bracket_text_object(&mut self, register: char, open: char, close: char, inner: bool) {
        let col = self.engine.state.cursor.col;
        let cursor = (self.engine.state.cursor.line, col);

        // Find the innermost matching pair that contains the cursor. Unlike a
        // backwards scan, this also works when the cursor sits before the
        // opening bracket (e.g. `di(` on `foo(bar)`).
        let mut stack: Vec<(usize, usize)> = Vec::new();
        let mut pair: Option<((usize, usize), (usize, usize))> = None;
        for l in 1..=self.engine.buffer.line_count() {
            for (i, &c) in self.engine.buffer.line_chars(l).iter().enumerate() {
                if c == open {
                    stack.push((l, i));
                } else if c == close {
                    if let Some(op) = stack.pop() {
                        if op <= cursor
                            && cursor <= (l, i)
                            && pair.map_or(true, |(best, _)| op > best)
                        {
                            pair = Some((op, (l, i)));
                        }
                    }
                }
            }
        }

        // Not inside any pair: Vim searches forward on the cursor line for the
        // next opening bracket and uses its match.
        if pair.is_none() {
            let chars = self.engine.buffer.line_chars(cursor.0);
            if let Some(open_col) = (cursor.1..chars.len()).find(|&i| chars[i] == open) {
                let mut depth = 0usize;
                'outer: for l in cursor.0..=self.engine.buffer.line_count() {
                    let chars = self.engine.buffer.line_chars(l);
                    let from = if l == cursor.0 { open_col + 1 } else { 0 };
                    for (i, &c) in chars.iter().enumerate().skip(from) {
                        if c == open {
                            depth += 1;
                        } else if c == close {
                            if depth == 0 {
                                pair = Some(((cursor.0, open_col), (l, i)));
                                break 'outer;
                            }
                            depth -= 1;
                        }
                    }
                }
            }
        }

        let (open_line, open_pos) = match pair {
            Some((op, _)) => (op.0, Some(op.1)),
            None => (cursor.0, None),
        };
        let (close_line, close_pos) = match pair {
            Some((_, cl)) => (cl.0, Some(cl.1)),
            None => (cursor.0, None),
        };

        if let Some(start) = open_pos {
            if let Some(end) = close_pos {
                // Start just inside the opening bracket. When the bracket ends
                // its line, the inner range begins on the next line, leaving
                // the newline after `(` in place (as Vim does).
                let (start_line, start_col) = if inner {
                    if open_line == close_line || start + 1 < self.line_content_len(open_line) {
                        (open_line, start + 1)
                    } else {
                        (open_line + 1, 0)
                    }
                } else {
                    (open_line, start)
                };
                let (end_line, end_col) = if inner {
                    (close_line, end)
                } else {
                    (close_line, end + 1)
                };

                let mod_count = self.engine.buffer.modification_count();
                let content = self
                    .engine
                    .buffer
                    .delete_range(start_line, start_col, end_line, end_col);
                self.engine.register.set(register, &content);

                self.engine.state.cursor.line = start_line;
                self.engine.state.cursor.col =
                    start_col.min(self.engine.buffer.line_max_col(start_line));

                self.engine.undo_manager.push(Edit {
                    edit_type: EditType::Delete {
                        line: start_line,
                        col: start_col,
                        text: content,
                    },
                    cursor_before: self.engine.state.cursor,
                    cursor_after: self.engine.state.cursor,
                    modification_count: mod_count,
                });
            }
        }
    }
}

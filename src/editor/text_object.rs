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
        self.handle_text_object(key, register, inner).await;

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

        let mut open_pos: Option<usize> = None;
        let mut open_line = self.engine.state.cursor.line;
        for l in (1..=self.engine.state.cursor.line).rev() {
            let l_chars: Vec<char> = self.engine.buffer.line_chars(l);
            for i in (0..l_chars.len()).rev() {
                if l == self.engine.state.cursor.line && i >= col {
                    continue;
                }
                if l_chars[i] == open {
                    open_pos = Some(i);
                    open_line = l;
                    break;
                }
            }
            if open_pos.is_some() {
                break;
            }
        }

        let mut close_pos: Option<usize> = None;
        let mut close_line = self.engine.state.cursor.line;
        for l in self.engine.state.cursor.line..=self.engine.buffer.line_count() {
            let l_chars: Vec<char> = self.engine.buffer.line_chars(l);
            for (i, &c) in l_chars.iter().enumerate() {
                if l == self.engine.state.cursor.line && i <= col {
                    continue;
                }
                if c == close {
                    close_pos = Some(i);
                    close_line = l;
                    break;
                }
            }
            if close_pos.is_some() {
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

                let start_char_idx = self.engine.buffer.line_to_char(open_line - 1) + content_start;
                let end_char_idx = if close_line == open_line {
                    start_char_idx + (content_end - content_start)
                } else {
                    self.engine.buffer.line_to_char(close_line - 1) + content_end
                };

                let mut content = String::new();
                if open_line == close_line {
                    let line_chars: Vec<char> = self.engine.buffer.line_chars(open_line);
                    content = line_chars[content_start..content_end].iter().collect();
                } else {
                    let open_chars: Vec<char> = self.engine.buffer.line_chars(open_line);
                    content.push_str(&open_chars[content_start..].iter().collect::<String>());
                    content.push('\n');
                    for l in (open_line + 1)..close_line {
                        content.push_str(&self.engine.buffer.get_line(l));
                        content.push('\n');
                    }
                    let close_chars: Vec<char> = self.engine.buffer.line_chars(close_line);
                    content.push_str(&close_chars[..content_end].iter().collect::<String>());
                }

                let mod_count = self.engine.buffer.modification_count();
                self.engine.register.set(register, &content);
                self.engine
                    .buffer
                    .remove_range(start_char_idx, end_char_idx);

                // Position cursor at the start of the deleted range
                self.engine.state.cursor.line = open_line;
                self.engine.state.cursor.col = content_start;

                self.engine.undo_manager.push(Edit {
                    edit_type: EditType::Delete {
                        line: open_line,
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
}

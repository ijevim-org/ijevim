use crate::editor::Editor;
use crate::types::{MessageLevel, Position};
use crate::undo::{Edit, EditType};

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

impl Editor {
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

    pub fn set_mark(&mut self) {
        self.engine.state.mark = Some(self.engine.state.cursor);
        self.engine.state.region_active = true;
        self.needs_render = true;
    }

    pub fn has_active_region(&self) -> bool {
        self.engine.state.region_active
            && self.engine.state.mark.is_some()
            && self.engine.state.mark.unwrap() != self.engine.state.cursor
    }

    /// Emacs `C-x h` (`mark-whole-buffer`): mark at the start, point at the end.
    pub fn mark_whole_buffer(&mut self) {
        let last_line = self.engine.buffer.content_line_count();
        let end_col = self.engine.buffer.line_content_len(last_line);
        self.engine.state.mark = Some(Position { line: 1, col: 0 });
        self.engine.state.region_active = true;
        self.engine.state.cursor = Position {
            line: last_line,
            col: end_col,
        };
        self.needs_render = true;
    }

    /// Emacs `C-x C-x` (`exchange-point-and-mark`).
    pub fn exchange_point_and_mark(&mut self) {
        match self.engine.state.mark {
            Some(mark) => {
                self.engine.state.mark = Some(self.engine.state.cursor);
                self.engine.state.cursor = mark;
                self.engine.state.region_active = true;
                self.needs_render = true;
            }
            None => self
                .engine
                .state
                .push_message(MessageLevel::Info, "No mark set in this buffer".to_string()),
        }
    }

    pub fn kill_region(&mut self) {
        let mark = match self.engine.state.mark {
            Some(m) => m,
            None => {
                self.engine.state.push_message(
                    MessageLevel::Info,
                    "The mark is not set now, so there is no region".to_string(),
                );
                return;
            }
        };
        let cursor = self.engine.state.cursor;

        let (s_line, s_col, e_line, e_col) =
            if mark.line < cursor.line || (mark.line == cursor.line && mark.col <= cursor.col) {
                (mark.line, mark.col, cursor.line, cursor.col)
            } else {
                (cursor.line, cursor.col, mark.line, mark.col)
            };

        if e_line == s_line && e_col <= s_col {
            self.deactivate_region();
            return;
        }

        let mod_count = self.engine.buffer.modification_count();
        let content = self
            .engine
            .buffer
            .delete_range(s_line, s_col, e_line, e_col);
        if content.is_empty() {
            self.deactivate_region();
            return;
        }

        self.kill_to_ring(&content);
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Delete {
                line: s_line,
                col: s_col,
                text: content,
            },
            cursor_before: cursor,
            cursor_after: Position {
                line: s_line,
                col: s_col,
            },
            modification_count: mod_count,
        });
        self.engine.state.cursor = Position {
            line: s_line,
            col: s_col,
        };
        self.deactivate_region();
        self.on_buffer_modified();
    }

    pub fn deactivate_region(&mut self) {
        if self.engine.state.region_active {
            self.engine.state.region_active = false;
            self.needs_render = true;
        }
    }

    pub fn copy_region(&mut self) {
        let mark = match self.engine.state.mark {
            Some(m) => m,
            None => {
                self.engine.state.push_message(
                    MessageLevel::Info,
                    "The mark is not set now, so there is no region".to_string(),
                );
                return;
            }
        };
        let cursor = self.engine.state.cursor;
        let (s_line, s_col, e_line, e_col) =
            if mark.line < cursor.line || (mark.line == cursor.line && mark.col <= cursor.col) {
                (mark.line, mark.col, cursor.line, cursor.col)
            } else {
                (cursor.line, cursor.col, mark.line, mark.col)
            };
        let content = self
            .engine
            .buffer
            .get_char_range(s_line, s_col, e_line, e_col);
        if content.is_empty() {
            self.deactivate_region();
            return;
        }
        // `M-w` saves to the kill ring without making the next kill append.
        self.engine.kill_ring.push(&content);
        self.engine.register.set('"', &content);
        self.deactivate_region();
        self.needs_render = true;
    }

    pub fn yank_from_kill_ring(&mut self) {
        let text = self.engine.kill_ring.yank().map(|s| s.to_string());
        if let Some(text) = text {
            let start = self.engine.state.cursor;
            let line = start.line;
            let col = start.col;
            let mod_count = self.engine.buffer.modification_count();

            self.engine.buffer.insert(line, col, &text);
            let after = point_after(start, &text);
            self.engine.state.cursor = after;
            self.engine.cursor_state.yank_start = Some(start);
            self.engine.cursor_state.yank_length = text.chars().count();
            self.engine.cursor_state.yank_text = text.clone();
            self.engine.kill_ring.reset_index();

            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Insert {
                    line,
                    col,
                    text: text.clone(),
                },
                cursor_before: start,
                cursor_after: after,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        }
    }

    pub fn yank_pop(&mut self) {
        let start = match self.engine.cursor_state.yank_start {
            Some(p) => p,
            None => return,
        };
        let old_text = self.engine.cursor_state.yank_text.clone();
        if old_text.is_empty() {
            return;
        }
        let start_char = self
            .engine
            .buffer
            .line_to_char(start.line.saturating_sub(1))
            + start.col;
        let old_len = old_text.chars().count();
        let mod_count = self.engine.buffer.modification_count();
        self.engine
            .buffer
            .remove_range(start_char, start_char + old_len);

        let next = self.engine.kill_ring.yank_pop().map(|s| s.to_string());
        if let Some(text) = next {
            self.engine.buffer.insert(start.line, start.col, &text);
            let after = point_after(start, &text);
            self.engine.state.cursor = after;
            self.engine.cursor_state.yank_length = text.chars().count();
            self.engine.cursor_state.yank_text = text.clone();
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Group {
                    edits: vec![
                        Edit {
                            edit_type: EditType::Delete {
                                line: start.line,
                                col: start.col,
                                text: old_text,
                            },
                            cursor_before: start,
                            cursor_after: start,
                            modification_count: mod_count,
                        },
                        Edit {
                            edit_type: EditType::Insert {
                                line: start.line,
                                col: start.col,
                                text,
                            },
                            cursor_before: start,
                            cursor_after: after,
                            modification_count: mod_count,
                        },
                    ],
                },
                cursor_before: start,
                cursor_after: after,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        } else {
            self.engine.cursor_state.yank_start = None;
            self.engine.cursor_state.yank_length = 0;
            self.engine.cursor_state.yank_text.clear();
        }
    }

    /// Emacs `C-t` (`transpose-chars`).
    ///
    /// At end of line the two characters before point are swapped and point
    /// stays put; elsewhere the characters around point are swapped and point
    /// advances one column.
    pub fn transpose_chars(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let chars = self.engine.buffer.line_chars(line);
        let len = self.engine.buffer.line_content_len(line);
        if len < 2 || col == 0 {
            return;
        }

        let (swap_at, advance) = if col >= len {
            (len - 2, false)
        } else {
            (col - 1, true)
        };
        let c1 = chars[swap_at];
        let c2 = chars[swap_at + 1];
        let mod_count = self.engine.buffer.modification_count();

        self.engine.buffer.delete(line, swap_at + 1);
        self.engine.buffer.delete(line, swap_at);
        self.engine.buffer.insert_char(line, swap_at, c2);
        self.engine.buffer.insert_char(line, swap_at + 1, c1);

        self.engine.state.cursor.col = if advance { (col + 1).min(len) } else { len };
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Replace {
                line,
                col: swap_at,
                old_text: [c1, c2].iter().collect(),
                new_text: [c2, c1].iter().collect(),
            },
            cursor_before: Position { line, col },
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.on_buffer_modified();
    }

    pub fn insert_tab(&mut self) {
        self.insert_char('\t');
    }

    /// Emacs `C-o` (`open-line`): insert a newline at point and stay before it.
    pub fn open_line_at_point(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let mod_count = self.engine.buffer.modification_count();
        self.engine.buffer.insert(line, col, "\n");
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Split { line, col },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.on_buffer_modified();
    }

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

    pub fn kill_word(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let (_, _, end_col) = self.engine.buffer.get_word_range(line, col);
        let mod_count = self.engine.buffer.modification_count();

        if end_col > col {
            let deleted = self.engine.buffer.delete_range(line, col, line, end_col);
            if !deleted.is_empty() {
                self.kill_to_ring(&deleted);
            }
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line,
                    col,
                    text: deleted,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        } else if line < self.engine.buffer.line_count() {
            // C-k at end of line joins the next line, like Emacs.
            let join_col = self.engine.buffer.line_content_len(line).max(col);
            self.engine.buffer.merge_with_prev_line(line + 1);
            self.kill_to_ring("\n");
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Merge {
                    line,
                    deleted_newline_col: join_col,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        }
    }

    pub(crate) fn kill_line(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let end_col = self.engine.buffer.line_content_len(line);

        if col == 0 && (end_col > 0 || line < self.engine.buffer.line_count()) {
            // Emacs `C-k` at the beginning of a line kills the whole line,
            // including its newline (unlike Vim's `dd`-free partial kill).
            let mut deleted = self.engine.buffer.get_line(line);
            while deleted.ends_with('\n') || deleted.ends_with('\r') {
                deleted.pop();
            }
            let mod_count = self.engine.buffer.modification_count();
            self.engine.buffer.delete_range(line, 0, line, end_col);
            if line < self.engine.buffer.line_count() {
                self.engine.buffer.merge_with_prev_line(line + 1);
                deleted.push('\n');
            }
            self.kill_to_ring(&deleted);
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line,
                    col: 0,
                    text: deleted,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: Position { line, col: 0 },
                modification_count: mod_count,
            });
            self.on_buffer_modified();
            return;
        }

        if col < end_col {
            let deleted: String = self
                .engine
                .buffer
                .get_line(line)
                .chars()
                .skip(col)
                .take(end_col - col)
                .collect();
            let mod_count = self.engine.buffer.modification_count();
            self.engine.buffer.delete_range(line, col, line, end_col);
            self.kill_to_ring(&deleted);
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line,
                    col,
                    text: deleted,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        } else if line < self.engine.buffer.line_count() {
            // At end of line, C-k kills the line break itself.
            let join_col = end_col.max(col);
            let mod_count = self.engine.buffer.modification_count();
            self.engine.buffer.merge_with_prev_line(line + 1);
            self.kill_to_ring("\n");
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Merge {
                    line,
                    deleted_newline_col: join_col,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        }
    }

    pub(crate) fn insert_char(&mut self, c: char) {
        let before = self.engine.state.cursor;
        let line = before.line;
        let col = before.col;
        let mod_count = self.engine.buffer.modification_count();
        self.engine.buffer.insert_char(line, col, c);
        self.engine.state.cursor.col += 1;
        let after = self.engine.state.cursor;
        self.engine.undo_manager.push_insert_coalescing(Edit {
            edit_type: EditType::Insert {
                line,
                col,
                text: c.to_string(),
            },
            cursor_before: before,
            cursor_after: after,
            modification_count: mod_count,
        });
        self.on_buffer_modified();
    }

    pub(crate) fn delete_char_backward(&mut self) {
        let before = self.engine.state.cursor;
        let line = before.line;
        let col = before.col;
        let mod_count = self.engine.buffer.modification_count();

        if col > 0 {
            let deleted = self
                .engine
                .buffer
                .get_line(line)
                .chars()
                .nth(col - 1)
                .unwrap_or_default()
                .to_string();
            self.engine.buffer.delete(line, col - 1);
            self.engine.state.cursor.col -= 1;
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line,
                    col: col - 1,
                    text: deleted,
                },
                cursor_before: before,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        } else if line > 1 {
            let prev_len = self.engine.buffer.line_content_len(line - 1);
            self.engine.buffer.merge_with_prev_line(line);
            self.engine.state.cursor.line = line - 1;
            self.engine.state.cursor.col = prev_len;
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Merge {
                    line: line - 1,
                    deleted_newline_col: prev_len,
                },
                cursor_before: before,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        }
    }

    pub(crate) fn insert_newline(&mut self) {
        let before = self.engine.state.cursor;
        let line = before.line;
        let col = before.col;
        let mod_count = self.engine.buffer.modification_count();

        // Splitting the line is a single newline insertion: everything after
        // the cursor stays on the new line automatically.
        self.engine.buffer.insert(line, col, "\n");
        let after = Position {
            line: line + 1,
            col: 0,
        };
        self.engine.state.cursor = after;
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Split { line, col },
            cursor_before: before,
            cursor_after: after,
            modification_count: mod_count,
        });
        self.on_buffer_modified();
    }

    pub(crate) fn delete_char_forward(&mut self) {
        let before = self.engine.state.cursor;
        let line = before.line;
        let col = before.col;
        let content_len = self.engine.buffer.line_content_len(line);
        let mod_count = self.engine.buffer.modification_count();

        if col < content_len {
            let deleted_char = self
                .engine
                .buffer
                .get_line(line)
                .chars()
                .nth(col)
                .unwrap_or_default()
                .to_string();
            self.engine.buffer.delete(line, col);
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line,
                    col,
                    text: deleted_char,
                },
                cursor_before: before,
                cursor_after: before,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        } else if line < self.engine.buffer.line_count() {
            // C-d at end of line deletes the newline, joining the next line.
            self.engine.buffer.merge_with_prev_line(line + 1);
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Merge {
                    line,
                    deleted_newline_col: content_len,
                },
                cursor_before: before,
                cursor_after: before,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        }
    }
}

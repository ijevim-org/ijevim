use crate::editor::Editor;
use crate::types::Position;
use crate::undo::{Edit, EditType};

impl Editor {
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

    pub fn kill_region(&mut self) {
        let mark = match self.engine.state.mark {
            Some(m) => m,
            None => return,
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

        self.engine.kill_ring.push(&content);
        self.engine.register.set('"', &content);
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
            None => return,
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
        self.engine.kill_ring.push(&content);
        self.engine.register.set('"', &content);
        self.deactivate_region();
        self.needs_render = true;
    }

    pub fn yank_from_kill_ring(&mut self) {
        let text = self.engine.kill_ring.yank().map(|s| s.to_string());
        if let Some(ref text) = text {
            self.engine.cursor_state.yank_start = Some(self.engine.state.cursor);
            self.engine.cursor_state.yank_length = text.chars().count();
            self.engine.kill_ring.reset_index();
            self.engine.buffer.insert(
                self.engine.state.cursor.line,
                self.engine.state.cursor.col,
                text,
            );
            self.engine.state.cursor.col += self.engine.cursor_state.yank_length;
            self.on_buffer_modified();
        }
    }

    pub fn yank_pop(&mut self) {
        let start = match self.engine.cursor_state.yank_start {
            Some(p) => p,
            None => return,
        };
        let len = self.engine.cursor_state.yank_length;
        if len == 0 {
            return;
        }
        let start_char = self
            .engine
            .buffer
            .line_to_char(start.line.saturating_sub(1))
            + start.col;
        self.engine
            .buffer
            .remove_range(start_char, start_char + len);
        let next = self.engine.kill_ring.yank_pop().map(|s| s.to_string());
        if let Some(ref text) = next {
            self.engine.buffer.insert(start.line, start.col, text);
            self.engine.cursor_state.yank_length = text.chars().count();
            self.engine.state.cursor = start;
            self.engine.state.cursor.col += self.engine.cursor_state.yank_length;
        } else {
            self.engine.cursor_state.yank_start = None;
            self.engine.cursor_state.yank_length = 0;
        }
        self.on_buffer_modified();
    }

    pub fn transpose_chars(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;

        if col > 0 {
            let line_str = self.engine.buffer.get_line(line);
            let chars: Vec<char> = line_str.chars().collect();
            if col < chars.len() {
                let c1 = chars[col - 1];
                let c2 = chars[col];

                self.engine.buffer.delete(line, col);
                self.engine.buffer.delete(line, col - 1);
                self.engine.buffer.insert_char(line, col - 1, c2);
                self.engine.buffer.insert_char(line, col, c1);
                self.engine.state.cursor.col =
                    (col + 1).min(self.engine.buffer.line_max_col(line));
                self.on_buffer_modified();
            }
        }
    }

    pub fn insert_tab(&mut self) {
        self.engine.buffer.insert_char(
            self.engine.state.cursor.line,
            self.engine.state.cursor.col,
            '\t',
        );
        self.engine.state.cursor.col += 1;
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
                self.engine.kill_ring.push(&deleted);
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
            self.engine.kill_ring.push("\n");
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
        let line = self.engine.buffer.get_line(self.engine.state.cursor.line);
        let end_col = self.engine.buffer.line_content_len(self.engine.state.cursor.line);
        if self.engine.state.cursor.col < end_col {
            let deleted: String = line
                .chars()
                .skip(self.engine.state.cursor.col)
                .take(end_col - self.engine.state.cursor.col)
                .collect();
            self.engine.kill_ring.push(&deleted);
            let mod_count = self.engine.buffer.modification_count();
            self.engine.buffer.delete_range(
                self.engine.state.cursor.line,
                self.engine.state.cursor.col,
                self.engine.state.cursor.line,
                end_col,
            );
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line: self.engine.state.cursor.line,
                    col: self.engine.state.cursor.col,
                    text: deleted,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        }
    }

    pub(crate) fn insert_char(&mut self, c: char) {
        self.engine.buffer.insert_char(
            self.engine.state.cursor.line,
            self.engine.state.cursor.col,
            c,
        );
        self.engine.state.cursor.col += 1;
        self.on_buffer_modified();
    }

    pub(crate) fn delete_char_backward(&mut self) {
        if self.engine.state.cursor.col > 0 {
            self.engine.buffer.delete(
                self.engine.state.cursor.line,
                self.engine.state.cursor.col - 1,
            );
            self.engine.state.cursor.col -= 1;
            self.on_buffer_modified();
        } else if self.engine.state.cursor.line > 1 {
            let merged = self
                .engine
                .buffer
                .get_line(self.engine.state.cursor.line - 1);
            let prev_len = merged.chars().count();
            self.engine
                .buffer
                .merge_with_prev_line(self.engine.state.cursor.line);
            self.engine.state.cursor.line -= 1;
            self.engine.state.cursor.col = prev_len.saturating_sub(1);
            self.on_buffer_modified();
        }
    }

    pub(crate) fn insert_newline(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let mod_count = self.engine.buffer.modification_count();

        // Splitting the line is a single newline insertion: everything after
        // the cursor stays on the new line automatically.
        self.engine.buffer.insert(line, col, "\n");

        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Split { line, col },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.engine.state.cursor.line += 1;
        self.engine.state.cursor.col = 0;
        self.on_buffer_modified();
    }

    pub(crate) fn delete_char_forward(&mut self) {
        let line = self.engine.buffer.get_line(self.engine.state.cursor.line);
        let content_len = self.engine.buffer.line_content_len(self.engine.state.cursor.line);
        if self.engine.state.cursor.col < content_len {
            let deleted_char = line
                .chars()
                .nth(self.engine.state.cursor.col)
                .unwrap_or_default()
                .to_string();
            let mod_count = self.engine.buffer.modification_count();
            self.engine
                .buffer
                .delete(self.engine.state.cursor.line, self.engine.state.cursor.col);
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line: self.engine.state.cursor.line,
                    col: self.engine.state.cursor.col,
                    text: deleted_char,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.on_buffer_modified();
        }
    }
}

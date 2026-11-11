use crate::editor::Editor;
use crate::types::Position;
use crate::undo::{Edit, EditType};

impl Editor {
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

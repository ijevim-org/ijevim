use crate::editor::Editor;
use crate::types::Position;
use crate::undo::{Edit, EditType};

impl Editor {
    pub fn kill_word(&mut self) {
        let point = self.engine.state.cursor;
        let Some((from, to)) = self.forward_word_bounds(point) else {
            return;
        };
        let deleted = self
            .engine
            .buffer
            .get_char_range(from.line, from.col, to.line, to.col);
        if deleted.is_empty() {
            return;
        }
        let mod_count = self.engine.buffer.modification_count();
        self.engine
            .buffer
            .delete_range(from.line, from.col, to.line, to.col);
        self.kill_to_ring(&deleted);
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Delete {
                line: from.line,
                col: from.col,
                text: deleted,
            },
            cursor_before: point,
            cursor_after: from,
            modification_count: mod_count,
        });
        self.engine.state.cursor = from;
        self.on_buffer_modified();
        self.needs_render = true;
    }

    pub fn kill_word_backward(&mut self) {
        let point = self.engine.state.cursor;
        let (line, col) = self.word_backward_pos(point.line, point.col, false);
        let from = Position { line, col };
        if from == point {
            return;
        }
        let deleted = self
            .engine
            .buffer
            .get_char_range(from.line, from.col, point.line, point.col);
        if deleted.is_empty() {
            return;
        }
        let mod_count = self.engine.buffer.modification_count();
        self.engine
            .buffer
            .delete_range(from.line, from.col, point.line, point.col);
        self.kill_to_ring(&deleted);
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Delete {
                line: from.line,
                col: from.col,
                text: deleted,
            },
            cursor_before: point,
            cursor_after: from,
            modification_count: mod_count,
        });
        self.engine.state.cursor = from;
        self.on_buffer_modified();
        self.needs_render = true;
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
}

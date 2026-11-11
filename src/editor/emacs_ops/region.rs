use crate::editor::Editor;
use crate::types::{MessageLevel, Position};
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
}

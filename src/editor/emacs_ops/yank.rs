use super::point_after;
use crate::editor::Editor;
use crate::undo::{Edit, EditType};

impl Editor {
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
}

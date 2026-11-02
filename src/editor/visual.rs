use crate::editor::Editor;
use crate::types::{Mode, VisualType};
use crate::undo::{Edit, EditType};
use crossterm::event::KeyCode;

impl Editor {
    /// Helper: save the current visual selection to last_visual for gv
    fn save_visual_selection(&mut self) {
        self.engine.state.last_visual_start = self.engine.state.visual_start;
        self.engine.state.last_visual_type = self.engine.state.visual_type;
    }

    pub(crate) async fn handle_visual(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                self.save_visual_selection();
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Normal;
                self.engine.state.visual_start = None;
                self.engine.state.visual_type = None;
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Normal,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('y') => {
                self.save_visual_selection();
                self.visual_yank();
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Normal;
                self.engine.state.visual_start = None;
                self.engine.state.visual_type = None;
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Normal,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('d') => {
                self.save_visual_selection();
                self.visual_delete();
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Normal;
                self.engine.state.visual_start = None;
                self.engine.state.visual_type = None;
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Normal,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('c') => {
                self.save_visual_selection();
                self.visual_delete();
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Insert;
                self.engine.state.visual_start = None;
                self.engine.state.visual_type = None;
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('h') => {
                self.engine.state.cursor.col = self.engine.state.cursor.col.saturating_sub(1);
                self.needs_render = true;
            }
            KeyCode::Char('l') => {
                let line_len = self
                    .engine
                    .buffer
                    .line_char_len(self.engine.state.cursor.line);
                self.engine.state.cursor.col =
                    (self.engine.state.cursor.col + 1).min(line_len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('j') => {
                let line_count = self.engine.buffer.line_count();
                self.engine.state.cursor.line = (self.engine.state.cursor.line + 1).min(line_count);
                let len = self
                    .engine
                    .buffer
                    .get_line(self.engine.state.cursor.line)
                    .len();
                self.engine.state.cursor.col =
                    self.engine.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('k') => {
                self.engine.state.cursor.line =
                    self.engine.state.cursor.line.saturating_sub(1).max(1);
                let len = self
                    .engine
                    .buffer
                    .get_line(self.engine.state.cursor.line)
                    .len();
                self.engine.state.cursor.col =
                    self.engine.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('0') => {
                self.engine.state.cursor.col = 0;
                self.needs_render = true;
            }
            KeyCode::Char('^') => {
                let line = self.engine.buffer.get_line(self.engine.state.cursor.line);
                let first_non_blank = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                self.engine.state.cursor.col = first_non_blank;
                self.needs_render = true;
            }
            KeyCode::Char('$') => {
                let line = self.engine.buffer.get_line(self.engine.state.cursor.line);
                let line_char_len = self
                    .engine
                    .buffer
                    .line_char_len(self.engine.state.cursor.line);
                self.engine.state.cursor.col = if line_char_len > 0 && line.ends_with('\n') {
                    line_char_len.saturating_sub(2)
                } else {
                    line_char_len.saturating_sub(1)
                };
                self.needs_render = true;
            }
            KeyCode::Char('I') if self.engine.state.visual_type == Some(VisualType::Block) => {
                self.save_visual_selection();
                let s_col = self
                    .engine
                    .state
                    .visual_start
                    .map(|s| s.col)
                    .unwrap_or(0)
                    .min(self.engine.state.cursor.col);
                self.engine.state.cursor.col = s_col;
                self.engine.state.visual_start = None;
                self.engine.state.visual_type = None;
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Insert;
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                self.needs_render = true;
            }
            KeyCode::Char('A') if self.engine.state.visual_type == Some(VisualType::Block) => {
                self.save_visual_selection();
                let e_col = self
                    .engine
                    .state
                    .visual_start
                    .map(|s| s.col)
                    .unwrap_or(0)
                    .max(self.engine.state.cursor.col);
                self.engine.state.cursor.col = e_col;
                self.engine.state.visual_start = None;
                self.engine.state.visual_type = None;
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Insert;
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                self.needs_render = true;
            }
            _ => {}
        }
    }

    fn visual_yank(&mut self) {
        if let (Some(start), Some(vtype)) = (
            &self.engine.state.visual_start,
            self.engine.state.visual_type,
        ) {
            let content = match vtype {
                VisualType::Character => {
                    let (s_line, s_col, e_line, e_col) =
                        self.normalize_selection(start, &self.engine.state.cursor);
                    self.engine
                        .buffer
                        .get_char_range(s_line, s_col, e_line, e_col)
                }
                VisualType::Line => {
                    let (s_line, e_line) =
                        self.normalize_line_selection(start.line, self.engine.state.cursor.line);
                    self.engine.buffer.get_line_range(s_line, e_line)
                }
                // Block ranges are end-exclusive, so include the cursor column.
                VisualType::Block => self.engine.buffer.get_block_range(
                    start.line,
                    start.col,
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col + 1,
                ),
            };
            self.engine.register.set('"', &content);
        }
    }

    fn visual_delete(&mut self) {
        if let (Some(start), Some(vtype)) = (
            &self.engine.state.visual_start,
            self.engine.state.visual_type,
        ) {
            let start = *start;
            let mod_count = self.engine.buffer.modification_count();
            let cursor_before = self.engine.state.cursor;
            let mut edits: Vec<Edit> = Vec::new();

            let (cursor_line, cursor_col) = match vtype {
                VisualType::Character => {
                    let (s_line, s_col, e_line, e_col) =
                        self.normalize_selection(&start, &self.engine.state.cursor);
                    let content = self
                        .engine
                        .buffer
                        .delete_range(s_line, s_col, e_line, e_col);
                    self.engine.register.set('"', &content);
                    edits.push(Edit {
                        edit_type: EditType::Delete {
                            line: s_line,
                            col: s_col,
                            text: content,
                        },
                        cursor_before,
                        cursor_after: self.engine.state.cursor,
                        modification_count: mod_count,
                    });
                    (s_line, s_col)
                }
                VisualType::Line => {
                    let (s_line, e_line) =
                        self.normalize_line_selection(start.line, self.engine.state.cursor.line);
                    let count = e_line.saturating_sub(s_line) + 1;
                    let mut content = String::new();
                    for _ in 0..count {
                        let deleted = self.engine.buffer.delete_line(s_line);
                        content.push_str(&deleted);
                        edits.push(Edit {
                            edit_type: EditType::DeleteLine {
                                line: s_line,
                                text: deleted,
                            },
                            cursor_before,
                            cursor_after: self.engine.state.cursor,
                            modification_count: mod_count,
                        });
                    }
                    self.engine.register.set('"', &content);
                    (s_line.min(self.engine.buffer.line_count()), 0)
                }
                VisualType::Block => {
                    let s_line = start.line.min(self.engine.state.cursor.line);
                    let e_line = start.line.max(self.engine.state.cursor.line);
                    let s_col = start.col.min(self.engine.state.cursor.col);
                    let e_col = start.col.max(self.engine.state.cursor.col);

                    // Record one deletion per line so undo can restore the
                    // rectangle without inserting stray newlines.
                    for line_num in s_line..=e_line {
                        let chars = self.engine.buffer.line_chars(line_num);
                        let end = (e_col + 1).min(chars.len());
                        if s_col < end {
                            edits.push(Edit {
                                edit_type: EditType::Delete {
                                    line: line_num,
                                    col: s_col,
                                    text: chars[s_col..end].iter().collect(),
                                },
                                cursor_before,
                                cursor_after: self.engine.state.cursor,
                                modification_count: mod_count,
                            });
                        }
                    }

                    let content =
                        self.engine
                            .buffer
                            .delete_block_range(s_line, s_col, e_line, e_col + 1);
                    self.engine.register.set('"', &content);
                    (s_line, s_col)
                }
            };

            // A visual delete is one undo step regardless of how many lines
            // were touched.
            if !edits.is_empty() {
                self.engine.undo_manager.push(Edit {
                    edit_type: EditType::Group { edits },
                    cursor_before,
                    cursor_after: self.engine.state.cursor,
                    modification_count: mod_count,
                });
            }

            self.engine.state.cursor.line = cursor_line;
            self.engine.state.cursor.col = cursor_col;
            self.on_buffer_modified();
        }
    }

    fn normalize_selection(
        &self,
        start: &crate::types::Position,
        end: &crate::types::Position,
    ) -> (usize, usize, usize, usize) {
        if start.line < end.line || (start.line == end.line && start.col <= end.col) {
            (start.line, start.col, end.line, end.col + 1)
        } else {
            (end.line, end.col, start.line, start.col + 1)
        }
    }

    fn normalize_line_selection(&self, start_line: usize, end_line: usize) -> (usize, usize) {
        if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        }
    }
}

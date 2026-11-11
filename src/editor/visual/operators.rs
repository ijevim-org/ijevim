use crate::editor::Editor;
use crate::types::{Mode, Position, VisualType};
use crate::undo::{Edit, EditType};

impl Editor {
    pub(crate) fn visual_insert_start(&mut self) {
        let Some((start, _)) = self.selection_bounds() else {
            return;
        };
        let pos = if self.engine.state.visual_type == Some(VisualType::Block) {
            let (s_line, e_line, s_col, _) = self.block_bounds();
            self.block_insert = Some(crate::editor::BlockInsert {
                col: s_col,
                cursor_col: s_col,
                start_line: s_line,
                end_line: e_line,
            });
            Position {
                line: s_line,
                col: s_col,
            }
        } else {
            start
        };
        self.engine.state.cursor = pos;
        self.engine.insert_state.accum.clear();
        self.engine.insert_state.start_pos = Some(pos);
        self.exit_visual(Mode::Insert);
    }

    pub(crate) fn visual_insert_end(&mut self) {
        let Some((_, end)) = self.selection_bounds() else {
            return;
        };
        let pos = if self.engine.state.visual_type == Some(VisualType::Block) {
            let (s_line, e_line, s_col, e_col) = self.block_bounds();
            let col = e_col + 1;
            self.block_insert = Some(crate::editor::BlockInsert {
                col,
                cursor_col: s_col,
                start_line: s_line,
                end_line: e_line,
            });
            Position { line: s_line, col }
        } else {
            end
        };
        self.engine.state.cursor = pos;
        self.engine.insert_state.accum.clear();
        self.engine.insert_state.start_pos = Some(pos);
        self.exit_visual(Mode::Insert);
    }

    pub(super) fn visual_yank(&mut self, linewise: bool) {
        let register = self.visual_register();
        let blockwise = self.engine.state.visual_type == Some(VisualType::Block);
        let content = if linewise {
            if let Some((start, end)) = self.selection_bounds() {
                let (s, e) = self.normalize_line_selection(start.line, end.line);
                self.engine.buffer.get_line_range(s, e)
            } else {
                String::new()
            }
        } else {
            self.selection_text()
        };
        if !content.is_empty() {
            let kind = if blockwise {
                crate::register::RegisterKind::Block
            } else if linewise || content.ends_with('\n') {
                crate::register::RegisterKind::Line
            } else {
                crate::register::RegisterKind::Char
            };
            self.engine.register.set_with_kind(register, &content, kind);
        }
        if let Some((start, _)) = self.selection_bounds() {
            self.engine.state.cursor = start;
        }
        self.needs_render = true;
    }

    /// Blockwise `D`: truncate every selected line from the block's left edge.
    pub(super) fn visual_delete_to_eol(&mut self) {
        let Some((start, end)) = self.selection_bounds() else {
            return;
        };
        let register = self.visual_register();
        let s_line = start.line.min(end.line);
        let e_line = start.line.max(end.line);
        let s_col = start.col.min(end.col);
        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();
        let mut edits = Vec::new();
        let mut content = String::new();
        for line in s_line..=e_line {
            let len = self.engine.buffer.line_content_len(line);
            if s_col < len {
                let chars = self.engine.buffer.line_chars(line);
                let removed: String = chars[s_col..len].iter().collect();
                content.push_str(&removed);
                edits.push(Edit {
                    edit_type: EditType::Delete {
                        line,
                        col: s_col,
                        text: removed.clone(),
                    },
                    cursor_before,
                    cursor_after: self.engine.state.cursor,
                    modification_count: mod_count,
                });
                let start_char = self.engine.buffer.line_to_char(line.saturating_sub(1)) + s_col;
                self.engine
                    .buffer
                    .remove_range(start_char, start_char + removed.chars().count());
            }
            if line < e_line {
                content.push('\n');
            }
        }
        if !content.is_empty() {
            self.engine.register.set_with_kind(
                register,
                &content,
                crate::register::RegisterKind::Block,
            );
        }
        self.engine.state.cursor.line = s_line;
        self.engine.state.cursor.col = s_col.saturating_sub(1).min(
            self.engine
                .buffer
                .line_content_len(s_line)
                .saturating_sub(1),
        );
        self.push_undo_group(edits);
        self.on_buffer_modified();
    }

    /// Delete the current selection as one undo group.
    pub(super) fn visual_delete(&mut self) {
        let register = self.visual_register();
        let edits = self.visual_delete_collect(register);
        self.push_undo_group(edits);
    }

    pub(super) fn visual_delete_lines(&mut self) {
        let register = self.visual_register();
        let edits = self.visual_delete_lines_collect(register);
        self.push_undo_group(edits);
    }

    pub(super) fn visual_change(&mut self, linewise: bool) {
        let register = self.visual_register();
        if linewise {
            if let Some((start, end)) = self.selection_bounds() {
                let (s, e) = self.normalize_line_selection(start.line, end.line);
                self.engine.state.cursor.line = s;
                self.change_line_range(register, s, e);
            }
        } else {
            let block = if self.engine.state.visual_type == Some(VisualType::Block) {
                let (s_line, e_line, s_col, _) = self.block_bounds();
                Some(crate::editor::BlockInsert {
                    col: s_col,
                    cursor_col: s_col,
                    start_line: s_line,
                    end_line: e_line,
                })
            } else {
                None
            };
            let edits = self.visual_delete_collect(register);
            self.push_undo_group(edits);
            self.enter_insert_after_change();
            self.block_insert = block;
        }
    }

    /// Delete the selection and return the undo edits without pushing them.
    pub(super) fn visual_delete_collect(&mut self, register: char) -> Vec<Edit> {
        let Some((start, end)) = self.selection_bounds() else {
            return Vec::new();
        };
        let vtype = self
            .engine
            .state
            .visual_type
            .unwrap_or(VisualType::Character);
        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();
        let mut edits: Vec<Edit> = Vec::new();

        let (cursor_line, cursor_col) = match vtype {
            VisualType::Character => {
                let (s_line, s_col, e_line, e_col) = self.normalize_selection(&start, &end);
                let content = self
                    .engine
                    .buffer
                    .delete_range(s_line, s_col, e_line, e_col);
                self.engine.register.set(register, &content);
                if !content.is_empty() {
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
                }
                (s_line, s_col)
            }
            VisualType::Line => {
                let (s_line, e_line) = self.normalize_line_selection(start.line, end.line);
                let count = e_line.saturating_sub(s_line) + 1;
                let mut content = String::new();
                for _ in 0..count {
                    let deleted = self.engine.buffer.delete_line(s_line);
                    if deleted.is_empty() {
                        continue;
                    }
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
                self.engine.register.set(register, &content);
                (s_line.min(self.engine.buffer.line_count()), 0)
            }
            VisualType::Block => {
                let s_line = start.line.min(end.line);
                let e_line = start.line.max(end.line);
                let s_col = start.col.min(end.col);
                let e_col = start.col.max(end.col);

                for line_num in s_line..=e_line {
                    let chars = self.engine.buffer.line_chars(line_num);
                    let end_col = (e_col + 1).min(chars.len());
                    if s_col < end_col {
                        edits.push(Edit {
                            edit_type: EditType::Delete {
                                line: line_num,
                                col: s_col,
                                text: chars[s_col..end_col].iter().collect(),
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
                self.engine.register.set(register, &content);
                (s_line, s_col)
            }
        };

        self.engine.state.cursor.line = cursor_line;
        self.engine.state.cursor.col = cursor_col;
        self.on_buffer_modified();
        edits
    }

    /// Delete full lines for `X` / `D`, returning the undo edits.
    pub(super) fn visual_delete_lines_collect(&mut self, register: char) -> Vec<Edit> {
        let Some((start, end)) = self.selection_bounds() else {
            return Vec::new();
        };
        let (s_line, e_line) = self.normalize_line_selection(start.line, end.line);
        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();
        let mut edits = Vec::new();
        let mut content = String::new();
        for _ in s_line..=e_line {
            let deleted = self.engine.buffer.delete_line(s_line);
            if deleted.is_empty() {
                continue;
            }
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
        self.engine.register.set(register, &content);
        self.engine.state.cursor.line = s_line.min(self.engine.buffer.line_count()).max(1);
        self.engine.state.cursor.col = 0;
        self.on_buffer_modified();
        edits
    }

    /// Visual `p` / `P`: replace the selection with the register contents.
    pub(super) fn visual_put(&mut self, _before: bool) {
        let register = self.visual_register();
        let content = self.engine.register.get(register);
        if content.is_empty() {
            // Vim leaves Visual mode even when the register is empty.
            self.exit_visual(Mode::Normal);
            return;
        }
        let kind = self.engine.register.kind(register);
        let Some((start, _)) = self.selection_bounds() else {
            return;
        };
        let mod_count = self.engine.buffer.modification_count();
        let cursor_before = self.engine.state.cursor;
        let mut edits = self.visual_delete_collect(register);
        let insert_at = Position {
            line: start.line.min(self.engine.buffer.line_count()).max(1),
            col: start.col.min(self.line_content_len(start.line)),
        };

        let linewise = content.ends_with('\n');
        if kind == crate::register::RegisterKind::Block {
            let mut block_edits = self.insert_block_lines(
                insert_at.line,
                insert_at.col,
                &content,
                cursor_before,
                mod_count,
            );
            edits.append(&mut block_edits);
            self.engine.state.cursor = insert_at;
            self.push_undo_group(edits);
            self.on_buffer_modified();
            self.exit_visual(Mode::Normal);
            return;
        }
        let (edit_line, edit_col) = if linewise {
            let line = insert_at.line;
            self.engine.buffer.insert(line, 0, &content);
            self.engine.state.cursor.line = line.min(self.engine.buffer.line_count());
            self.engine.state.cursor.col = self.first_non_blank_col(self.engine.state.cursor.line);
            (line, 0)
        } else {
            self.engine
                .buffer
                .insert(insert_at.line, insert_at.col, &content);
            let pasted = content.chars().count();
            self.engine.state.cursor.line = insert_at.line;
            self.engine.state.cursor.col = insert_at.col + pasted.saturating_sub(1);
            (insert_at.line, insert_at.col)
        };
        edits.push(Edit {
            edit_type: EditType::Insert {
                line: edit_line,
                col: edit_col,
                text: content,
            },
            cursor_before,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.push_undo_group(edits);
        self.on_buffer_modified();
        self.exit_visual(Mode::Normal);
    }

    pub(super) fn visual_join(&mut self, with_space: bool) {
        let count = self.visual_count();
        let Some((start, end)) = self.selection_bounds() else {
            return;
        };
        let (s, e) = self.normalize_line_selection(start.line, end.line);
        let joins = (e.saturating_sub(s) + 1).saturating_sub(1) * count.max(1);
        self.engine.state.cursor.line = s;
        for _ in 0..joins {
            if with_space {
                self.join_lines();
            } else {
                self.join_lines_without_space();
            }
        }
        self.exit_visual(Mode::Normal);
    }

    /// `gJ`: join lines without inserting the separating space.
    pub(super) fn join_lines_without_space(&mut self) {
        let line = self.engine.state.cursor.line;
        if line >= self.engine.buffer.content_line_count() {
            return;
        }
        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();
        let join_pos = self.engine.buffer.merge_with_prev_line(line + 1);
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Merge {
                line,
                deleted_newline_col: join_pos,
            },
            cursor_before,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.engine.state.cursor.col = join_pos;
        self.on_buffer_modified();
    }
}

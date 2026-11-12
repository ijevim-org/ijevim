use crate::editor::Editor;
use crate::types::DotAction;
use crate::types::{Mode, PluginEvent, Position};
use crate::undo::{Edit, EditType};
use crossterm::event::KeyCode;

impl Editor {
    /// Enter Insert mode after a `c{motion}` deletion.
    pub(crate) fn enter_insert_after_change(&mut self) {
        self.engine.insert_state.accum.clear();
        self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
        let prev_mode = self.engine.state.mode;
        self.engine.state.mode = Mode::Insert;
        self.engine.plugin_manager.emit(PluginEvent::ModeChange {
            from: prev_mode,
            to: Mode::Insert,
        });
        self.needs_render = true;
    }

    /// Enter Insert mode at `pos`, which is where a charwise change starts.
    ///
    /// Unlike Normal mode, Insert mode may rest one column past the last
    /// character (e.g. after `cw` at the end of a line).
    /// Snapshot the undo position before a change's deletion.
    pub(crate) fn undo_marker(&self) -> usize {
        self.engine.undo_manager.current_index()
    }

    /// Move the deletion pushed since `marker` into the Insert-mode change, so
    /// `u` reverts the delete and the typed text together.
    pub(crate) fn defer_undo_since(&mut self, marker: usize) {
        if self.engine.undo_manager.current_index() != marker {
            if let Some(edit) = self.engine.undo_manager.pop_last() {
                self.engine.insert_state.pending_edits.push(edit);
            }
        }
    }

    pub(crate) fn enter_insert_at(&mut self, pos: Position) {
        let line = pos.line.clamp(1, self.engine.buffer.line_count().max(1));
        let col = pos.col.min(self.line_content_len(line));
        self.engine.state.cursor = Position { line, col };
        self.enter_insert_after_change();
    }

    pub(super) async fn paste(&mut self, before: bool, count: usize) {
        let register = match self.engine.operator_state.register.take() {
            Some(c) if c != '\0' => c,
            _ => '"',
        };
        let content = self.engine.register.get(register);
        if content.is_empty() {
            return;
        }
        if self.engine.register.kind(register) == crate::register::RegisterKind::Block {
            self.paste_block(register, &content, before, count);
            return;
        }
        // `3p` pastes three copies as one change, so a single `u` reverts it.
        let content = content.repeat(count.max(1));

        let mod_count = self.engine.buffer.modification_count();
        let cursor_before = self.engine.state.cursor;
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;

        // A trailing newline marks a linewise register. Pasting it inserts
        // whole lines above or below the cursor line; Vim never drops it at
        // the cursor column.
        let (edit_line, edit_col) = if content.ends_with('\n') {
            let insert_at = if before { line } else { line + 1 };
            self.engine.buffer.insert(insert_at, 0, &content);
            self.engine.state.cursor.line = insert_at.min(self.engine.buffer.line_count());
            self.engine.state.cursor.col = self.first_non_blank_col(self.engine.state.cursor.line);
            (insert_at, 0)
        } else {
            let at = if before {
                col
            } else {
                (col + 1).min(self.line_content_len(line))
            };
            self.engine.buffer.insert(line, at, &content);
            let pasted = content.chars().count();
            self.engine.state.cursor.line = line;
            self.engine.state.cursor.col = at + pasted.saturating_sub(1);
            (line, at)
        };

        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Insert {
                line: edit_line,
                col: edit_col,
                text: content,
            },
            cursor_before,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.on_buffer_modified();
    }

    /// Blockwise `p` / `P`: paste each register line at the same column,
    /// padding short lines with spaces like Vim.
    fn paste_block(&mut self, register: char, content: &str, before: bool, count: usize) {
        let line = self.engine.state.cursor.line;
        let base_col = self.engine.state.cursor.col;
        let width = self.engine.register.block_width(register).max(1);
        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();
        let mut edits = Vec::new();
        let mut col = if before { base_col } else { base_col + 1 };
        for _ in 0..count.max(1) {
            edits.extend(self.insert_block_lines(line, col, content, cursor_before, mod_count));
            col += width;
        }
        self.engine.state.cursor.line = line;
        self.engine.state.cursor.col = (if before { base_col } else { base_col + 1 })
            .min(self.engine.buffer.line_content_len(line));
        self.push_undo_group(edits);
        self.on_buffer_modified();
    }

    /// Insert a rectangular block of text (`content` split on newlines) at
    /// `col` on `line` and following lines. Returns the undo edits.
    pub(crate) fn insert_block_lines(
        &mut self,
        line: usize,
        col: usize,
        content: &str,
        cursor_before: crate::types::Position,
        mod_count: usize,
    ) -> Vec<Edit> {
        let mut edits = Vec::new();
        for (target, seg) in (line..).zip(content.split('\n')) {
            let len = self.engine.buffer.line_content_len(target);
            let at = col.min(len);
            let text = format!("{}{}", " ".repeat(col.saturating_sub(len)), seg);
            self.engine.buffer.insert(target, at, &text);
            edits.push(Edit {
                edit_type: EditType::Insert {
                    line: target,
                    col: at,
                    text,
                },
                cursor_before,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
        }
        edits
    }

    /// Toggle case of character under cursor (`~`).
    pub(crate) fn toggle_case_char(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let line_str = self.engine.buffer.get_line(line);
        let chars: Vec<char> = line_str.chars().collect();
        if col >= chars.len() {
            return;
        }
        let ch = chars[col];
        let toggled = if ch.is_uppercase() {
            ch.to_lowercase().next().unwrap_or(ch)
        } else if ch.is_lowercase() {
            ch.to_uppercase().next().unwrap_or(ch)
        } else {
            return;
        };
        let char_start = self.engine.buffer.line_to_char(line.saturating_sub(1)) + col;
        self.engine.buffer.remove_range(char_start, char_start + 1);
        self.engine.buffer.insert_char(line, col, toggled);
        let mod_count = self.engine.buffer.modification_count();
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Replace {
                line,
                col,
                old_text: ch.to_string(),
                new_text: toggled.to_string(),
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.on_buffer_modified();
    }
    pub(crate) fn delete_char(&mut self, register: char) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let line_str = self.engine.buffer.get_line(line);
        let mod_count = self.engine.buffer.modification_count();

        if col >= line_str.chars().count() {
            // `x` at the end of a line does not delete the line break; only
            // `J`, `dd`, or `X` join lines.
        } else {
            let deleted_text: String = line_str[col..].chars().take(1).collect();
            self.engine.buffer.delete(line, col);
            // Deleting the last character leaves the cursor one past the end;
            // Normal mode keeps it on the new last character.
            let max_col = self.engine.buffer.line_max_col(line);
            self.engine.state.cursor.col = self.engine.state.cursor.col.min(max_col);
            self.engine.register.set(register, &deleted_text);
            self.engine.undo_manager.push(Edit {
                edit_type: EditType::Delete {
                    line,
                    col,
                    text: deleted_text.clone(),
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
            self.engine.dot_last_action = Some(DotAction::Delete {
                text: String::new(),
                line: 0,
                col: 0,
            });
        }

        self.on_buffer_modified();
    }
    pub(super) async fn repeat_last_action(&mut self) {
        if let Some(action) = &self.engine.dot_last_action {
            match action.clone() {
                DotAction::Insert { text } => {
                    for ch in text.chars() {
                        self.engine.buffer.insert_char(
                            self.engine.state.cursor.line,
                            self.engine.state.cursor.col,
                            ch,
                        );
                        self.engine.state.cursor.col += 1;
                    }
                    self.on_buffer_modified();
                }
                DotAction::Delete {
                    text: _,
                    line: _,
                    col: _,
                } => {
                    // Re-delete one char at current cursor position.
                    self.delete_char('"');
                    self.on_buffer_modified();
                }
                DotAction::Change { text, line, col } => {
                    self.engine.buffer.insert(line, col, &text);
                    self.engine.state.cursor.line = line;
                    self.engine.state.cursor.col = col;
                    self.on_buffer_modified();
                }
                DotAction::Replace { ch } => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let line_str = self.engine.buffer.get_line(line);
                    if col < line_str.chars().count() {
                        let start = self.engine.buffer.line_to_char(line.saturating_sub(1)) + col;
                        let end = start + 1;
                        self.engine.buffer.remove_range(start, end);
                        self.engine.buffer.insert_char(line, col, ch);
                        self.on_buffer_modified();
                    }
                }
                DotAction::Join => {
                    self.join_lines();
                    self.on_buffer_modified();
                }
                DotAction::Indent { unindent } => {
                    let line = self.engine.state.cursor.line;
                    self.indent_lines(line, line, !unindent);
                    self.on_buffer_modified();
                }
                DotAction::ToggleCase => {
                    self.toggle_case_char();
                    self.needs_render = true;
                    self.on_buffer_modified();
                }
            }
        }
    }
    pub(crate) fn open_line(&mut self, above: bool) {
        let line = self.engine.state.cursor.line;
        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();

        let insert_line = if above { line } else { line + 1 };
        self.engine.buffer.insert(insert_line, 0, "\n");
        self.engine.state.cursor.line = insert_line;
        self.engine.state.cursor.col = 0;

        // The newline is part of the same change as the text typed next, so it
        // is committed together at `Esc`. Without this, undo leaves the opened
        // line (or removes text from the wrong place).
        self.engine.insert_state.pending_edits.push(Edit {
            edit_type: EditType::Insert {
                line: insert_line,
                col: 0,
                text: "\n".to_string(),
            },
            cursor_before,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.engine.insert_state.accum.clear();
        self.engine.insert_state.start_pos = Some(self.engine.state.cursor);

        let prev_mode = self.engine.state.mode;
        self.engine.state.mode = Mode::Insert;
        self.engine.plugin_manager.emit(PluginEvent::ModeChange {
            from: prev_mode,
            to: Mode::Insert,
        });

        self.engine.dot_last_action = Some(DotAction::Insert {
            text: String::new(),
        });
        self.on_buffer_modified();
    }
    pub(crate) fn join_lines(&mut self) {
        let line = self.engine.state.cursor.line;
        if line >= self.engine.buffer.content_line_count() {
            return;
        }

        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();
        let current_line = self.engine.buffer.get_line(line);
        let current_stripped = current_line.trim_end_matches('\n');
        let next_line = self.engine.buffer.get_line(line + 1);
        let next_stripped = next_line.trim_end_matches('\n');
        let next_trimmed = next_stripped.trim_start();

        let join_pos = self.engine.buffer.merge_with_prev_line(line + 1);
        let mut edits = vec![Edit {
            edit_type: EditType::Merge {
                line,
                deleted_newline_col: join_pos,
            },
            cursor_before,
            cursor_after: cursor_before,
            modification_count: mod_count,
        }];

        // Vim drops the indentation of the joined line.
        let leading = next_stripped.chars().count() - next_trimmed.chars().count();
        if leading > 0 {
            let start = self.engine.buffer.line_to_char(line.saturating_sub(1)) + join_pos;
            let removed: String = next_stripped.chars().take(leading).collect();
            self.engine.buffer.remove_range(start, start + leading);
            edits.push(Edit {
                edit_type: EditType::Delete {
                    line,
                    col: join_pos,
                    text: removed,
                },
                cursor_before,
                cursor_after: cursor_before,
                modification_count: mod_count,
            });
        }

        // A single space separates the halves unless one side provides it.
        let needs_space = !current_stripped.ends_with(' ')
            && !current_stripped.ends_with('\t')
            && !next_trimmed.is_empty();
        if needs_space {
            self.engine.buffer.insert(line, join_pos, " ");
            edits.push(Edit {
                edit_type: EditType::Insert {
                    line,
                    col: join_pos,
                    text: " ".to_string(),
                },
                cursor_before,
                cursor_after: cursor_before,
                modification_count: mod_count,
            });
        }

        self.push_undo_group(edits);

        // Vim leaves the cursor on the join point, clamped to the line end.
        self.engine.state.cursor.col = join_pos.min(self.engine.buffer.line_max_col(line));
        self.engine.dot_last_action = Some(DotAction::Join);
        self.on_buffer_modified();
    }
    pub(super) async fn handle_ctrl_w(&mut self, key: KeyCode) {
        use crate::types::Direction;
        match key {
            KeyCode::Char('w') => {
                // Ctrl-w w: next window
                if let Err(e) = self.engine.focus_next_window() {
                    self.engine
                        .state
                        .push_message(crate::types::MessageLevel::Error, e);
                }
            }
            KeyCode::Char('j') => {
                // Ctrl-w j: window below
                self.engine.focus_window_direction(Direction::Down);
            }
            KeyCode::Char('k') => {
                // Ctrl-w k: window above
                self.engine.focus_window_direction(Direction::Up);
            }
            KeyCode::Char('h') => {
                // Ctrl-w h: window left
                self.engine.focus_window_direction(Direction::Left);
            }
            KeyCode::Char('l') => {
                // Ctrl-w l: window right
                self.engine.focus_window_direction(Direction::Right);
            }
            KeyCode::Char('c') => {
                // Ctrl-w c: close window
                if let Err(e) = self.engine.close_focused_window() {
                    self.engine
                        .state
                        .push_message(crate::types::MessageLevel::Error, e);
                }
            }
            KeyCode::Char('=') => {
                // Ctrl-w =: equalize windows
                let rows = self.terminal.rows() as usize;
                let cols = self.terminal.cols() as usize;
                self.engine.equalize_windows(rows, cols);
            }
            KeyCode::Char('_') => {
                // Ctrl-w _: maximize vertically
                let rows = self.terminal.rows() as usize;
                let cols = self.terminal.cols() as usize;
                self.engine.maximize_vertical(rows, cols);
            }
            KeyCode::Char('|') => {
                // Ctrl-w |: maximize horizontally
                let rows = self.terminal.rows() as usize;
                let cols = self.terminal.cols() as usize;
                self.engine.maximize_horizontal(rows, cols);
            }
            KeyCode::Char('s') => {
                // Ctrl-w s: split horizontal (same as :split)
                if let Err(e) = self.engine.split_horizontal() {
                    self.engine
                        .state
                        .push_message(crate::types::MessageLevel::Error, e);
                }
            }
            KeyCode::Char('v') => {
                // Ctrl-w v: split vertical (same as :vsplit)
                if let Err(e) = self.engine.split_vertical() {
                    self.engine
                        .state
                        .push_message(crate::types::MessageLevel::Error, e);
                }
            }
            _ => {
                // Unknown Ctrl-w subcommand
                self.engine.state.push_message(
                    crate::types::MessageLevel::Warning,
                    format!("Unknown Ctrl-w subcommand: {:?}", key),
                );
            }
        }
        self.needs_render = true;
    }
}

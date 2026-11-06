use crate::editor::Editor;
use crate::types::{DotAction, Mode, Position};
use crate::undo::{Edit, EditType};
use crossterm::event::KeyCode;

impl Editor {
    pub(crate) async fn handle_insert(&mut self, key: KeyCode) {
        // Check for pending register insertion (Ctrl-r in insert mode).
        if self.engine.insert_state.waiting_register {
            self.engine.insert_state.waiting_register = false;
            match key {
                KeyCode::Char(c) => {
                    let register = if c == '"' { '"' } else { c };
                    let content = self.engine.register.get(register);
                    if !content.is_empty() {
                        let line = self.engine.state.cursor.line;
                        let col = self.engine.state.cursor.col;
                        self.engine.buffer.insert(line, col, &content);
                        self.engine.state.cursor.col += content.chars().count();
                        self.engine.insert_state.accum.push_str(&content);
                        self.on_buffer_modified();
                    }
                    return;
                }
                KeyCode::Esc => {
                    self.needs_render = true;
                    return;
                }
                _ => return,
            }
        }

        match key {
            KeyCode::Esc => {
                // Push accumulated insert text as a single undo group
                if !self.engine.insert_state.accum.is_empty() {
                    let text = std::mem::take(&mut self.engine.insert_state.accum);
                    let cursor_before = self
                        .engine
                        .insert_state
                        .start_pos
                        .unwrap_or(crate::types::Position { line: 1, col: 0 });
                    self.engine.insert_state.start_pos = None;
                    let mod_count = self.engine.buffer.modification_count();
                    self.engine.undo_manager.push(crate::undo::Edit {
                        edit_type: crate::undo::EditType::Insert {
                            line: cursor_before.line,
                            col: cursor_before.col,
                            text,
                        },
                        cursor_before,
                        cursor_after: self.engine.state.cursor,
                        modification_count: mod_count,
                    });
                }
                self.engine.insert_state.start_pos = None;
                self.transition_mode(Mode::Normal);
                self.engine.state.cursor.col = self.engine.state.cursor.col.saturating_sub(1);
                self.needs_render = true;
            }
            KeyCode::Backspace => {
                if self.engine.state.cursor.col > 0 {
                    self.engine.buffer.delete(
                        self.engine.state.cursor.line,
                        self.engine.state.cursor.col - 1,
                    );
                    self.engine.state.cursor.col -= 1;
                    self.engine.insert_state.accum.pop();
                    self.on_buffer_modified();
                } else if self.engine.state.cursor.line > 1 {
                    let new_col = self
                        .engine
                        .buffer
                        .merge_with_prev_line(self.engine.state.cursor.line);
                    self.engine.state.cursor.line -= 1;
                    self.engine.state.cursor.col = new_col;
                    // Remove the last newline from accumulator
                    self.engine.insert_state.accum.pop();
                    self.on_buffer_modified();
                }
            }
            KeyCode::Enter => {
                self.engine.buffer.insert(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col,
                    "\n",
                );
                self.engine.state.cursor.line += 1;
                self.engine.state.cursor.col = 0;
                self.engine.insert_state.accum.push('\n');
                self.on_buffer_modified();
            }
            KeyCode::Tab => {
                let text = if self.engine.state.expandtab {
                    " ".repeat(self.engine.state.shiftwidth)
                } else {
                    "\t".to_string()
                };
                self.engine.buffer.insert(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col,
                    &text,
                );
                self.engine.state.cursor.col += text.chars().count();
                self.engine.insert_state.accum.push_str(&text);
                self.on_buffer_modified();
            }
            KeyCode::Char(c) => {
                self.engine.buffer.insert_char(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col,
                    c,
                );
                self.engine.state.cursor.col += 1;
                self.engine.insert_state.accum.push(c);
                self.on_buffer_modified();
            }
            _ => {}
        }
    }

    /// Delete from cursor to beginning of the current word (Ctrl-w in Insert mode).
    pub(crate) fn insert_delete_word_backward(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self
            .engine
            .state
            .cursor
            .col
            .min(self.engine.buffer.line_content_len(line));
        if col == 0 {
            return;
        }

        // Work in character indices throughout. Slicing the line as a `String`
        // with a character column would panic on multi-byte input.
        let chars = self.engine.buffer.line_chars(line);
        let mut start = col;
        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        if start == col {
            return;
        }

        self.engine.buffer.delete_range(line, start, line, col);
        self.engine.state.cursor.col = start;
        // Remove deleted chars from accumulator
        for _ in 0..(col - start) {
            self.engine.insert_state.accum.pop();
        }
        self.on_buffer_modified();
    }

    /// Delete from cursor to beginning of line (Ctrl-u in Insert mode).
    pub(crate) fn insert_delete_to_bol(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        if col > 0 {
            let line_str = self.engine.buffer.get_line(line);
            let deleted: String = line_str.chars().take(col).collect();
            self.engine.buffer.delete_range(line, 0, line, col);
            self.engine.state.cursor.col = 0;
            // Remove deleted chars from accumulator
            for _ in deleted.chars() {
                self.engine.insert_state.accum.pop();
            }
            self.on_buffer_modified();
        }
    }

    /// Handle the single-char replace (r{char}) synchronously.
    /// Extracted to break async recursion — Enter delegates here.
    fn handle_replace_inline_char(&mut self, c: char) {
        let count = self.engine.operator_state.count.take().unwrap_or(1);
        let line = self.engine.state.cursor.line;
        let mut col = self.engine.state.cursor.col;

        // Collect the text being replaced (for undo).
        let mut old_text = String::new();
        for _ in 0..count {
            let line_str = self.engine.buffer.get_line(line);
            if col < line_str.chars().count() {
                old_text.push(line_str[col..].chars().next().unwrap());
                col += 1;
            }
        }

        // Vim aborts `{count}r` when the line has fewer characters left:
        // `5r!` on a two-character line changes nothing at all.
        let available = self.engine.buffer.line_content_len(line) - self.engine.state.cursor.col;
        if count > available {
            self.engine.operator_state.replace_char = None;
            self.transition_mode(Mode::Normal);
            self.needs_render = true;
            return;
        }

        // `3r!` cannot extend a line: only the characters that exist are
        // replaced, so the line length stays the same.
        let replaced = old_text.chars().count();
        if replaced == 0 {
            self.engine.operator_state.replace_char = None;
            self.transition_mode(Mode::Normal);
            self.needs_render = true;
            return;
        }

        // Perform the replace — delete old, insert new.
        let start_col = self.engine.state.cursor.col;
        let start_char = self.engine.buffer.line_to_char(line.saturating_sub(1)) + start_col;
        let end_char = start_char + replaced;
        self.engine.buffer.remove_range(start_char, end_char);
        let new_text: String = (0..replaced).map(|_| c).collect();
        self.engine
            .buffer
            .insert(self.engine.state.cursor.line, start_col, &new_text);
        // Vim leaves the cursor on the last replaced character.
        self.engine.state.cursor.col = start_col + replaced - 1;

        // Push undo entry.
        let mod_count = self.engine.buffer.modification_count();
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Replace {
                line,
                col: self.engine.state.cursor.col,
                old_text,
                new_text,
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });

        // Store for dot-repeat (`.`).
        self.engine.dot_last_action = Some(DotAction::Replace { ch: c });

        self.engine.operator_state.replace_char = None;
        self.transition_mode(Mode::Normal);
        self.on_buffer_modified();
    }

    pub(crate) async fn handle_replace(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                if self.engine.operator_state.replace_char == Some('\0')
                    && self.engine.state.visual_start.is_some()
                {
                    self.engine.operator_state.replace_char = None;
                    self.engine.state.visual_start = None;
                    self.engine.state.visual_type = None;
                    self.transition_mode(Mode::Normal);
                    self.needs_render = true;
                    return;
                }
                // Flush accumulated Replace mode edits as a single undo group.
                if !self.engine.replace_undo_group.is_empty() {
                    let edits = std::mem::take(&mut self.engine.replace_undo_group);
                    let mod_count = self.engine.buffer.modification_count();
                    self.engine.undo_manager.push(Edit {
                        edit_type: EditType::Group { edits },
                        cursor_before: self.engine.state.cursor,
                        cursor_after: self.engine.state.cursor,
                        modification_count: mod_count,
                    });
                }
                self.engine.operator_state.replace_char = None;
                self.transition_mode(Mode::Normal);
                self.needs_render = true;
            }
            KeyCode::Char(c) => {
                // ── Single-char replace (r{char}) ─────────────────
                if self.engine.operator_state.replace_char == Some('\0') {
                    if self.engine.state.visual_start.is_some() {
                        return self.visual_replace_char(c);
                    }
                    return self.handle_replace_inline_char(c);
                }

                // ── Multi-char Replace mode (R) ───────────────────
                let line = self.engine.state.cursor.line;
                let col = self.engine.state.cursor.col;
                let old_char = if col < self.engine.buffer.line_content_len(line) {
                    let line_str = self.engine.buffer.get_line(line);
                    line_str.chars().nth(col).unwrap_or('\0')
                } else {
                    '\0'
                };
                if old_char != '\0' {
                    self.engine.buffer.delete(line, col);
                }
                self.engine.buffer.insert_char(line, col, c);
                self.engine.state.cursor.col += 1;
                // Record the replace for undo grouping.
                let mod_count = self.engine.buffer.modification_count();
                self.engine.replace_undo_group.push(Edit {
                    edit_type: EditType::Replace {
                        line,
                        col,
                        old_text: old_char.to_string(),
                        new_text: c.to_string(),
                    },
                    cursor_before: Position { line, col },
                    cursor_after: self.engine.state.cursor,
                    modification_count: mod_count,
                });
                self.on_buffer_modified();
            }
            KeyCode::Backspace => {
                if self.engine.state.cursor.col > 0 {
                    self.engine.state.cursor.col -= 1;
                }
                self.needs_render = true;
            }
            KeyCode::Enter => {
                // Enter during single-char replace acts like Char('\n')
                if self.engine.operator_state.replace_char == Some('\0') {
                    if self.engine.state.visual_start.is_some() {
                        return self.visual_replace_char('\n');
                    }
                    return self.handle_replace_inline_char('\n');
                }
                let line = self.engine.state.cursor.line;
                let col = self.engine.state.cursor.col;
                let mod_count = self.engine.buffer.modification_count();
                self.engine.buffer.insert(line, col, "\n");
                self.engine.state.cursor.line += 1;
                self.engine.state.cursor.col = 0;
                self.engine.replace_undo_group.push(Edit {
                    edit_type: EditType::Split { line, col },
                    cursor_before: Position { line, col },
                    cursor_after: self.engine.state.cursor,
                    modification_count: mod_count,
                });
                self.on_buffer_modified();
            }
            _ => {}
        }
    }
}

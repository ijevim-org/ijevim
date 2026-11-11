use crate::editor::Editor;
use crate::types::{Mode, Operator, PendingOperator, PluginEvent};
use crossterm::event::KeyCode;

impl Editor {
    pub(crate) async fn handle_operator(&mut self, op: Operator, key: KeyCode, count: usize) {
        let register = match self.engine.operator_state.register.take() {
            Some(c) if c != '\0' => c,
            _ => '"',
        };

        // `g` is a motion prefix: inside operator-pending it can only be
        // `gg`, so wait for the second key instead of dropping the operator.
        if self.engine.operator_state.pending_g {
            self.engine.operator_state.pending_g = false;
            if let KeyCode::Char('g') = key {
                let line = self.engine.state.cursor.line;
                match op {
                    Operator::Delete => self.delete_lines_range(register, 1, line),
                    Operator::Yank => self.yank_lines_range(register, 1, line),
                    Operator::Change => self.change_line_range(register, 1, line),
                    _ => {}
                }
            }
            self.needs_render = true;
            return;
        }

        match op {
            Operator::Yank => match key {
                KeyCode::Char('y') => {
                    let line = self.engine.state.cursor.line;
                    self.yank_lines_range(register, line, line + count.max(1) - 1);
                }
                KeyCode::Char('w') => {
                    self.yank_word(register, count, false);
                }
                KeyCode::Char('W') => {
                    self.yank_word(register, count, true);
                }
                KeyCode::Char('$') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    self.yank_cols(register, col, self.line_content_len(line));
                }
                KeyCode::Char('0') => {
                    let col = self.engine.state.cursor.col;
                    self.yank_cols(register, 0, col);
                }
                KeyCode::Char('^') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let start = self.first_non_blank_col(line);
                    self.yank_cols(register, start, col);
                }
                KeyCode::Char('e') => {
                    self.yank_word_end(register, count, false);
                }
                KeyCode::Char('l') | KeyCode::Char(' ') => {
                    let col = self.engine.state.cursor.col;
                    let end = col + count.max(1);
                    self.yank_cols(register, col, end);
                }
                KeyCode::Char('h') => {
                    let col = self.engine.state.cursor.col;
                    self.yank_cols(register, col.saturating_sub(count.max(1)), col);
                }
                KeyCode::Char('E') => {
                    self.yank_word_end(register, count, true);
                }
                KeyCode::Char('b') => {
                    let from = self.word_backward_target(count, false);
                    let to = (self.engine.state.cursor.line, self.engine.state.cursor.col);
                    self.yank_span(register, from, to);
                }
                KeyCode::Char('B') => {
                    let from = self.word_backward_target(count, true);
                    let to = (self.engine.state.cursor.line, self.engine.state.cursor.col);
                    self.yank_span(register, from, to);
                }
                KeyCode::Char('j') => {
                    // `j` fails on the last line, so the operator does nothing.
                    let line = self.engine.state.cursor.line;
                    if line < self.engine.buffer.content_line_count() {
                        self.yank_lines_range(register, line, line + count.max(1));
                    }
                }
                KeyCode::Char('k') => {
                    let line = self.engine.state.cursor.line;
                    if line > 1 {
                        let start = line.saturating_sub(count.max(1)).max(1);
                        self.yank_lines_range(register, start, line);
                    }
                }
                KeyCode::Char('G') => {
                    let line = self.engine.state.cursor.line;
                    self.yank_lines_range(register, line, self.engine.buffer.content_line_count());
                }
                KeyCode::Char('i') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                        op: Operator::Yank,
                        register,
                        inner: true,
                    };
                }
                KeyCode::Char('a') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                        op: Operator::Yank,
                        register,
                        inner: false,
                    };
                }
                _ => {}
            },
            Operator::Delete => match key {
                KeyCode::Char('d') => {
                    // `3dd` is a single undo step in Vim.
                    let line = self.engine.state.cursor.line;
                    self.delete_lines_range(register, line, line + count.max(1) - 1);
                }
                KeyCode::Char('w') => {
                    self.delete_word(register, count, false);
                }
                KeyCode::Char('W') => {
                    self.delete_word(register, count, true);
                }
                KeyCode::Char('$') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    self.delete_cols(register, col, self.line_content_len(line));
                }
                KeyCode::Char('0') => {
                    let col = self.engine.state.cursor.col;
                    self.delete_cols(register, 0, col);
                }
                KeyCode::Char('^') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let start = self.first_non_blank_col(line);
                    self.delete_cols(register, start, col);
                }
                KeyCode::Char('e') => {
                    self.delete_word_end(register, count, false);
                }
                KeyCode::Char('l') | KeyCode::Char(' ') => {
                    let col = self.engine.state.cursor.col;
                    self.delete_cols(register, col, col + count.max(1));
                }
                KeyCode::Char('h') => {
                    let col = self.engine.state.cursor.col;
                    self.delete_cols(register, col.saturating_sub(count.max(1)), col);
                }
                KeyCode::Char('E') => {
                    self.delete_word_end(register, count, true);
                }
                KeyCode::Char('b') => {
                    let from = self.word_backward_target(count, false);
                    let to = (self.engine.state.cursor.line, self.engine.state.cursor.col);
                    self.delete_span(register, from, to);
                }
                KeyCode::Char('B') => {
                    let from = self.word_backward_target(count, true);
                    let to = (self.engine.state.cursor.line, self.engine.state.cursor.col);
                    self.delete_span(register, from, to);
                }
                KeyCode::Char('j') => {
                    // `j` fails on the last line, so the operator does nothing.
                    let line = self.engine.state.cursor.line;
                    if line < self.engine.buffer.content_line_count() {
                        self.delete_lines_range(register, line, line + count.max(1));
                    }
                }
                KeyCode::Char('k') => {
                    let line = self.engine.state.cursor.line;
                    if line > 1 {
                        let start = line.saturating_sub(count.max(1)).max(1);
                        self.delete_lines_range(register, start, line);
                    }
                }
                KeyCode::Char('G') => {
                    let line = self.engine.state.cursor.line;
                    self.delete_lines_range(
                        register,
                        line,
                        self.engine.buffer.content_line_count(),
                    );
                }
                KeyCode::Char('i') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                        op: Operator::Delete,
                        register,
                        inner: true,
                    };
                }
                KeyCode::Char('a') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                        op: Operator::Delete,
                        register,
                        inner: false,
                    };
                }
                _ => {}
            },
            Operator::Change => match key {
                KeyCode::Char('c') => {
                    // `cc` clears the line but keeps one empty line.
                    self.change_lines(register, count);
                }
                KeyCode::Char('w') => {
                    self.change_word(register, count, false);
                }
                KeyCode::Char('W') => {
                    self.change_word(register, count, true);
                }
                KeyCode::Char('e') => {
                    self.delete_word_end(register, count, false);
                    self.enter_insert_after_change();
                }
                KeyCode::Char('l') | KeyCode::Char(' ') => {
                    let col = self.engine.state.cursor.col;
                    self.delete_cols(register, col, col + count.max(1));
                    self.enter_insert_after_change();
                }
                KeyCode::Char('h') => {
                    let col = self.engine.state.cursor.col;
                    self.delete_cols(register, col.saturating_sub(count.max(1)), col);
                    self.enter_insert_after_change();
                }
                KeyCode::Char('E') => {
                    self.delete_word_end(register, count, true);
                    self.enter_insert_after_change();
                }
                KeyCode::Char('b') => {
                    let from = self.word_backward_target(count, false);
                    let to = (self.engine.state.cursor.line, self.engine.state.cursor.col);
                    self.delete_span(register, from, to);
                    self.enter_insert_after_change();
                }
                KeyCode::Char('B') => {
                    let from = self.word_backward_target(count, true);
                    let to = (self.engine.state.cursor.line, self.engine.state.cursor.col);
                    self.delete_span(register, from, to);
                    self.enter_insert_after_change();
                }
                KeyCode::Char('$') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    self.delete_cols(register, col, self.line_content_len(line));
                    self.enter_insert_after_change();
                }
                KeyCode::Char('0') => {
                    let col = self.engine.state.cursor.col;
                    self.delete_cols(register, 0, col);
                    self.enter_insert_after_change();
                }
                KeyCode::Char('^') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let start = self.first_non_blank_col(line);
                    self.delete_cols(register, start, col);
                    self.enter_insert_after_change();
                }
                KeyCode::Char('j') => {
                    let line = self.engine.state.cursor.line;
                    if line < self.engine.buffer.content_line_count() {
                        self.change_line_range(register, line, line + count.max(1));
                    }
                }
                KeyCode::Char('k') => {
                    let line = self.engine.state.cursor.line;
                    if line > 1 {
                        let start = line.saturating_sub(count.max(1)).max(1);
                        self.change_line_range(register, start, line);
                    }
                }
                KeyCode::Char('G') => {
                    let line = self.engine.state.cursor.line;
                    self.change_line_range(register, line, self.engine.buffer.content_line_count());
                }
                KeyCode::Char('i') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                        op: Operator::Change,
                        register,
                        inner: true,
                    };
                }
                KeyCode::Char('a') => {
                    self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                        op: Operator::Change,
                        register,
                        inner: false,
                    };
                }
                _ => {}
            },
            Operator::IndentRight => {
                if let KeyCode::Char('>') = key {
                    let line = self.engine.state.cursor.line;
                    self.indent_lines(line, line + count.max(1) - 1, true);
                }
            }
            Operator::IndentLeft => {
                if let KeyCode::Char('<') = key {
                    let line = self.engine.state.cursor.line;
                    self.indent_lines(line, line + count.max(1) - 1, false);
                }
            }
            Operator::FindCharForward => {
                if let KeyCode::Char(ch) = key {
                    self.engine.search_state.last_fchar = Some(ch);
                    self.engine.search_state.last_fchar_till = false;
                    if self.find_char(ch, false, true) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::FindCharBackward => {
                if let KeyCode::Char(ch) = key {
                    self.engine.search_state.last_fchar = Some(ch);
                    self.engine.search_state.last_fchar_till = false;
                    if self.find_char(ch, false, false) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::TillCharForward => {
                if let KeyCode::Char(ch) = key {
                    self.engine.search_state.last_fchar = Some(ch);
                    self.engine.search_state.last_fchar_till = true;
                    if self.find_char(ch, true, true) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::TillCharBackward => {
                if let KeyCode::Char(ch) = key {
                    self.engine.search_state.last_fchar = Some(ch);
                    self.engine.search_state.last_fchar_till = true;
                    if self.find_char(ch, true, false) {
                        self.needs_render = true;
                    }
                }
            }
            Operator::RecordMacro => {
                if let KeyCode::Char(c) = key {
                    if c.is_ascii_lowercase() {
                        self.engine.state.macros.start_recording(c);
                    }
                }
            }
            Operator::Goto => {
                if let KeyCode::Char('g') = key {
                    self.engine.state.cursor.line = 1;
                    self.engine.state.cursor.col = 0;
                    self.needs_render = true;
                } else if let KeyCode::Char('e') = key {
                    for _ in 0..count {
                        self.move_word_end_backward(false);
                    }
                } else if let KeyCode::Char('E') = key {
                    for _ in 0..count {
                        self.move_word_end_backward(true);
                    }
                } else if let KeyCode::Char('-') = key {
                    // g- : switch to older undo branch
                    if let Some(parent) = self.engine.undo_manager.parent_node() {
                        if self
                            .engine
                            .undo_manager
                            .switch_branch(&mut self.engine.buffer, parent)
                            .is_some()
                        {
                            self.needs_render = true;
                        }
                    }
                } else if let KeyCode::Char('+') = key {
                    // g+ : switch to newer undo branch
                    if let Some(child) = self.engine.undo_manager.first_child_node() {
                        if self
                            .engine
                            .undo_manager
                            .switch_branch(&mut self.engine.buffer, child)
                            .is_some()
                        {
                            self.needs_render = true;
                        }
                    }
                } else if let KeyCode::Char('v') = key {
                    // gv: restore last visual selection
                    if let (Some(start), Some(vtype)) = (
                        self.engine.state.last_visual_start,
                        self.engine.state.last_visual_type,
                    ) {
                        self.engine.state.visual_start = Some(start);
                        self.engine.state.visual_type = Some(vtype);
                        self.engine.state.cursor =
                            self.engine.state.last_visual_end.unwrap_or(start);
                        let prev_mode = self.engine.state.mode;
                        self.engine.state.mode = Mode::Visual;
                        self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Visual,
                        });
                        self.needs_render = true;
                    }
                }
            }
            Operator::Scroll => {
                match key {
                    KeyCode::Char('z') => {
                        self.scroll_cursor_to_center();
                    }
                    KeyCode::Char('t') => {
                        self.scroll_cursor_to_top();
                    }
                    KeyCode::Char('b') => {
                        self.scroll_cursor_to_bottom();
                    }
                    _ => {}
                }
                self.needs_render = true;
            }
            Operator::SaveQuit => {
                if let KeyCode::Char('Z') = key {
                    self.save_file_async().await;
                    if !self.engine.buffer.is_dirty() {
                        self.running = false;
                    }
                }
                self.needs_render = true;
            }
        }
    }
}

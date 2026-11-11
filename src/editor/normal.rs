use crate::editor::Editor;
use crate::types::DotAction;
use crate::types::{Mode, Operator, PendingOperator, PluginEvent, VisualType};
use crossterm::event::KeyCode;

mod edits;
mod motions;
mod operator;
mod paste;
mod prefixes;

impl Editor {
    pub(crate) async fn handle_normal(&mut self, key: KeyCode) {
        if self.handle_ctrl_w_prefix(key).await {
            return;
        }
        if self.handle_register_prefix(key) {
            return;
        }
        if self.handle_mark_prefix(key) {
            return;
        }
        if self.handle_count_prefix(key) {
            return;
        }
        if self.handle_directory_prefix(key) {
            return;
        }
        if self.handle_pending_operator(key).await {
            return;
        }
        if self.handle_macro_prefix(key).await {
            return;
        }

        let line_count = self.engine.buffer.line_count();

        match key {
            // ── Motions with count support ───────────────────────
            KeyCode::Char('h') => {
                let count = self.take_count();
                self.engine.state.cursor.col = self.engine.state.cursor.col.saturating_sub(count);
                self.needs_render = true;
            }
            KeyCode::Char('l') => {
                let count = self.take_count();
                let max_col = self
                    .engine
                    .buffer
                    .line_max_col(self.engine.state.cursor.line);
                self.engine.state.cursor.col = (self.engine.state.cursor.col + count).min(max_col);
                self.needs_render = true;
            }
            KeyCode::Char('j') => {
                let count = self.take_count();
                self.engine.state.cursor.line =
                    (self.engine.state.cursor.line + count).min(line_count);
                let max_col = self
                    .engine
                    .buffer
                    .line_max_col(self.engine.state.cursor.line);
                self.engine.state.cursor.col = self.engine.state.cursor.col.min(max_col);
                self.needs_render = true;
            }
            KeyCode::Char('k') => {
                let count = self.take_count();
                self.engine.state.cursor.line =
                    self.engine.state.cursor.line.saturating_sub(count).max(1);
                let max_col = self
                    .engine
                    .buffer
                    .line_max_col(self.engine.state.cursor.line);
                self.engine.state.cursor.col = self.engine.state.cursor.col.min(max_col);
                self.needs_render = true;
            }
            KeyCode::Char('i') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.insert_state.accum.clear();
                self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('I') => {
                let _count = self.take_count();
                let line = self.engine.state.cursor.line;
                self.engine.state.cursor.col = self.first_non_blank_col(line);
                let prev_mode = self.engine.state.mode;
                self.engine.insert_state.accum.clear();
                self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('a') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.insert_state.accum.clear();
                self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
                if self.engine.state.cursor.col
                    < self
                        .engine
                        .buffer
                        .line_content_len(self.engine.state.cursor.line)
                {
                    self.engine.state.cursor.col += 1;
                }
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('A') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.insert_state.accum.clear();
                self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
                let line_len = self
                    .engine
                    .buffer
                    .line_content_len(self.engine.state.cursor.line);
                self.engine.state.cursor.col = line_len;
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char(':') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Command;
                self.engine.state.command_buffer.clear();
                self.engine.state.command_cursor_pos = 0;
                self.reset_command_line_nav();
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Command,
                });
                self.needs_render = true;
            }
            KeyCode::Char('0') => {
                self.engine.state.cursor.col = 0;
                self.needs_render = true;
            }
            KeyCode::Char('^') => {
                let _count = self.take_count();
                let line = self.engine.buffer.get_line(self.engine.state.cursor.line);
                let first_non_blank = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                self.engine.state.cursor.col = first_non_blank;
                self.needs_render = true;
            }
            KeyCode::Char('$') => {
                let count = self.take_count();
                let target = (self.engine.state.cursor.line + count - 1)
                    .min(self.engine.buffer.line_count());
                self.engine.state.cursor.line = target;
                self.cursor_line_end();
            }
            KeyCode::Char('D') => {
                let count = self.take_count();
                for _ in 0..count {
                    self.kill_line();
                }
            }
            KeyCode::Char('C') => {
                let count = self.take_count();
                for _ in 0..count {
                    self.kill_line();
                }
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.needs_render = true;
            }
            KeyCode::Char('S') => {
                let count = self.take_count();
                self.change_lines('"', count);
            }
            KeyCode::Char('*') => {
                let _count = self.take_count();
                self.search_word_under_cursor();
                self.needs_render = true;
            }
            KeyCode::Char('n') => {
                let count = self.take_count();
                for _ in 0..count {
                    self.search_next();
                }
                self.needs_render = true;
            }
            KeyCode::Char('N') => {
                let count = self.take_count();
                for _ in 0..count {
                    self.search_prev();
                }
                self.needs_render = true;
            }
            // `/` was missing entirely, so Vim-mode search could only be
            // reached through `?` (backwards) or the Emacs keymap.
            KeyCode::Char('/') | KeyCode::Char('?') => {
                let prev_mode = self.engine.state.mode;
                let direction = if key == KeyCode::Char('?') {
                    crate::types::SearchDirection::Backward
                } else {
                    crate::types::SearchDirection::Forward
                };
                self.begin_search_command(direction);
                self.engine.state.mode = Mode::Command;
                self.engine.state.command_buffer.clear();
                self.engine.state.command_cursor_pos = 0;
                if let KeyCode::Char(c) = key {
                    self.engine.state.command_buffer.push(c);
                    // The prefix is already typed: later keys must append
                    // after it, not at position 0.
                    self.engine.state.command_cursor_pos = self.engine.state.command_buffer.len();
                }
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Command,
                });
                self.needs_render = true;
            }
            KeyCode::Char('u') => {
                self.undo();
            }
            KeyCode::Char('r') => {
                if self.engine.state.command_buffer.is_empty() {
                    // Sentinel: single-char replace (r{char}) — wait for one char,
                    // replace, push undo, and exit back to Normal.
                    self.engine.operator_state.replace_char = Some('\0');
                    let prev_mode = self.engine.state.mode;
                    self.engine.state.mode = Mode::Replace;
                    self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Replace,
                    });
                    self.needs_render = true;
                }
            }
            // Multi-char Replace mode (R) — replace until Esc.
            KeyCode::Char('R') => {
                self.engine.operator_state.replace_char = None;
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Replace;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Replace,
                });
                self.needs_render = true;
            }
            KeyCode::Char('f') => {
                if self.engine.state.command_buffer.is_empty() {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::FindCharForward,
                        register,
                    };
                }
            }
            KeyCode::Char('F') => {
                if self.engine.state.command_buffer.is_empty() {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::FindCharBackward,
                        register,
                    };
                }
            }
            KeyCode::Char('t') => {
                if self.engine.state.command_buffer.is_empty() {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::TillCharForward,
                        register,
                    };
                }
            }
            KeyCode::Char('T') => {
                if self.engine.state.command_buffer.is_empty() {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::TillCharBackward,
                        register,
                    };
                }
            }
            KeyCode::Char(';') => {
                if self.repeat_find(true) {
                    self.needs_render = true;
                }
            }
            KeyCode::Char(',') => {
                if self.repeat_find(false) {
                    self.needs_render = true;
                }
            }
            KeyCode::Char('v') => {
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Visual;
                self.engine.state.visual_start = Some(self.engine.state.cursor);
                self.engine.state.visual_type = Some(VisualType::Character);
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Visual,
                });
                self.needs_render = true;
            }
            KeyCode::Char('V') => {
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Visual;
                self.engine.state.visual_start = Some(self.engine.state.cursor);
                self.engine.state.visual_type = Some(VisualType::Line);
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Visual,
                });
                self.needs_render = true;
            }
            KeyCode::Char('y') => {
                let register = self.engine.operator_state.register.take();
                self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                    op: Operator::Yank,
                    register,
                };
            }
            KeyCode::Char('Y') => {
                // `Y` yanks the whole line, like `yy`.
                let register = self.engine.operator_state.register.take().unwrap_or('"');
                let line = self.engine.state.cursor.line;
                let count = self.take_count();
                self.yank_lines_range(register, line, line + count.max(1) - 1);
            }
            KeyCode::Char('d') => {
                let register = self.engine.operator_state.register.take();
                self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                    op: Operator::Delete,
                    register,
                };
            }
            KeyCode::Char('p') => {
                let count = self.take_count();
                self.paste(false, count).await;
            }
            KeyCode::Char('P') => {
                let count = self.take_count();
                self.paste(true, count).await;
            }
            KeyCode::Char('"') => {
                self.engine.operator_state.register = Some('\0');
            }
            KeyCode::Char('m') => {
                self.engine.operator_state.mark = Some('m');
            }
            KeyCode::Char('`') => {
                self.engine.operator_state.mark = Some('`');
            }
            KeyCode::Char('\'') => {
                self.engine.operator_state.mark = Some('\'');
            }
            KeyCode::Char('@') => {
                self.engine.operator_state.macro_play = Some('@');
            }
            KeyCode::Char('q') => {
                if self.engine.state.command_buffer.is_empty() {
                    if self.engine.state.macros.is_recording() {
                        self.engine.state.macros.stop_recording();
                    } else {
                        let register = self.engine.operator_state.register.take();
                        self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                            op: Operator::RecordMacro,
                            register,
                        };
                    }
                }
            }
            _ => match key {
                KeyCode::Char('g') => {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::Goto,
                        register,
                    };
                }
                KeyCode::Char('G') => {
                    // `G` targets the last line that holds text, not the
                    // phantom line ropey reports after a trailing newline, and
                    // it lands on the first non-blank like Vim.
                    let last = self.engine.buffer.content_line_count();
                    let target = match self.engine.operator_state.count.take() {
                        Some(c) => c.clamp(1, last),
                        None => last,
                    };
                    self.engine.state.cursor.line = target;
                    self.engine.state.cursor.col = self.first_non_blank_col(target);
                    self.needs_render = true;
                }
                KeyCode::Char('%') => {
                    let _count = self.take_count();
                    self.jump_to_matching_bracket();
                    self.needs_render = true;
                }
                KeyCode::Char('{') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.paragraph_backward();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('}') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.paragraph_forward();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('(') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.sentence_backward();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char(')') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.sentence_forward();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('x') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.delete_char('"');
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('.') => {
                    let _count = self.take_count();
                    self.repeat_last_action().await;
                    self.needs_render = true;
                }
                KeyCode::Char('w') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.move_word_forward();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('W') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.move_word_forward_big();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('B') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.move_word_backward_big();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('E') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.move_word_end_big();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('b') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.move_word_backward();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('e') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.move_word_end();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('o') => {
                    let _count = self.take_count();
                    self.open_line(false);
                    self.needs_render = true;
                }
                KeyCode::Char('O') => {
                    let _count = self.take_count();
                    self.open_line(true);
                    self.needs_render = true;
                }
                KeyCode::Char('c') => {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::Change,
                        register,
                    };
                }
                KeyCode::Char('J') => {
                    // `3J` joins three lines into one, i.e. two line breaks.
                    let count = self.take_count();
                    for _ in 0..count.saturating_sub(1).max(1) {
                        self.join_lines();
                    }
                    self.needs_render = true;
                }
                KeyCode::Char('>') => {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::IndentRight,
                        register,
                    };
                }
                KeyCode::Char('<') => {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::IndentLeft,
                        register,
                    };
                }
                KeyCode::Char('z') => {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::Scroll,
                        register,
                    };
                }
                KeyCode::Char('Z') => {
                    let register = self.engine.operator_state.register.take();
                    self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                        op: Operator::SaveQuit,
                        register,
                    };
                }
                KeyCode::Char('s') => {
                    let _count = self.take_count();
                    self.delete_char('"');
                    let prev_mode = self.engine.state.mode;
                    self.engine.state.mode = Mode::Insert;
                    self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                    self.needs_render = true;
                }
                KeyCode::Char('~') => {
                    let count = self.take_count();
                    for _ in 0..count {
                        self.toggle_case_char();
                        let max_col = self
                            .engine
                            .buffer
                            .line_max_col(self.engine.state.cursor.line);
                        self.engine.state.cursor.col =
                            (self.engine.state.cursor.col + 1).min(max_col);
                    }
                    self.engine.dot_last_action = Some(DotAction::ToggleCase);
                    self.needs_render = true;
                }
                _ => {}
            },
        }
    }
}

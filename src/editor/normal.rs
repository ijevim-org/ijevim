use crate::editor::Editor;
use crate::types::DotAction;
use crate::types::{Mode, Operator, PendingOperator, PluginEvent, VisualType};
use crate::undo::{Edit, EditType};
use crossterm::event::KeyCode;

impl Editor {
    pub(crate) async fn handle_normal(&mut self, key: KeyCode) {
        // ── Ctrl-w prefix handling ──────────────────────────────────
        if self.engine.operator_state.ctrl_w_prefix {
            self.engine.operator_state.ctrl_w_prefix = false;
            self.handle_ctrl_w(key).await;
            return;
        }

        // ── Register prefix (`"x`) ───────────────────────────────────
        // The sentinel `'\0'` means "waiting for the register name".
        if self.engine.operator_state.register == Some('\0') {
            self.engine.operator_state.register = None;
            if let KeyCode::Char(c) = key {
                if c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '_' {
                    self.engine.operator_state.register = Some(c);
                    self.needs_render = true;
                    return;
                }
                if c == '"' {
                    // `""` selects the unnamed register, which is the default.
                    self.needs_render = true;
                    return;
                }
            }
        }

        // ── Mark prefix (m{a-z}, `{a-z}, '{a-z}) ────────────────────
        if let Some(pending) = self.engine.operator_state.mark {
            self.engine.operator_state.mark = None;
            if let KeyCode::Char(c) = key {
                if (pending == 'm' && c.is_ascii_lowercase())
                    || (pending != 'm' && !c.is_whitespace())
                {
                    self.handle_mark(pending, c);
                    self.needs_render = true;
                    return;
                }
            }
        }

        // ── Numeric count prefix ─────────────────────────────────
        // Accumulate digits 1-9 into pending_count.
        // '0' is only accumulated if there's already a pending count (e.g., "20j").
        // Standalone '0' falls through to the match arm for column-zero.
        if let KeyCode::Char(c) = key {
            if c.is_ascii_digit() && (c != '0' || self.engine.operator_state.count.is_some()) {
                let digit = c.to_digit(10).unwrap() as usize;
                self.engine.operator_state.count =
                    Some(self.engine.operator_state.count.unwrap_or(0) * 10 + digit);
                self.needs_render = true;
                return;
            }
        }

        // ── Directory listing (file browser) handling ────────────────
        // When the current buffer is a directory listing, intercept
        // Enter (open), '-' (parent dir), and 'R' (refresh).
        if self.engine.buffer.is_directory_listing() {
            match key {
                KeyCode::Enter => {
                    self.open_selected_entry();
                    return;
                }
                KeyCode::Char('-') => {
                    self.go_up_directory();
                    return;
                }
                KeyCode::Char('R') => {
                    self.refresh_directory();
                    return;
                }
                _ => {}
            }
        }

        // Resolve 3-key operator sequences (e.g., diw, da(, ciw)
        // and 2-key operator + motion sequences (e.g., dd, fw).
        // Single `replace` to avoid consuming operator state on text-object mismatch.
        let state = std::mem::replace(
            &mut self.engine.operator_state.pending,
            PendingOperator::Idle,
        );
        match state {
            PendingOperator::AwaitingTextObject {
                op,
                register,
                inner,
            } => {
                match op {
                    Operator::Yank | Operator::Delete => {
                        self.handle_text_object(key, register, inner).await;
                    }
                    Operator::Change => {
                        self.handle_text_object_change(key, register, inner).await;
                    }
                    _ => {}
                }
                return;
            }
            PendingOperator::AwaitingOperator { op, register } => {
                self.engine.operator_state.register = register;
                let count = self.take_count();
                self.handle_operator(op, key, count).await;
                return;
            }
            PendingOperator::Idle => {}
        }

        // ── Macro playback (@x) ──────────────────────────────────
        // Check BEFORE the outer match because the macro name register
        // (a-z) overlaps with Vim normal-mode commands like `a` (append).
        if let Some(_pending) = self.engine.operator_state.macro_play {
            if let KeyCode::Char(c) = key {
                if c.is_ascii_lowercase() {
                    // Clear macro_play BEFORE dispatching so re-entrant
                    // handle_normal calls don't try to play another macro.
                    self.engine.operator_state.macro_play = None;
                    self.play_macro(c).await;
                    self.needs_render = true;
                    return;
                }
            }
            self.engine.operator_state.macro_play = None;
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
                let line_len = self
                    .engine
                    .buffer
                    .line_char_len(self.engine.state.cursor.line);
                self.engine.state.cursor.col =
                    (self.engine.state.cursor.col + count).min(line_len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('j') => {
                let count = self.take_count();
                self.engine.state.cursor.line =
                    (self.engine.state.cursor.line + count).min(line_count);
                let len = self
                    .engine
                    .buffer
                    .line_char_len(self.engine.state.cursor.line);
                self.engine.state.cursor.col =
                    self.engine.state.cursor.col.min(len.saturating_sub(1));
                self.needs_render = true;
            }
            KeyCode::Char('k') => {
                let count = self.take_count();
                self.engine.state.cursor.line =
                    self.engine.state.cursor.line.saturating_sub(count).max(1);
                let len = self
                    .engine
                    .buffer
                    .line_char_len(self.engine.state.cursor.line);
                self.engine.state.cursor.col =
                    self.engine.state.cursor.col.min(len.saturating_sub(1));
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
            KeyCode::Char('a') => {
                let _count = self.take_count();
                let prev_mode = self.engine.state.mode;
                self.engine.insert_state.accum.clear();
                self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
                if self.engine.state.cursor.col
                    < self
                        .engine
                        .buffer
                        .line_char_len(self.engine.state.cursor.line)
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
                    .line_char_len(self.engine.state.cursor.line);
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
                let line = self.engine.buffer.get_line(target);
                let line_char_len = self.engine.buffer.line_char_len(target);
                let end_col = if line_char_len > 0 && line.ends_with('\n') {
                    line_char_len.saturating_sub(2)
                } else {
                    line_char_len.saturating_sub(1)
                };
                self.engine.state.cursor.line = target;
                self.engine.state.cursor.col = end_col;
                self.needs_render = true;
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
                let _count = self.take_count();
                self.delete_line('"');
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Insert;
                self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                    from: prev_mode,
                    to: Mode::Insert,
                });
                self.on_buffer_modified();
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
            KeyCode::Char('?') => {
                let prev_mode = self.engine.state.mode;
                self.engine.state.mode = Mode::Command;
                self.engine.state.command_buffer.clear();
                self.engine.state.command_cursor_pos = 0;
                self.engine.state.command_buffer.push('?');
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
            KeyCode::Char('d') => {
                let register = self.engine.operator_state.register.take();
                self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                    op: Operator::Delete,
                    register,
                };
            }
            KeyCode::Char('p') => {
                self.paste(false).await;
            }
            KeyCode::Char('P') => {
                self.paste(true).await;
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
                    let target = match self.engine.operator_state.count.take() {
                        Some(c) => c.min(self.engine.buffer.line_count()).max(1),
                        None => self.engine.buffer.line_count().max(1),
                    };
                    self.engine.state.cursor.line = target;
                    let len = self.engine.buffer.line_char_len(target);
                    self.engine.state.cursor.col = len.saturating_sub(1);
                    self.needs_render = true;
                }
                KeyCode::Char('%') => {
                    let _count = self.take_count();
                    self.jump_to_matching_bracket();
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
                    let count = self.take_count();
                    for _ in 0..count {
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
                        let line_len = self
                            .engine
                            .buffer
                            .line_char_len(self.engine.state.cursor.line);
                        self.engine.state.cursor.col =
                            (self.engine.state.cursor.col + 1).min(line_len.saturating_sub(1));
                    }
                    self.engine.dot_last_action = Some(DotAction::ToggleCase);
                    self.needs_render = true;
                }
                _ => {}
            },
        }
    }

    pub(crate) async fn handle_operator(&mut self, op: Operator, key: KeyCode, count: usize) {
        let register = match self.engine.operator_state.register.take() {
            Some(c) if c != '\0' => c,
            _ => '"',
        };

        match op {
            Operator::Yank => match key {
                KeyCode::Char('y') => {
                    let line = self.engine.state.cursor.line;
                    self.yank_lines_range(register, line, line + count.max(1) - 1);
                }
                KeyCode::Char('w') => {
                    self.yank_word(register);
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
                    self.yank_word_end(register);
                }
                KeyCode::Char('b') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let (word, start, _) = self.engine.buffer.get_word_range(line, col);
                    if !word.is_empty() {
                        self.yank_cols(register, start, col);
                    }
                }
                KeyCode::Char('j') => {
                    let line = self.engine.state.cursor.line;
                    self.yank_lines_range(register, line, line + count.max(1));
                }
                KeyCode::Char('k') => {
                    let line = self.engine.state.cursor.line;
                    let start = line.saturating_sub(count.max(1).saturating_sub(1));
                    self.yank_lines_range(register, start, line);
                }
                KeyCode::Char('G') => {
                    let line = self.engine.state.cursor.line;
                    self.yank_lines_range(register, line, self.engine.buffer.line_count());
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
                    let mut edits = Vec::new();
                    for _ in 0..count.max(1) {
                        if let Some(edit) = self.delete_line_edit(register) {
                            edits.push(edit);
                        }
                    }
                    self.push_undo_group(edits);
                }
                KeyCode::Char('w') => {
                    self.delete_word(register);
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
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let (word, _, end) = self.engine.buffer.get_word_range(line, col);
                    if !word.is_empty() {
                        self.delete_cols(register, col, end);
                    }
                }
                KeyCode::Char('b') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let (word, start, _) = self.engine.buffer.get_word_range(line, col);
                    if !word.is_empty() {
                        self.delete_cols(register, start, col);
                    }
                }
                KeyCode::Char('j') => {
                    let line = self.engine.state.cursor.line;
                    self.delete_lines_range(register, line, line + count.max(1));
                }
                KeyCode::Char('k') => {
                    let line = self.engine.state.cursor.line;
                    let start = line.saturating_sub(count.max(1).saturating_sub(1));
                    self.delete_lines_range(register, start, line);
                }
                KeyCode::Char('G') => {
                    let line = self.engine.state.cursor.line;
                    self.delete_lines_range(register, line, self.engine.buffer.line_count());
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
                    let mut edits = Vec::new();
                    for _ in 0..count.max(1) {
                        if let Some(edit) = self.delete_line_edit(register) {
                            edits.push(edit);
                        }
                    }
                    self.push_undo_group(edits);
                    let prev_mode = self.engine.state.mode;
                    self.engine.state.mode = Mode::Insert;
                    self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Insert,
                    });
                    self.engine.dot_last_action = Some(DotAction::Change {
                        text: String::new(),
                        line: self.engine.state.cursor.line,
                        col: 0,
                    });
                    self.on_buffer_modified();
                }
                KeyCode::Char('w') => {
                    let (word, start, _) = self.engine.buffer.get_word_range(
                        self.engine.state.cursor.line,
                        self.engine.state.cursor.col,
                    );
                    if !word.is_empty() {
                        let word_chars = word.chars().count();
                        let char_start = self
                            .engine
                            .buffer
                            .line_to_char(self.engine.state.cursor.line - 1)
                            + start;
                        let char_end = char_start + word_chars;
                        let content = self.engine.buffer.get_char_range(
                            self.engine.state.cursor.line,
                            start,
                            self.engine.state.cursor.line,
                            start + word_chars,
                        );
                        self.engine.register.set(register, &content);
                        self.engine.buffer.remove_range(char_start, char_end);
                        let prev_mode = self.engine.state.mode;
                        self.engine.state.mode = Mode::Insert;
                        self.engine.plugin_manager.emit(PluginEvent::ModeChange {
                            from: prev_mode,
                            to: Mode::Insert,
                        });
                        self.engine.dot_last_action = Some(DotAction::Change {
                            text: content,
                            line: self.engine.state.cursor.line,
                            col: start,
                        });
                        self.on_buffer_modified();
                    }
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
                KeyCode::Char('e') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let (word, _, end) = self.engine.buffer.get_word_range(line, col);
                    if !word.is_empty() {
                        self.delete_cols(register, col, end);
                    }
                    self.enter_insert_after_change();
                }
                KeyCode::Char('b') => {
                    let line = self.engine.state.cursor.line;
                    let col = self.engine.state.cursor.col;
                    let (word, start, _) = self.engine.buffer.get_word_range(line, col);
                    if !word.is_empty() {
                        self.delete_cols(register, start, col);
                    }
                    self.enter_insert_after_change();
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
                    for _ in 0..count {
                        self.indent_lines(register, true);
                    }
                }
            }
            Operator::IndentLeft => {
                if let KeyCode::Char('<') = key {
                    for _ in 0..count {
                        self.indent_lines(register, false);
                    }
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
                        self.engine.state.cursor = start;
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


    /// Enter Insert mode after a `c{motion}` deletion.
    fn enter_insert_after_change(&mut self) {
        self.engine.insert_state.accum.clear();
        self.engine.insert_state.start_pos = Some(self.engine.state.cursor);
        let prev_mode = self.engine.state.mode;
        self.engine.state.mode = Mode::Insert;
        self.engine
            .plugin_manager
            .emit(PluginEvent::ModeChange {
                from: prev_mode,
                to: Mode::Insert,
            });
        self.needs_render = true;
    }

    /// Content length of a line, excluding its trailing newline.
    fn line_content_len(&self, line: usize) -> usize {
        self.engine
            .buffer
            .get_line(line)
            .chars()
            .take_while(|c| *c != '\n')
            .count()
    }

    /// Delete the character range `[start, end)` on the cursor line.
    fn delete_cols(&mut self, register: char, start: usize, end: usize) {
        let line = self.engine.state.cursor.line;
        let (start, end) = (start.min(end), start.max(end));
        let end = end.min(self.line_content_len(line));
        if start >= end {
            return;
        }
        let content = self
            .engine
            .buffer
            .get_char_range(line, start, line, end);
        let char_start = self.engine.buffer.line_to_char(line.saturating_sub(1)) + start;
        let mod_count = self.engine.buffer.modification_count();
        self.engine
            .buffer
            .remove_range(char_start, char_start + (end - start));
        self.engine.register.set(register, &content);
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Delete {
                line,
                col: start,
                text: content,
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.engine.state.cursor.col = start;
        self.on_buffer_modified();
    }

    /// Yank the character range `[start, end)` on the cursor line.
    fn yank_cols(&mut self, register: char, start: usize, end: usize) {
        let line = self.engine.state.cursor.line;
        let end = end.min(self.line_content_len(line));
        let (start, end) = (start.min(end), start.max(end));
        if start >= end {
            return;
        }
        let content = self
            .engine
            .buffer
            .get_char_range(line, start, line, end);
        self.engine.register.set(register, &content);
        self.needs_render = true;
    }

    /// Delete whole lines `start_line..=end_line` as a single undo step.
    fn delete_lines_range(&mut self, register: char, start_line: usize, end_line: usize) {
        let line_count = self.engine.buffer.line_count();
        let start = start_line.max(1).min(line_count);
        let end = end_line.max(1).min(line_count);
        if start > end {
            return;
        }
        let count = end - start + 1;
        let restore_line = self.engine.state.cursor.line;
        self.engine.state.cursor.line = start;

        let mut edits = Vec::new();
        for _ in 0..count {
            if let Some(edit) = self.delete_line_edit(register) {
                edits.push(edit);
            }
        }
        self.push_undo_group(edits);

        let line_count = self.engine.buffer.line_count().max(1);
        self.engine.state.cursor.line = restore_line.min(line_count);
        let len = self
            .engine
            .buffer
            .line_char_len(self.engine.state.cursor.line);
        self.engine.state.cursor.col = self.engine.state.cursor.col.min(len.saturating_sub(1));
        self.needs_render = true;
    }

    /// Yank whole lines `start_line..=end_line` into `register`.
    fn yank_lines_range(&mut self, register: char, start_line: usize, end_line: usize) {
        let line_count = self.engine.buffer.line_count();
        let start = start_line.max(1).min(line_count);
        let end = end_line.max(1).min(line_count);
        if start > end {
            return;
        }
        let content = self.engine.buffer.get_line_range(start, end);
        self.engine.register.set(register, &content);
        self.needs_render = true;
    }

    fn first_non_blank_col(&self, line: usize) -> usize {
        self.engine
            .buffer
            .get_line(line)
            .chars()
            .position(|c| !c.is_whitespace())
            .unwrap_or(0)
    }

    fn yank_word(&mut self, register: char) {
        let (word, _, _) = self
            .engine
            .buffer
            .get_word_range(self.engine.state.cursor.line, self.engine.state.cursor.col);
        self.engine.register.set(register, &word);
        self.needs_render = true;
    }

    fn yank_word_end(&mut self, register: char) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let line_str = self.engine.buffer.get_line(line);
        let chars: Vec<char> = line_str.chars().collect();

        let mut end = col;
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }

        let word: String = chars[col..end].iter().collect();
        self.engine.register.set(register, &word);
        self.needs_render = true;
    }

    fn delete_line(&mut self, register: char) {
        if let Some(edit) = self.delete_line_edit(register) {
            self.engine.undo_manager.push(edit);
        }
    }

    /// Delete the current line and return the undo entry without pushing it.
    ///
    /// Callers that delete several lines (e.g. `3dd`) collect the entries and
    /// push them as one [`EditType::Group`].
    fn delete_line_edit(&mut self, register: char) -> Option<Edit> {
        let line = self.engine.state.cursor.line;
        let mod_count = self.engine.buffer.modification_count();
        let content = self.engine.buffer.delete_line(line);
        if content.is_empty() {
            return None;
        }
        self.engine.register.set(register, &content);

        let edit = Edit {
            edit_type: EditType::DeleteLine {
                line,
                text: content,
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        };

        let line_count = self.engine.buffer.line_count();
        if line > line_count {
            self.engine.state.cursor.line = line_count.max(1);
        }
        let len = self
            .engine
            .buffer
            .line_char_len(self.engine.state.cursor.line);
        self.engine.state.cursor.col = self.engine.state.cursor.col.min(len.saturating_sub(1));
        self.on_buffer_modified();
        Some(edit)
    }

    /// Push a set of edits as a single undo step. No-op for an empty set.
    ///
    /// The group must carry the modification count and cursor from *before* the
    /// first edit, otherwise undoing it leaves the buffer marked dirty.
    pub(crate) fn push_undo_group(&mut self, edits: Vec<Edit>) {
        let Some(first) = edits.first() else {
            return;
        };
        let modification_count = first.modification_count;
        let cursor_before = first.cursor_before;
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Group { edits },
            cursor_before,
            cursor_after: self.engine.state.cursor,
            modification_count,
        });
    }

    fn delete_word(&mut self, register: char) {
        let (word, start, _) = self
            .engine
            .buffer
            .get_word_range(self.engine.state.cursor.line, self.engine.state.cursor.col);
        if !word.is_empty() {
            let char_start = self
                .engine
                .buffer
                .line_to_char(self.engine.state.cursor.line - 1)
                + start;
            let char_end = char_start + word.chars().count();
            self.engine.buffer.remove_range(char_start, char_end);
            self.engine.register.set(register, &word);
        }
        self.needs_render = true;
    }

    async fn paste(&mut self, before: bool) {
        let register = match self.engine.operator_state.register.take() {
            Some(c) if c != '\0' => c,
            _ => '"',
        };
        let content = self.engine.register.get(register);
        if content.is_empty() {
            return;
        }

        let mod_count = self.engine.buffer.modification_count();
        let cursor_before = self.engine.state.cursor;

        if content.contains('\n') {
            let lines: Vec<&str> = content.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                if i == 0 {
                    if before {
                        self.engine
                            .buffer
                            .insert(self.engine.state.cursor.line, 0, line);
                        self.engine.buffer.insert(
                            self.engine.state.cursor.line,
                            line.chars().count(),
                            "\n",
                        );
                    } else {
                        self.engine.buffer.insert(
                            self.engine.state.cursor.line,
                            self.engine.state.cursor.col,
                            line,
                        );
                        self.engine.buffer.insert(
                            self.engine.state.cursor.line,
                            self.engine.state.cursor.col + line.chars().count(),
                            "\n",
                        );
                    }
                } else {
                    let insert_line = self.engine.state.cursor.line + i;
                    self.engine.buffer.insert(insert_line, 0, line);
                    self.engine
                        .buffer
                        .insert(insert_line, line.chars().count(), "\n");
                }
            }
            if before {
                self.engine.state.cursor.line += lines.len() - 1;
                self.engine.state.cursor.col = lines.last().map(|l| l.chars().count()).unwrap_or(0);
            } else {
                self.engine.state.cursor.line += lines.len() - 1;
                let last_line = lines.last().unwrap();
                self.engine.state.cursor.col = last_line.chars().count();
            }
        } else {
            if before {
                self.engine.buffer.insert(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col,
                    &content,
                );
            } else {
                self.engine.buffer.insert(
                    self.engine.state.cursor.line,
                    self.engine.state.cursor.col + 1,
                    &content,
                );
                self.engine.state.cursor.col += 1;
            }
            if !before {
                self.engine.state.cursor.col += content.chars().count();
            }
        }

        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Insert {
                line: cursor_before.line,
                col: cursor_before.col,
                text: content,
            },
            cursor_before,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
        self.on_buffer_modified();
    }

    /// Toggle case of character under cursor (`~`).
    fn toggle_case_char(&mut self) {
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

    fn delete_char(&mut self, register: char) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let line_str = self.engine.buffer.get_line(line);
        let mod_count = self.engine.buffer.modification_count();

        if col >= line_str.chars().count() {
            if line < self.engine.buffer.line_count() {
                // Merge with next line (EOL x behavior)
                self.engine.buffer.merge_with_prev_line(line + 1);
                self.engine.undo_manager.push(Edit {
                    edit_type: EditType::Merge {
                        line,
                        deleted_newline_col: col,
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
        } else {
            let deleted_text: String = line_str[col..].chars().take(1).collect();
            self.engine.buffer.delete(line, col);
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

    async fn repeat_last_action(&mut self) {
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
                    let register = self.engine.operator_state.register.unwrap_or('"');
                    self.indent_lines(register, !unindent);
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

        if above {
            self.engine.buffer.insert(line, 0, "\n");
            self.engine.state.cursor.line = line;
        } else {
            self.engine.buffer.insert(line + 1, 0, "\n");
            self.engine.state.cursor.line = line + 1;
        }
        self.engine.state.cursor.col = 0;

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
        if line >= self.engine.buffer.line_count() {
            return;
        }

        let mod_count = self.engine.buffer.modification_count();
        let current_line = self.engine.buffer.get_line(line);
        let current_stripped = current_line.trim_end_matches('\n');
        let next_line = self.engine.buffer.get_line(line + 1);
        let next_stripped = next_line.trim_end_matches('\n');

        let join_pos = self.engine.buffer.merge_with_prev_line(line + 1);

        let needs_space = !current_stripped.ends_with(' ')
            && !current_stripped.ends_with('\t')
            && !next_stripped.starts_with(' ')
            && !next_stripped.is_empty();

        let merge_edit = Edit {
            edit_type: EditType::Merge {
                line,
                deleted_newline_col: join_pos,
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        };

        if needs_space {
            self.engine.buffer.insert(line, join_pos, " ");
            self.engine.state.cursor.col = join_pos + 1;
            // The separating space is part of the join: undo must remove it
            // together with restoring the newline.
            self.push_undo_group(vec![
                merge_edit,
                Edit {
                    edit_type: EditType::Insert {
                        line,
                        col: join_pos,
                        text: " ".to_string(),
                    },
                    cursor_before: self.engine.state.cursor,
                    cursor_after: self.engine.state.cursor,
                    modification_count: mod_count,
                },
            ]);
        } else {
            self.engine.state.cursor.col = join_pos;
            self.engine.undo_manager.push(merge_edit);
        }

        self.engine.dot_last_action = Some(DotAction::Join);
        self.on_buffer_modified();
    }

    async fn handle_ctrl_w(&mut self, key: KeyCode) {
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

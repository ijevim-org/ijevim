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
                // `g` starts a two-key motion (`dgg`). Only the operators
                // that take a motion wait for it; `gg` itself is a complete
                // motion and must pass through.
                if let KeyCode::Char('g') = key {
                    if matches!(op, Operator::Delete | Operator::Yank | Operator::Change)
                        && !self.engine.operator_state.pending_g
                    {
                        self.engine.operator_state.pending_g = true;
                        self.engine.operator_state.pending =
                            PendingOperator::AwaitingOperator { op, register };
                        self.needs_render = true;
                        return;
                    }
                }
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
                        self.engine.state.cursor = self.engine.state.last_visual_end.unwrap_or(start);
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

    /// Content length of a line, excluding its trailing newline.
    pub(crate) fn line_content_len(&self, line: usize) -> usize {
        self.engine
            .buffer
            .get_line(line)
            .chars()
            .take_while(|c| *c != '\n')
            .count()
    }

    /// Delete the character range `[start, end)` on the cursor line.
    pub(crate) fn delete_cols(&mut self, register: char, start: usize, end: usize) {
        if let Some(edit) = self.delete_cols_edit(register, start, end) {
            self.engine.undo_manager.push(edit);
        }
    }

    /// Delete the range and return its undo entry without pushing it.
    ///
    /// Multi-step operators (`3dw`, `dj`, ...) collect the entries and push
    /// them as one group, so the single `u` a user expects restores them all.
    pub(crate) fn delete_cols_edit(&mut self, register: char, start: usize, end: usize) -> Option<Edit> {
        let line = self.engine.state.cursor.line;
        let (start, end) = (start.min(end), start.max(end));
        let end = end.min(self.line_content_len(line));
        if start >= end {
            return None;
        }
        let content = self.engine.buffer.get_char_range(line, start, line, end);
        let char_start = self.engine.buffer.line_to_char(line.saturating_sub(1)) + start;
        let mod_count = self.engine.buffer.modification_count();
        self.engine
            .buffer
            .remove_range(char_start, char_start + (end - start));
        self.engine.register.set(register, &content);
        self.engine.state.cursor.col = start.min(
            self.engine
                .buffer
                .line_max_col(self.engine.state.cursor.line),
        );
        self.on_buffer_modified();
        Some(Edit {
            edit_type: EditType::Delete {
                line,
                col: start,
                text: content,
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        })
    }

    /// Yank the character range `[start, end)` on the cursor line.
    pub(crate) fn yank_cols(&mut self, register: char, start: usize, end: usize) {
        let line = self.engine.state.cursor.line;
        let end = end.min(self.line_content_len(line));
        let (start, end) = (start.min(end), start.max(end));
        if start >= end {
            return;
        }
        let content = self.engine.buffer.get_char_range(line, start, line, end);
        self.engine.register.set(register, &content);
        self.needs_render = true;
    }

    /// Delete whole lines `start_line..=end_line` as a single undo step.
    pub(crate) fn delete_lines_range(&mut self, register: char, start_line: usize, end_line: usize) {
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

        // Vim leaves the cursor on the first line of the deleted range,
        // clamped to the new end of the buffer, at its first non-blank.
        let line_count = self.engine.buffer.content_line_count().max(1);
        let _ = restore_line;
        self.engine.state.cursor.line = start.min(line_count);
        self.engine.state.cursor.col = self.first_non_blank_col(self.engine.state.cursor.line);
        self.needs_render = true;
    }

    /// Yank whole lines `start_line..=end_line` into `register`.
    pub(crate) fn yank_lines_range(&mut self, register: char, start_line: usize, end_line: usize) {
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

    pub(crate) fn first_non_blank_col(&self, line: usize) -> usize {
        self.engine
            .buffer
            .get_line(line)
            .chars()
            .position(|c| !c.is_whitespace())
            .unwrap_or(0)
    }

    /// Resolve the target of a charwise word motion the way Vim does.
    ///
    /// A repeated `w` that lands in column 0 of a later line stops at the end
    /// of the previous line: the line break is only part of the motion when
    /// the target is further into the line.
    pub(crate) fn exclusive_target(&self, from_line: usize, target: (usize, usize)) -> (usize, usize) {
        let (line, col) = target;
        if line > from_line && col == 0 {
            (line - 1, self.line_content_len(line - 1))
        } else {
            target
        }
    }

    /// Target of `{count}w` / `{count}e`, honoring the count across lines.
    pub(crate) fn word_motion_target(&self, count: usize, big: bool, to_word_end: bool) -> (usize, usize) {
        let (mut line, mut col) = (self.engine.state.cursor.line, self.engine.state.cursor.col);
        for _ in 0..count.max(1) {
            let next = if to_word_end {
                self.word_end_pos(line, col, big)
            } else {
                self.word_forward_pos(line, col, big)
            };
            if next == (line, col) {
                break;
            }
            line = next.0;
            col = next.1;
        }
        (line, col)
    }

    /// Target of `{count}b` / `{count}B`, crossing lines.
    pub(crate) fn word_backward_target(&self, count: usize, big: bool) -> (usize, usize) {
        let (mut line, mut col) = (self.engine.state.cursor.line, self.engine.state.cursor.col);
        for _ in 0..count.max(1) {
            let next = self.word_backward_pos(line, col, big);
            if next == (line, col) {
                break;
            }
            line = next.0;
            col = next.1;
        }
        (line, col)
    }

    /// `cc` / `S`: clear `count` lines and leave a single empty line behind.
    pub(crate) fn change_lines(&mut self, register: char, count: usize) {
        let start = self.engine.state.cursor.line;
        let mut edits = Vec::new();
        for _ in 1..count.max(1) {
            if let Some(edit) = self.delete_line_edit(register) {
                edits.push(edit);
            }
        }
        let len = self.line_content_len(self.engine.state.cursor.line);
        if let Some(edit) = self.delete_cols_edit(register, 0, len) {
            edits.push(edit);
        }
        self.push_undo_group(edits);
        self.engine.state.cursor.line = start.min(self.engine.buffer.line_count());
        self.engine.state.cursor.col = 0;
        self.enter_insert_after_change();
    }

    /// `cj` / `ck` / `cG`: delete whole lines, keep one empty line, insert.
    pub(crate) fn change_line_range(&mut self, register: char, first: usize, last: usize) {
        let first = first.max(1);
        let last = last.max(first).min(self.engine.buffer.content_line_count());
        self.engine.state.cursor.line = first;
        let mut edits = Vec::new();
        for _ in first..=last {
            if let Some(edit) = self.delete_line_edit(register) {
                edits.push(edit);
            }
        }
        // Vim leaves one empty line where the changed lines were.
        if self.engine.buffer.to_string() != "\n" {
            self.engine.buffer.insert(first, 0, "\n");
            edits.push(Edit {
                edit_type: EditType::InsertLine {
                    line: first,
                    text: String::new(),
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: self.engine.buffer.modification_count(),
            });
        }
        self.push_undo_group(edits);
        self.engine.state.cursor.line = first.min(self.engine.buffer.line_count());
        self.engine.state.cursor.col = 0;
        self.enter_insert_after_change();
    }

    /// Delete an arbitrary charwise span, joining lines when it crosses one.
    pub(crate) fn delete_span(&mut self, register: char, from: (usize, usize), to: (usize, usize)) {
        if let Some(edit) = self.delete_span_edit(register, from, to) {
            self.engine.undo_manager.push(edit);
        }
    }

    /// Core of [`Self::delete_span`] that leaves the undo entry to the caller.
    fn delete_span_edit(
        &mut self,
        register: char,
        from: (usize, usize),
        to: (usize, usize),
    ) -> Option<Edit> {
        if from >= to {
            return None;
        }
        let content = self
            .engine
            .buffer
            .get_char_range(from.0, from.1, to.0, to.1);
        if content.is_empty() {
            return None;
        }
        let mod_count = self.engine.buffer.modification_count();
        let start_char = self.engine.buffer.line_to_char(from.0.saturating_sub(1)) + from.1;
        let end_char = start_char + content.chars().count();
        self.engine.buffer.remove_range(start_char, end_char);
        self.engine.register.set(register, &content);
        self.engine.state.cursor.line = from.0;
        self.engine.state.cursor.col = from.1.min(self.engine.buffer.line_max_col(from.0));
        self.on_buffer_modified();
        Some(Edit {
            edit_type: EditType::Delete {
                line: from.0,
                col: from.1,
                text: content,
            },
            cursor_before: self.engine.state.cursor,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        })
    }

    /// Yank an arbitrary charwise span.
    pub(crate) fn yank_span(&mut self, register: char, from: (usize, usize), to: (usize, usize)) {
        if from >= to {
            return;
        }
        let content = self
            .engine
            .buffer
            .get_char_range(from.0, from.1, to.0, to.1);
        if content.is_empty() {
            return;
        }
        self.engine.register.set(register, &content);
        self.needs_render = true;
    }

    /// `yw` / `yW`: yank the same span `dw` would delete.
    fn yank_word(&mut self, register: char, count: usize, big: bool) {
        let from = (self.engine.state.cursor.line, self.engine.state.cursor.col);
        let target = self.word_motion_target(count, big, false);
        let target = self.exclusive_target(from.0, target);
        self.yank_span(register, from, target);
    }

    /// `ye` / `yE`: yank through the end of the word, inclusive.
    fn yank_word_end(&mut self, register: char, count: usize, big: bool) {
        let from = (self.engine.state.cursor.line, self.engine.state.cursor.col);
        let (line, col) = self.word_motion_target(count, big, true);
        self.yank_span(register, from, (line, col + 1));
    }

    /// Delete the current line and return the undo entry without pushing it.
    ///
    /// Callers that delete several lines (e.g. `3dd`) collect the entries and
    /// push them as one [`EditType::Group`].
    pub(crate) fn delete_line_edit(&mut self, register: char) -> Option<Edit> {
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
        let max_col = self
            .engine
            .buffer
            .line_max_col(self.engine.state.cursor.line);
        self.engine.state.cursor.col = self.engine.state.cursor.col.min(max_col);
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

    /// `dw` / `dW`: delete to the start of the next word, like Vim.
    ///
    /// The trailing blanks belong to the motion; without them the next word
    /// would stay glued to whatever follows. When no further word exists the
    /// delete runs to the end of the line.
    pub(crate) fn delete_word(&mut self, register: char, count: usize, big: bool) {
        let from = (self.engine.state.cursor.line, self.engine.state.cursor.col);
        let target = self.word_motion_target(count, big, false);
        let target = self.exclusive_target(from.0, target);
        self.delete_span(register, from, target);
    }

    /// `de` / `dE`: delete through the end of the word, inclusive.
    pub(crate) fn delete_word_end(&mut self, register: char, count: usize, big: bool) {
        let from = (self.engine.state.cursor.line, self.engine.state.cursor.col);
        let (line, col) = self.word_motion_target(count, big, true);
        self.delete_span(register, from, (line, col + 1));
    }

    /// `cw` / `cW`: change to the end of the word, or behave like `dw` when the
    /// cursor sits on blanks (Vim's documented special case).
    pub(crate) fn change_word(&mut self, register: char, count: usize, big: bool) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let on_blank = self
            .engine
            .buffer
            .line_chars(line)
            .get(col)
            .map_or(true, |c| c.is_whitespace());
        if on_blank {
            self.delete_word(register, count, big);
        } else {
            self.delete_word_end(register, count, big);
        }
        self.enter_insert_after_change();
    }

    async fn paste(&mut self, before: bool, count: usize) {
        let register = match self.engine.operator_state.register.take() {
            Some(c) if c != '\0' => c,
            _ => '"',
        };
        let content = self.engine.register.get(register);
        if content.is_empty() {
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

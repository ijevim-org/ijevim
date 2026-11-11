use crate::editor::Editor;
use crate::types::{Mode, PendingOperator, VisualType};
use crossterm::event::KeyCode;

mod case;
mod operators;
mod selection;
mod text_objects;

use case::CaseTransform;

impl Editor {
    pub(crate) async fn handle_visual(&mut self, key: KeyCode) {
        // ── Register prefix (`"x`) ───────────────────────────────────
        if self.engine.operator_state.register == Some('\0') {
            self.engine.operator_state.register = None;
            if let KeyCode::Char(c) = key {
                if c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '_' {
                    self.engine.operator_state.register = Some(c);
                    self.needs_render = true;
                } else if c == '"' {
                    self.needs_render = true;
                }
            }
            return;
        }

        // ── Pending `f`/`F`/`t`/`T` character ───────────────────────
        if let PendingOperator::AwaitingOperator { op, .. } =
            self.engine.operator_state.pending.clone()
        {
            let (till, forward) = match op {
                crate::types::Operator::FindCharForward => (false, true),
                crate::types::Operator::FindCharBackward => (false, false),
                crate::types::Operator::TillCharForward => (true, true),
                crate::types::Operator::TillCharBackward => (true, false),
                _ => (false, false),
            };
            self.engine.operator_state.pending = PendingOperator::Idle;
            if let KeyCode::Char(ch) = key {
                self.engine.search_state.last_fchar = Some(ch);
                self.engine.search_state.last_fchar_till = till;
                let count = self.visual_count();
                for _ in 0..count {
                    self.find_char(ch, till, forward);
                }
                self.needs_render = true;
            }
            return;
        }

        // ── Pending text object (`i`/`a` + object) ──────────────────
        if let PendingOperator::AwaitingTextObject { inner, .. } =
            self.engine.operator_state.pending.clone()
        {
            self.engine.operator_state.pending = PendingOperator::Idle;
            self.visual_text_object(inner, key);
            return;
        }

        // ── Pending `g` prefix (`gg`, `ge`, `g~`, `gu`, `gU`, `gJ`) ──
        if self.engine.operator_state.pending_g {
            self.engine.operator_state.pending_g = false;
            match key {
                KeyCode::Char('g') => {
                    let count = self.visual_count();
                    let target = if count > 1 { count } else { 1 };
                    self.visual_goto_line(target);
                }
                KeyCode::Char('e') => {
                    let count = self.visual_count();
                    for _ in 0..count {
                        self.move_word_end_backward(false);
                    }
                }
                KeyCode::Char('E') => {
                    let count = self.visual_count();
                    for _ in 0..count {
                        self.move_word_end_backward(true);
                    }
                }
                KeyCode::Char('~') => self.transform_selection(CaseTransform::Toggle),
                KeyCode::Char('u') => self.transform_selection(CaseTransform::Lower),
                KeyCode::Char('U') => self.transform_selection(CaseTransform::Upper),
                KeyCode::Char('J') => self.visual_join(false),
                _ => {}
            }
            self.needs_render = true;
            return;
        }

        // ── Numeric count prefix ────────────────────────────────────
        if let KeyCode::Char(c) = key {
            if c.is_ascii_digit() && (c != '0' || self.engine.operator_state.count.is_some()) {
                let digit = c.to_digit(10).unwrap() as usize;
                self.engine.operator_state.count = Some(
                    self.engine
                        .operator_state
                        .count
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(digit),
                );
                self.needs_render = true;
                return;
            }
        }

        match key {
            KeyCode::Esc => {
                self.exit_visual(Mode::Normal);
            }

            // ── Mode switches ───────────────────────────────────────
            KeyCode::Char('v') => {
                if self.engine.state.visual_type == Some(VisualType::Character) {
                    self.exit_visual(Mode::Normal);
                } else {
                    self.engine.state.visual_type = Some(VisualType::Character);
                    self.needs_render = true;
                }
            }
            KeyCode::Char('V') => {
                if self.engine.state.visual_type == Some(VisualType::Line) {
                    self.exit_visual(Mode::Normal);
                } else {
                    self.engine.state.visual_type = Some(VisualType::Line);
                    self.needs_render = true;
                }
            }

            // ── Motions ─────────────────────────────────────────────
            KeyCode::Char('h') | KeyCode::Left => {
                let count = self.visual_count();
                self.cursor_left(count);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let count = self.visual_count();
                self.cursor_right(count);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let count = self.visual_count();
                self.cursor_down(count);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let count = self.visual_count();
                self.cursor_up(count);
            }
            KeyCode::Char('w') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.move_word_forward();
                }
            }
            KeyCode::Char('W') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.move_word_forward_big();
                }
            }
            KeyCode::Char('b') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.move_word_backward();
                }
            }
            KeyCode::Char('B') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.move_word_backward_big();
                }
            }
            KeyCode::Char('e') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.move_word_end();
                }
            }
            KeyCode::Char('E') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.move_word_end_big();
                }
            }
            KeyCode::Char('0') => {
                self.cursor_line_start();
            }
            KeyCode::Char('^') => {
                let line = self.engine.state.cursor.line;
                self.engine.state.cursor.col = self.first_non_blank_col(line);
                self.needs_render = true;
            }
            KeyCode::Char('$') => {
                let count = self.visual_count();
                if count > 1 {
                    self.cursor_down(count - 1);
                }
                self.cursor_line_end();
            }
            KeyCode::Char('|') => {
                let count = self.visual_count();
                let max_col = self
                    .engine
                    .buffer
                    .line_max_col(self.engine.state.cursor.line);
                self.engine.state.cursor.col = count.saturating_sub(1).min(max_col);
                self.needs_render = true;
            }
            KeyCode::Char('+') | KeyCode::Enter => {
                let count = self.visual_count();
                self.cursor_down(count);
                self.cursor_line_start();
            }
            KeyCode::Char('-') => {
                let count = self.visual_count();
                self.cursor_up(count);
                self.cursor_line_start();
            }
            KeyCode::Char('_') => {
                let count = self.visual_count();
                if count > 1 {
                    self.cursor_down(count - 1);
                }
                let line = self.engine.state.cursor.line;
                self.engine.state.cursor.col = self.first_non_blank_col(line);
                self.needs_render = true;
            }
            KeyCode::Char('%') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.jump_to_matching_bracket();
                }
                self.needs_render = true;
            }
            KeyCode::Char('{') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.paragraph_backward();
                }
            }
            KeyCode::Char('}') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.paragraph_forward();
                }
            }
            KeyCode::Char('(') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.sentence_backward();
                }
            }
            KeyCode::Char(')') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.sentence_forward();
                }
            }
            KeyCode::Char('H') | KeyCode::Char('M') | KeyCode::Char('L') => {
                if let KeyCode::Char(c) = key {
                    self.visual_screen_line(c);
                }
            }
            KeyCode::Char('G') => {
                let last = self.engine.buffer.content_line_count();
                let target = match self.engine.operator_state.count.take() {
                    Some(c) => c.clamp(1, last),
                    None => last,
                };
                self.visual_goto_line(target);
            }
            KeyCode::Char('g') => {
                self.engine.operator_state.pending_g = true;
                self.needs_render = true;
            }

            // ── Character search ────────────────────────────────────
            KeyCode::Char('f') => {
                self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                    op: crate::types::Operator::FindCharForward,
                    register: None,
                };
            }
            KeyCode::Char('F') => {
                self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                    op: crate::types::Operator::FindCharBackward,
                    register: None,
                };
            }
            KeyCode::Char('t') => {
                self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                    op: crate::types::Operator::TillCharForward,
                    register: None,
                };
            }
            KeyCode::Char('T') => {
                self.engine.operator_state.pending = PendingOperator::AwaitingOperator {
                    op: crate::types::Operator::TillCharBackward,
                    register: None,
                };
            }
            KeyCode::Char(';') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.repeat_find(true);
                }
            }
            KeyCode::Char(',') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.repeat_find(false);
                }
            }

            // ── Search motions ──────────────────────────────────────
            KeyCode::Char('n') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.search_next();
                }
                self.needs_render = true;
            }
            KeyCode::Char('N') => {
                let count = self.visual_count();
                for _ in 0..count {
                    self.search_prev();
                }
                self.needs_render = true;
            }

            // ── Text object prefix ──────────────────────────────────
            KeyCode::Char('i') => {
                self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                    op: crate::types::Operator::Yank,
                    register: '"',
                    inner: true,
                };
            }
            KeyCode::Char('a') => {
                self.engine.operator_state.pending = PendingOperator::AwaitingTextObject {
                    op: crate::types::Operator::Yank,
                    register: '"',
                    inner: false,
                };
            }

            // ── Swap anchor ─────────────────────────────────────────
            KeyCode::Char('o') | KeyCode::Char('O') => {
                if let Some(anchor) = self.engine.state.visual_start {
                    let cursor = self.engine.state.cursor;
                    self.engine.state.visual_start = Some(cursor);
                    self.engine.state.cursor = anchor;
                    self.needs_render = true;
                }
            }

            // ── Insert at selection start/end ───────────────────────
            KeyCode::Char('I') => {
                self.visual_insert_start();
            }
            KeyCode::Char('A') => {
                self.visual_insert_end();
            }

            // ── Yank / delete / change ──────────────────────────────
            KeyCode::Char('y') => {
                self.visual_yank(false);
                self.exit_visual(Mode::Normal);
            }
            KeyCode::Char('Y') => {
                let block = self.engine.state.visual_type == Some(VisualType::Block);
                self.visual_yank(!block);
                self.exit_visual(Mode::Normal);
            }
            KeyCode::Char('d') | KeyCode::Char('x') => {
                self.visual_delete();
                self.exit_visual(Mode::Normal);
            }
            KeyCode::Char('X') | KeyCode::Char('D') => {
                if self.engine.state.visual_type == Some(VisualType::Block) {
                    if matches!(key, KeyCode::Char('X')) {
                        self.visual_delete();
                    } else {
                        self.visual_delete_to_eol();
                    }
                } else {
                    self.visual_delete_lines();
                }
                self.exit_visual(Mode::Normal);
            }
            KeyCode::Char('c') | KeyCode::Char('s') => {
                self.visual_change(false);
            }
            KeyCode::Char('C') | KeyCode::Char('S') => {
                self.visual_change(true);
            }

            // ── Put ─────────────────────────────────────────────────
            KeyCode::Char('p') => {
                self.visual_put(false);
            }
            KeyCode::Char('P') => {
                self.visual_put(true);
            }

            // ── Replace / case / indent / join ──────────────────────
            KeyCode::Char('r') => {
                self.engine.operator_state.replace_char = Some('\0');
                self.engine.operator_state.pending = PendingOperator::Idle;
                // Wait for the replacement char by reusing Replace mode's
                // `replace_char` sentinel. The keymap routes Replace mode
                // keys to `handle_replace`, which handles the sentinel.
                self.transition_mode(Mode::Replace);
            }
            KeyCode::Char('~') => self.transform_selection(CaseTransform::Toggle),
            KeyCode::Char('u') => self.transform_selection(CaseTransform::Lower),
            KeyCode::Char('U') => self.transform_selection(CaseTransform::Upper),
            KeyCode::Char('>') => {
                let count = self.visual_count();
                if let Some((start, end)) = self.selection_bounds() {
                    self.engine.state.cursor.line = start.line.min(end.line);
                    for _ in 0..count {
                        self.indent_lines(start.line.min(end.line), start.line.max(end.line), true);
                    }
                }
                self.exit_visual(Mode::Normal);
            }
            KeyCode::Char('<') => {
                let count = self.visual_count();
                if let Some((start, end)) = self.selection_bounds() {
                    self.engine.state.cursor.line = start.line.min(end.line);
                    for _ in 0..count {
                        self.indent_lines(
                            start.line.min(end.line),
                            start.line.max(end.line),
                            false,
                        );
                    }
                }
                self.exit_visual(Mode::Normal);
            }
            KeyCode::Char('J') => self.visual_join(true),

            // ── Visual command line ─────────────────────────────────
            KeyCode::Char(':') => {
                self.save_visual_selection();
                self.reset_command_line_nav();
                self.engine.state.command_buffer = "'<,'>".to_string();
                self.engine.state.command_cursor_pos = self.engine.state.command_buffer.len();
                self.exit_visual(Mode::Command);
            }

            _ => {}
        }
    }
}

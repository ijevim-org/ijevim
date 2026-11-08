use crate::editor::Editor;
use crate::types::{Mode, PendingOperator, Position, VisualType};
use crate::undo::{Edit, EditType};
use crossterm::event::KeyCode;

#[derive(Clone, Copy)]
enum CaseTransform {
    Toggle,
    Lower,
    Upper,
}

impl Editor {
    /// Save the current visual selection so `gv` and `:'<,'>` can use it.
    fn save_visual_selection(&mut self) {
        self.engine.state.last_visual_start = self.engine.state.visual_start;
        self.engine.state.last_visual_end = Some(self.engine.state.cursor);
        self.engine.state.last_visual_type = self.engine.state.visual_type;
    }

    /// Leave Visual mode, remembering the selection for `gv`.
    fn exit_visual(&mut self, to: Mode) {
        self.save_visual_selection();
        self.engine.state.visual_start = None;
        self.engine.state.visual_type = None;
        self.transition_mode(to);
    }

    /// Register selected by a `"x` prefix, defaulting to the unnamed register.
    fn visual_register(&mut self) -> char {
        match self.engine.operator_state.register.take() {
            Some(c) if c != '\0' => c,
            _ => '"',
        }
    }

    fn visual_count(&mut self) -> usize {
        self.take_count()
    }

    fn goto_line_first_non_blank(&mut self, line: usize) {
        let line = line.clamp(1, self.engine.buffer.content_line_count());
        self.engine.state.cursor.line = line;
        self.engine.state.cursor.col = self.first_non_blank_col(line);
        self.needs_render = true;
    }

    fn visual_goto_line(&mut self, line: usize) {
        self.goto_line_first_non_blank(line);
    }

    /// Screen-relative line motion: H (top), M (middle), L (bottom).
    fn visual_screen_line(&mut self, key: char) {
        let window = &self.engine.window_layout.windows[self.engine.window_layout.focused];
        let top = window.scroll_top.max(1);
        let height = window.row_end.saturating_sub(window.row_start);
        let bottom = (top + height.saturating_sub(1)).min(self.engine.buffer.content_line_count());
        let target = match key {
            'H' => top,
            'M' => (top + bottom) / 2,
            'L' => bottom,
            _ => return,
        };
        self.goto_line_first_non_blank(target.max(1));
    }

    /// Move the selection anchor and cursor to the given inclusive range.
    fn select_range(&mut self, start: Position, end: Position) {
        self.engine.state.visual_start = Some(start);
        self.engine.state.cursor = end;
        self.needs_render = true;
    }

    /// Inclusive selection bounds, normalized.
    fn selection_bounds(&self) -> Option<(Position, Position)> {
        let start = self.engine.state.visual_start?;
        let cursor = self.engine.state.cursor;
        let (a, b) =
            if start.line < cursor.line || (start.line == cursor.line && start.col <= cursor.col) {
                (start, cursor)
            } else {
                (cursor, start)
            };
        Some((a, b))
    }

    fn selection_text(&self) -> String {
        let Some((start, end)) = self.selection_bounds() else {
            return String::new();
        };
        match self
            .engine
            .state
            .visual_type
            .unwrap_or(VisualType::Character)
        {
            VisualType::Character => {
                self.engine
                    .buffer
                    .get_char_range(start.line, start.col, end.line, end.col + 1)
            }
            VisualType::Line => {
                let (s, e) = self.normalize_line_selection(start.line, end.line);
                self.engine.buffer.get_line_range(s, e)
            }
            VisualType::Block => {
                self.engine
                    .buffer
                    .get_block_range(start.line, start.col, end.line, end.col + 1)
            }
        }
    }

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

    /// `I` / `A` for block selections is handled in the keymap; this is the
    /// shared entry point for all visual types.
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

    /// Block selection bounds: `(start_line, end_line, start_col, end_col)`,
    /// all inclusive.
    fn block_bounds(&self) -> (usize, usize, usize, usize) {
        let anchor = self
            .engine
            .state
            .visual_start
            .unwrap_or(self.engine.state.cursor);
        let cursor = self.engine.state.cursor;
        (
            anchor.line.min(cursor.line),
            anchor.line.max(cursor.line),
            anchor.col.min(cursor.col),
            anchor.col.max(cursor.col),
        )
    }

    fn visual_yank(&mut self, linewise: bool) {
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
    fn visual_delete_to_eol(&mut self) {
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
    fn visual_delete(&mut self) {
        let register = self.visual_register();
        let edits = self.visual_delete_collect(register);
        self.push_undo_group(edits);
    }

    fn visual_delete_lines(&mut self) {
        let register = self.visual_register();
        let edits = self.visual_delete_lines_collect(register);
        self.push_undo_group(edits);
    }

    fn visual_change(&mut self, linewise: bool) {
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
    fn visual_delete_collect(&mut self, register: char) -> Vec<Edit> {
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
    fn visual_delete_lines_collect(&mut self, register: char) -> Vec<Edit> {
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
    fn visual_put(&mut self, _before: bool) {
        let register = self.visual_register();
        let content = self.engine.register.get(register);
        if content.is_empty() {
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

    fn visual_join(&mut self, with_space: bool) {
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
    fn join_lines_without_space(&mut self) {
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

    fn transform_selection(&mut self, transform: CaseTransform) {
        let Some((start, end)) = self.selection_bounds() else {
            return;
        };
        let vtype = self
            .engine
            .state
            .visual_type
            .unwrap_or(VisualType::Character);
        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();
        let mut edits = Vec::new();

        match vtype {
            VisualType::Character => {
                let (s_line, s_col, e_line, e_col) = self.normalize_selection(&start, &end);
                if s_line == e_line {
                    self.transform_line(
                        s_line,
                        s_col,
                        e_col,
                        transform,
                        cursor_before,
                        mod_count,
                        &mut edits,
                    );
                } else {
                    self.transform_line(
                        s_line,
                        s_col,
                        self.line_content_len(s_line),
                        transform,
                        cursor_before,
                        mod_count,
                        &mut edits,
                    );
                    for line in (s_line + 1)..e_line {
                        let len = self.line_content_len(line);
                        self.transform_line(
                            line,
                            0,
                            len,
                            transform,
                            cursor_before,
                            mod_count,
                            &mut edits,
                        );
                    }
                    self.transform_line(
                        e_line,
                        0,
                        e_col,
                        transform,
                        cursor_before,
                        mod_count,
                        &mut edits,
                    );
                }
            }
            VisualType::Line => {
                let (s, e) = self.normalize_line_selection(start.line, end.line);
                for line in s..=e {
                    let len = self.line_content_len(line);
                    self.transform_line(
                        line,
                        0,
                        len,
                        transform,
                        cursor_before,
                        mod_count,
                        &mut edits,
                    );
                }
            }
            VisualType::Block => {
                let s_line = start.line.min(end.line);
                let e_line = start.line.max(end.line);
                let s_col = start.col.min(end.col);
                let e_col = start.col.max(end.col) + 1;
                for line in s_line..=e_line {
                    self.transform_line(
                        line,
                        s_col,
                        e_col,
                        transform,
                        cursor_before,
                        mod_count,
                        &mut edits,
                    );
                }
            }
        }

        self.push_undo_group(edits);
        self.engine.state.cursor = start;
        self.on_buffer_modified();
        self.exit_visual(Mode::Normal);
    }

    #[allow(clippy::too_many_arguments)]
    fn transform_line(
        &mut self,
        line: usize,
        start_col: usize,
        end_col: usize,
        transform: CaseTransform,
        cursor_before: Position,
        mod_count: usize,
        edits: &mut Vec<Edit>,
    ) {
        let chars = self.engine.buffer.line_chars(line);
        if start_col >= chars.len() {
            return;
        }
        let end_col = end_col.min(chars.len());
        let old: String = chars[start_col..end_col].iter().collect();
        let new: String = old
            .chars()
            .map(|c| match transform {
                CaseTransform::Toggle => {
                    if c.is_uppercase() {
                        c.to_lowercase().next().unwrap_or(c)
                    } else if c.is_lowercase() {
                        c.to_uppercase().next().unwrap_or(c)
                    } else {
                        c
                    }
                }
                CaseTransform::Lower => c.to_lowercase().next().unwrap_or(c),
                CaseTransform::Upper => c.to_uppercase().next().unwrap_or(c),
            })
            .collect();
        if old == new {
            return;
        }
        let char_start = self.engine.buffer.line_to_char(line - 1) + start_col;
        self.engine
            .buffer
            .remove_range(char_start, char_start + old.chars().count());
        self.engine.buffer.insert(line, start_col, &new);
        edits.push(Edit {
            edit_type: EditType::Replace {
                line,
                col: start_col,
                old_text: old,
                new_text: new,
            },
            cursor_before,
            cursor_after: self.engine.state.cursor,
            modification_count: mod_count,
        });
    }

    /// Replace every character in the visual selection with `ch`.
    pub(crate) fn visual_replace_char(&mut self, ch: char) {
        let Some((start, end)) = self.selection_bounds() else {
            self.engine.operator_state.replace_char = None;
            self.transition_mode(Mode::Normal);
            return;
        };
        let vtype = self
            .engine
            .state
            .visual_type
            .unwrap_or(VisualType::Character);
        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();
        let mut edits = Vec::new();
        let mut replace_line =
            |editor: &mut Editor, line: usize, start_col: usize, end_col: usize| {
                let chars = editor.engine.buffer.line_chars(line);
                if start_col >= chars.len() {
                    return;
                }
                let end_col = end_col.min(chars.len());
                let old: String = chars[start_col..end_col].iter().collect();
                let new: String = old
                    .chars()
                    .map(|c| if c == '\n' { c } else { ch })
                    .collect();
                if old == new {
                    return;
                }
                let char_start = editor.engine.buffer.line_to_char(line - 1) + start_col;
                editor
                    .engine
                    .buffer
                    .remove_range(char_start, char_start + old.chars().count());
                editor.engine.buffer.insert(line, start_col, &new);
                edits.push(Edit {
                    edit_type: EditType::Replace {
                        line,
                        col: start_col,
                        old_text: old,
                        new_text: new,
                    },
                    cursor_before,
                    cursor_after: editor.engine.state.cursor,
                    modification_count: mod_count,
                });
            };

        match vtype {
            VisualType::Character => {
                let (s_line, s_col, e_line, e_col) = self.normalize_selection(&start, &end);
                if s_line == e_line {
                    replace_line(self, s_line, s_col, e_col);
                } else {
                    replace_line(self, s_line, s_col, self.line_content_len(s_line));
                    for line in (s_line + 1)..e_line {
                        let len = self.line_content_len(line);
                        replace_line(self, line, 0, len);
                    }
                    replace_line(self, e_line, 0, e_col);
                }
            }
            VisualType::Line => {
                let (s, e) = self.normalize_line_selection(start.line, end.line);
                for line in s..=e {
                    let len = self.line_content_len(line);
                    replace_line(self, line, 0, len);
                }
            }
            VisualType::Block => {
                let s_line = start.line.min(end.line);
                let e_line = start.line.max(end.line);
                let s_col = start.col.min(end.col);
                let e_col = start.col.max(end.col) + 1;
                for line in s_line..=e_line {
                    replace_line(self, line, s_col, e_col);
                }
            }
        }

        self.push_undo_group(edits);
        self.engine.state.cursor = start;
        self.on_buffer_modified();
        self.engine.operator_state.replace_char = None;
        self.exit_visual(Mode::Normal);
    }

    /// Resolve `i`/`a` text objects into a visual selection.
    fn visual_text_object(&mut self, inner: bool, key: KeyCode) {
        let KeyCode::Char(c) = key else {
            return;
        };
        let cursor = self.engine.state.cursor;
        let line = cursor.line;
        let col = cursor.col;
        let mut range: Option<(Position, Position)> = None;

        match c {
            'w' | 'W' => {
                let (word, start, end) = if c == 'W' {
                    self.engine.buffer.get_word_range_upper(line, col)
                } else {
                    self.engine.buffer.get_word_range(line, col)
                };
                if !word.is_empty() {
                    let chars = self.engine.buffer.line_chars(line);
                    let (mut s, mut e) = (start, end);
                    if !inner {
                        while s > 0 && chars[s - 1].is_whitespace() {
                            s -= 1;
                        }
                        while e < chars.len() && chars[e].is_whitespace() {
                            e += 1;
                        }
                    }
                    range = Some((
                        Position { line, col: s },
                        Position {
                            line,
                            col: e.saturating_sub(1),
                        },
                    ));
                }
            }
            '"' | '\'' => {
                let chars = self.engine.buffer.line_chars(line);
                let mut open = None;
                for i in (0..col).rev() {
                    if chars.get(i) == Some(&c) {
                        open = Some(i);
                        break;
                    }
                }
                let mut close = None;
                if let Some(i) = chars.iter().skip(col + 1).position(|&ch| ch == c) {
                    close = Some(col + 1 + i);
                }
                if let (Some(o), Some(cl)) = (open, close) {
                    let (s, e) = if inner { (o + 1, cl) } else { (o, cl + 1) };
                    if e > s {
                        range = Some((Position { line, col: s }, Position { line, col: e - 1 }));
                    }
                }
            }
            '(' | ')' | 'b' | '[' | ']' | '{' | '}' | 'B' | '<' | '>' => {
                let (open, close) = match c {
                    '(' | ')' | 'b' => ('(', ')'),
                    '[' | ']' => ('[', ']'),
                    '{' | '}' | 'B' => ('{', '}'),
                    _ => ('<', '>'),
                };
                if let Some((s, e)) = self.find_enclosing_pair(open, close) {
                    let (s, e) = if inner {
                        (
                            Position {
                                line: s.0,
                                col: s.1 + 1,
                            },
                            Position {
                                line: e.0,
                                col: e.1.saturating_sub(1),
                            },
                        )
                    } else {
                        (
                            Position {
                                line: s.0,
                                col: s.1,
                            },
                            Position {
                                line: e.0,
                                col: e.1,
                            },
                        )
                    };
                    if (e.line, e.col) >= (s.line, s.col) {
                        range = Some((s, e));
                    }
                }
            }
            'p' => {
                let mut s = line;
                while s > 1 && !self.engine.buffer.get_line(s - 1).trim().is_empty() {
                    s -= 1;
                }
                let mut e = line;
                let last = self.engine.buffer.content_line_count();
                while e < last && !self.engine.buffer.get_line(e + 1).trim().is_empty() {
                    e += 1;
                }
                let start_col = if inner {
                    self.first_non_blank_col(s)
                } else {
                    0
                };
                range = Some((
                    Position {
                        line: s,
                        col: start_col,
                    },
                    Position {
                        line: e,
                        col: self.line_content_len(e).saturating_sub(1),
                    },
                ));
            }
            _ => {}
        }

        if let Some((start, end)) = range {
            self.select_range(start, end);
        }
    }

    fn find_enclosing_pair(
        &self,
        open: char,
        close: char,
    ) -> Option<((usize, usize), (usize, usize))> {
        let cursor = self.engine.state.cursor;
        let mut open_pos = None;
        'outer: for line in (1..=cursor.line).rev() {
            let chars = self.engine.buffer.line_chars(line);
            let start_col = if line == cursor.line {
                cursor.col.min(chars.len())
            } else {
                chars.len()
            };
            for i in (0..start_col).rev() {
                if chars.get(i) == Some(&open) {
                    open_pos = Some((line, i));
                    break 'outer;
                }
                if chars.get(i) == Some(&close) {
                    // A closer before the cursor on the same line means the
                    // pair opened earlier; keep scanning left for the opener.
                }
            }
        }
        let (open_line, open_col) = open_pos?;

        let mut depth = 0usize;
        for line in open_line..=self.engine.buffer.content_line_count() {
            let chars = self.engine.buffer.line_chars(line);
            let start = if line == open_line { open_col } else { 0 };
            for (i, &ch) in chars.iter().enumerate().skip(start) {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(((open_line, open_col), (line, i)));
                    }
                }
            }
        }
        None
    }

    fn normalize_selection(
        &self,
        start: &Position,
        end: &Position,
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

use crate::editor::Editor;

impl Editor {
    pub(crate) fn page_down(&mut self) {
        let terminal_rows = self.terminal.rows() as usize;
        let line_count = self.engine.buffer.line_count();
        self.engine.state.cursor.line = (self.engine.state.cursor.line + terminal_rows)
            .min(line_count)
            .max(1);
        let len = self
            .engine
            .buffer
            .get_line(self.engine.state.cursor.line)
            .len();
        self.engine.state.cursor.col = self.engine.state.cursor.col.min(len.saturating_sub(1));
    }

    pub(crate) fn page_up(&mut self) {
        let terminal_rows = self.terminal.rows() as usize;
        self.engine.state.cursor.line = self
            .engine
            .state
            .cursor
            .line
            .saturating_sub(terminal_rows)
            .max(1);
        let len = self
            .engine
            .buffer
            .get_line(self.engine.state.cursor.line)
            .len();
        self.engine.state.cursor.col = self.engine.state.cursor.col.min(len.saturating_sub(1));
    }

    pub(crate) fn scroll_by(&mut self, lines: usize, forward: bool) {
        let line_count = self.engine.buffer.line_count();
        if forward {
            self.engine.state.cursor.line = (self.engine.state.cursor.line + lines)
                .min(line_count)
                .max(1);
        } else {
            self.engine.state.cursor.line =
                self.engine.state.cursor.line.saturating_sub(lines).max(1);
        }
    }

    pub fn scroll_up(&mut self) {
        let rows = self.terminal.rows() as usize;
        self.scroll_by(rows, false);
        self.needs_render = true;
    }

    pub fn scroll_down(&mut self) {
        let rows = self.terminal.rows() as usize;
        self.scroll_by(rows, true);
        self.needs_render = true;
    }

    pub fn scroll_up_one(&mut self) {
        if self.engine.state.cursor.line > 1 {
            self.engine.state.cursor.line -= 1;
        }
    }

    pub fn scroll_down_one(&mut self) {
        let line_count = self.engine.buffer.line_count();
        if self.engine.state.cursor.line < line_count {
            self.engine.state.cursor.line += 1;
        }
    }

    pub fn scroll_cursor_to_center(&mut self) {
        let terminal_rows = self.terminal.rows() as usize;
        let visible_rows = terminal_rows.saturating_sub(2);
        let scroll_pos = self
            .engine
            .state
            .cursor
            .line
            .saturating_sub(visible_rows / 2);
        self.engine.state.cursor.line = scroll_pos.max(1);
    }

    pub fn scroll_cursor_to_top(&mut self) {
        self.engine.state.cursor.line = 1;
    }

    pub fn scroll_cursor_to_bottom(&mut self) {
        let terminal_rows = self.terminal.rows() as usize;
        let visible_rows = terminal_rows.saturating_sub(2);
        let line_count = self.engine.buffer.line_count();
        self.engine.state.cursor.line = (line_count.saturating_sub(visible_rows) + 1).max(1);
    }

    pub(crate) fn cursor_right(&mut self, n: usize) {
        let max_col = self
            .engine
            .buffer
            .line_max_col(self.engine.state.cursor.line);
        self.engine.state.cursor.col = (self.engine.state.cursor.col + n).min(max_col);
        self.needs_render = true;
    }

    pub(crate) fn cursor_left(&mut self, n: usize) {
        self.engine.state.cursor.col = self.engine.state.cursor.col.saturating_sub(n);
        self.needs_render = true;
    }

    pub(crate) fn cursor_down(&mut self, n: usize) {
        let line_count = self.engine.buffer.line_count();
        self.engine.state.cursor.line = (self.engine.state.cursor.line + n).min(line_count);
        let max_col = self
            .engine
            .buffer
            .line_max_col(self.engine.state.cursor.line);
        self.engine.state.cursor.col = self.engine.state.cursor.col.min(max_col);
        self.needs_render = true;
    }

    pub(crate) fn cursor_up(&mut self, n: usize) {
        self.engine.state.cursor.line = self.engine.state.cursor.line.saturating_sub(n).max(1);
        let max_col = self
            .engine
            .buffer
            .line_max_col(self.engine.state.cursor.line);
        self.engine.state.cursor.col = self.engine.state.cursor.col.min(max_col);
        self.needs_render = true;
    }

    pub(crate) fn cursor_line_start(&mut self) {
        self.engine.state.cursor.col = 0;
        self.needs_render = true;
    }

    /// Vim's `$`: the cursor sits *on* the last character.
    pub(crate) fn cursor_line_end(&mut self) {
        self.engine.state.cursor.col = self
            .engine
            .buffer
            .line_max_col(self.engine.state.cursor.line);
        self.needs_render = true;
    }

    /// Emacs' `C-e`: point rests *before* the newline, i.e. one past the last
    /// character. Kept separate because the two editors disagree here.
    pub(crate) fn cursor_line_end_point(&mut self) {
        self.engine.state.cursor.col = self
            .engine
            .buffer
            .line_content_len(self.engine.state.cursor.line);
        self.needs_render = true;
    }

    /// First column of the next word, or the last column of the line when
    /// there is none.
    ///
    /// Vim's `w` skips the rest of the current word *and* the blanks that
    /// follow it; stopping at the first blank leaves the cursor in the gap
    /// (which then breaks `db`, `cw`, ...).
    pub(crate) fn word_forward_col(&self, line: usize, col: usize) -> usize {
        let chars = self.engine.buffer.line_chars(line);
        let content_len = self.engine.buffer.line_content_len(line);
        let is_word = |c: char| !c.is_whitespace();

        let mut i = col.min(content_len);
        while i < content_len && is_word(chars[i]) {
            i += 1;
        }
        while i < content_len && !is_word(chars[i]) {
            i += 1;
        }
        i.min(content_len.saturating_sub(1))
    }

    /// First column of the word before the one under the cursor.
    ///
    /// Used by `b`, `db`, `cb`: the cursor must land on the start of the
    /// preceding word, skipping the blanks in between.
    pub(crate) fn word_back_start(&self, line: usize, col: usize) -> usize {
        let chars = self.engine.buffer.line_chars(line);
        if col == 0 || chars.is_empty() {
            return 0;
        }
        let is_word = |c: char| !c.is_whitespace();

        let mut i = col.saturating_sub(1).min(chars.len() - 1);
        while i > 0 && !is_word(chars[i]) {
            i -= 1;
        }
        while i > 0 && is_word(chars[i - 1]) {
            i -= 1;
        }
        i
    }

    pub(crate) fn move_word_forward(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        self.engine.state.cursor.col = self.word_forward_col(line, col);
        self.needs_render = true;
    }

    pub(crate) fn move_word_backward(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        self.engine.state.cursor.col = self.word_back_start(line, col);
        self.needs_render = true;
    }

    /// Last column of the word the cursor is on (or of the next one).
    pub(crate) fn word_end_col(&self, line: usize, col: usize) -> usize {
        let chars = self.engine.buffer.line_chars(line);
        let content_len = self.engine.buffer.line_content_len(line);
        if content_len == 0 {
            return 0;
        }
        let is_word = |c: char| !c.is_whitespace();

        let mut i = col.min(content_len.saturating_sub(1));
        while i < content_len && !is_word(chars[i]) {
            i += 1;
        }
        while i + 1 < content_len && is_word(chars[i + 1]) {
            i += 1;
        }
        i.min(content_len - 1)
    }

    pub(crate) fn move_word_end(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        self.engine.state.cursor.col = self.word_end_col(line, col);
        self.needs_render = true;
    }

    pub(crate) fn find_char(&mut self, ch: char, till: bool, forward: bool) -> bool {
        let current_line = self.engine.state.cursor.line;
        let start_col = self.engine.state.cursor.col;
        let total_lines = self.engine.buffer.line_count();

        if forward {
            // Search from start_col+1 on the current line, then subsequent lines.
            let chars: Vec<char> = self.engine.buffer.line_chars(current_line);
            for (i, &c) in chars.iter().enumerate().skip(start_col + 1) {
                if c == ch {
                    if till {
                        self.engine.state.cursor.col = i.saturating_sub(1);
                    } else {
                        self.engine.state.cursor.col = i;
                    }
                    return true;
                }
            }
            // Not found on current line — search subsequent lines.
            for line_idx in (current_line + 1)..=total_lines {
                let chars: Vec<char> = self.engine.buffer.line_chars(line_idx);
                for (i, &c) in chars.iter().enumerate() {
                    if c == ch {
                        self.engine.state.cursor.line = line_idx;
                        if till {
                            // `t` forward to next line: place cursor at end of previous line.
                            if line_idx > 1 {
                                let prev_chars: Vec<char> =
                                    self.engine.buffer.line_chars(line_idx - 1);
                                self.engine.state.cursor.col = prev_chars.len().saturating_sub(1);
                            } else {
                                self.engine.state.cursor.col = 0;
                            }
                        } else {
                            self.engine.state.cursor.col = i;
                        }
                        return true;
                    }
                }
            }
        } else {
            // Search from start_col-1 on the current line, then preceding lines.
            let chars: Vec<char> = self.engine.buffer.line_chars(current_line);
            for i in (0..start_col).rev() {
                if chars[i] == ch {
                    if till {
                        self.engine.state.cursor.col = (i + 1).min(chars.len().saturating_sub(1));
                    } else {
                        self.engine.state.cursor.col = i;
                    }
                    return true;
                }
            }
            // Not found on current line — search preceding lines.
            if current_line > 1 {
                for line_idx in (1..current_line).rev() {
                    let chars: Vec<char> = self.engine.buffer.line_chars(line_idx);
                    if let Some(pos) = chars.iter().rposition(|&c| c == ch) {
                        self.engine.state.cursor.line = line_idx;
                        if till {
                            // `T` backward to preceding line: place cursor at start of next line.
                            self.engine.state.cursor.col = 0;
                        } else {
                            self.engine.state.cursor.col = pos;
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn repeat_find(&mut self, forward: bool) -> bool {
        if let Some(ch) = self.engine.search_state.last_fchar {
            let till = self.engine.search_state.last_fchar_till;
            self.find_char(ch, till, forward)
        } else {
            false
        }
    }

    pub(crate) fn jump_to_matching_bracket(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let line_chars: Vec<char> = self.engine.buffer.line_chars(line);

        if col >= line_chars.len() {
            return;
        }

        let ch = line_chars[col];

        let matching = match ch {
            '(' => ')',
            ')' => '(',
            '[' => ']',
            ']' => '[',
            '{' => '}',
            '}' => '{',
            _ => return,
        };

        let direction: i32 = match ch {
            '(' | '[' | '{' => 1,
            ')' | ']' | '}' => -1,
            _ => return,
        };

        let mut count = 1;
        let mut current_line = line;
        let mut current_col = col;

        loop {
            if direction > 0 {
                current_col += 1;
                if current_col >= self.engine.buffer.line_char_len(current_line) {
                    current_line += 1;
                    if current_line > self.engine.buffer.line_count() {
                        break;
                    }
                    current_col = 0;
                }
            } else {
                if current_col == 0 {
                    if current_line == 1 {
                        break;
                    }
                    current_line -= 1;
                    current_col = self
                        .engine
                        .buffer
                        .line_char_len(current_line)
                        .saturating_sub(1);
                } else {
                    current_col -= 1;
                }
            }

            let cur_chars: Vec<char> = self.engine.buffer.line_chars(current_line);
            if current_col < cur_chars.len() {
                let c = cur_chars[current_col];
                if c == ch {
                    count += 1;
                } else if c == matching {
                    count -= 1;
                    if count == 0 {
                        self.engine.state.cursor.line = current_line;
                        self.engine.state.cursor.col = current_col;
                        return;
                    }
                }
            }

            if current_line > self.engine.buffer.line_count() || current_line < 1 {
                break;
            }
        }
    }
}

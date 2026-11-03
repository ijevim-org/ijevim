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
    /// follow it, crossing line breaks. An empty line counts as a word stop,
    /// which is why `w` lands on column 0 of a blank line instead of skipping
    /// it.
    pub(crate) fn word_forward_pos(&self, line: usize, col: usize, big: bool) -> (usize, usize) {
        let last = self.engine.buffer.content_line_count();
        let mut l = line.clamp(1, last);
        let mut chars = self.engine.buffer.line_chars(l);
        let mut len = content_len(&chars);
        let mut c = col.min(len);

        if c < len {
            let class = word_class(chars[c], big);
            if class != WordClass::Blank {
                while c < len && word_class(chars[c], big) == class {
                    c += 1;
                }
            }
        }

        loop {
            while c < len && chars[c].is_whitespace() {
                c += 1;
            }
            if c < len {
                return (l, c);
            }
            if l >= last {
                return (l, len.saturating_sub(1));
            }
            l += 1;
            chars = self.engine.buffer.line_chars(l);
            len = content_len(&chars);
            c = 0;
            if len == 0 {
                // An empty line is a word boundary of its own.
                return (l, 0);
            }
        }
    }

    /// Start of the word before the cursor, crossing line breaks.
    pub(crate) fn word_backward_pos(&self, line: usize, col: usize, big: bool) -> (usize, usize) {
        let mut l = line.max(1);
        let mut chars = self.engine.buffer.line_chars(l);
        let mut len = content_len(&chars);
        let mut c = col.min(len.saturating_sub(1));

        loop {
            if c > 0 {
                c -= 1;
            } else if l > 1 {
                l -= 1;
                chars = self.engine.buffer.line_chars(l);
                len = content_len(&chars);
                if len == 0 {
                    // Blank lines are skipped by `b`.
                    continue;
                }
                c = len - 1;
            } else {
                return (1, 0);
            }
            if !chars[c].is_whitespace() {
                break;
            }
        }

        let class = word_class(chars[c], big);
        while c > 0 && word_class(chars[c - 1], big) == class {
            c -= 1;
        }
        (l, c)
    }

    /// Last column of the next word end, crossing line breaks.
    pub(crate) fn word_end_pos(&self, line: usize, col: usize, big: bool) -> (usize, usize) {
        let last = self.engine.buffer.content_line_count();
        let mut l = line.clamp(1, last);
        let mut chars = self.engine.buffer.line_chars(l);
        let mut len = content_len(&chars);
        let mut c = col;

        // `e` always moves at least one character forward.
        if c + 1 < len {
            c += 1;
        } else if l < last {
            l += 1;
            chars = self.engine.buffer.line_chars(l);
            len = content_len(&chars);
            c = 0;
        } else {
            return (l, len.saturating_sub(1));
        }

        loop {
            if c < len && !chars[c].is_whitespace() {
                break;
            }
            if c + 1 < len {
                c += 1;
                continue;
            }
            if l >= last {
                return (l, len.saturating_sub(1));
            }
            l += 1;
            chars = self.engine.buffer.line_chars(l);
            len = content_len(&chars);
            c = 0;
        }

        let class = word_class(chars[c], big);
        while c + 1 < len && word_class(chars[c + 1], big) == class {
            c += 1;
        }
        (l, c)
    }

    pub(crate) fn move_word_forward(&mut self) {
        self.move_word_forward_impl(false);
    }

    /// Vim's `W`: whitespace-delimited WORDs.
    pub(crate) fn move_word_forward_big(&mut self) {
        self.move_word_forward_impl(true);
    }

    fn move_word_forward_impl(&mut self, big: bool) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let (l, c) = self.word_forward_pos(line, col, big);
        self.engine.state.cursor.line = l;
        self.engine.state.cursor.col = c;
        self.needs_render = true;
    }

    pub(crate) fn move_word_backward(&mut self) {
        self.move_word_backward_impl(false);
    }

    /// Vim's `B`.
    pub(crate) fn move_word_backward_big(&mut self) {
        self.move_word_backward_impl(true);
    }

    fn move_word_backward_impl(&mut self, big: bool) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let (l, c) = self.word_backward_pos(line, col, big);
        self.engine.state.cursor.line = l;
        self.engine.state.cursor.col = c;
        self.needs_render = true;
    }

    pub(crate) fn move_word_end(&mut self) {
        self.move_word_end_impl(false);
    }

    /// Vim's `E`.
    pub(crate) fn move_word_end_big(&mut self) {
        self.move_word_end_impl(true);
    }

    fn move_word_end_impl(&mut self, big: bool) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let (l, c) = self.word_end_pos(line, col, big);
        self.engine.state.cursor.line = l;
        self.engine.state.cursor.col = c;
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
                if current_col >= self.engine.buffer.line_content_len(current_line) {
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
                    current_col = self.engine.buffer.line_max_col(current_line);
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

/// Vim's character classes for word motions: keyword characters, runs of
/// other non-blank characters, and blanks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Blank,
    Keyword,
    Punct,
}

fn word_class(c: char, big: bool) -> WordClass {
    if c.is_whitespace() {
        WordClass::Blank
    } else if big || c.is_alphanumeric() || c == '_' {
        // `W`/`B`/`E` treat every non-blank as one class.
        WordClass::Keyword
    } else {
        WordClass::Punct
    }
}

/// Line length without the trailing newline that ropey keeps.
fn content_len(chars: &[char]) -> usize {
    match chars.last() {
        Some('\n') => chars.len() - 1,
        _ => chars.len(),
    }
}

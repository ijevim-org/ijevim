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

    /// Height of the focused window in terminal rows.
    fn active_window_rows(&self) -> usize {
        self.engine
            .window_layout
            .windows
            .get(self.engine.window_layout.focused)
            .map(|w| w.row_end.saturating_sub(w.row_start).max(1))
            .unwrap_or_else(|| (self.terminal.rows() as usize).saturating_sub(2).max(1))
    }

    /// Current viewport top line of the focused window.
    fn current_scroll_top(&self) -> usize {
        self.engine
            .window_layout
            .windows
            .get(self.engine.window_layout.focused)
            .map(|w| w.scroll_top.max(1))
            .unwrap_or(1)
    }

    /// Move the viewport to `top` (1-based), clamped to the buffer.
    ///
    /// Viewport moves never drag the cursor past the window edges; the caller
    /// pulls the cursor along when it would fall off.
    fn set_viewport_top(&mut self, top: usize) {
        let rows = self.active_window_rows();
        let max_top = self
            .engine
            .buffer
            .line_count()
            .saturating_sub(rows.saturating_sub(1))
            .max(1);
        let top = top.clamp(1, max_top);
        if let Some(w) = self
            .engine
            .window_layout
            .windows
            .get_mut(self.engine.window_layout.focused)
        {
            w.scroll_top = top;
        }
        self.viewport_override = true;
        self.needs_render = true;
    }

    /// `Ctrl-e`: scroll the viewport one line down.
    pub fn scroll_down_one(&mut self) {
        let rows = self.active_window_rows();
        let top = self.current_scroll_top();
        self.set_viewport_top(top + 1);
        let top = self.current_scroll_top();
        let bottom = top + rows.saturating_sub(1);
        let cursor = &mut self.engine.state.cursor;
        cursor.line = cursor.line.clamp(top, bottom);
    }

    /// `Ctrl-y`: scroll the viewport one line up.
    pub fn scroll_up_one(&mut self) {
        let rows = self.active_window_rows();
        let top = self.current_scroll_top();
        self.set_viewport_top(top.saturating_sub(1));
        let top = self.current_scroll_top();
        let bottom = top + rows.saturating_sub(1);
        let cursor = &mut self.engine.state.cursor;
        cursor.line = cursor.line.clamp(top, bottom);
    }

    /// `zz`: center the cursor line in the viewport.
    pub fn scroll_cursor_to_center(&mut self) {
        let rows = self.active_window_rows();
        let line = self.engine.state.cursor.line;
        self.set_viewport_top(line.saturating_sub(rows / 2));
    }

    /// `zt`: put the cursor line at the top of the viewport.
    pub fn scroll_cursor_to_top(&mut self) {
        let line = self.engine.state.cursor.line;
        self.set_viewport_top(line);
    }

    /// `zb`: put the cursor line at the bottom of the viewport.
    pub fn scroll_cursor_to_bottom(&mut self) {
        let rows = self.active_window_rows();
        let line = self.engine.state.cursor.line;
        self.set_viewport_top(line.saturating_sub(rows.saturating_sub(1)));
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

    /// Vim's `f`/`F`/`t`/`T`: they never leave the current line.
    pub(crate) fn find_char(&mut self, ch: char, till: bool, forward: bool) -> bool {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let chars = self.engine.buffer.line_chars(line);
        let len = content_len(&chars);

        if forward {
            for (offset, &c) in chars[col + 1..len].iter().enumerate() {
                if c == ch {
                    let i = col + 1 + offset;
                    self.engine.state.cursor.col = if till { i - 1 } else { i };
                    return true;
                }
            }
        } else {
            for (i, &c) in chars[..col.min(len)].iter().enumerate().rev() {
                if c == ch {
                    self.engine.state.cursor.col = if till {
                        (i + 1).min(len.saturating_sub(1))
                    } else {
                        i
                    };
                    return true;
                }
            }
        }
        false
    }

    /// Vim's `ge` / `gE`: move to the end of the previous word.
    pub(crate) fn move_word_end_backward(&mut self, big: bool) {
        let mut line = self.engine.state.cursor.line;
        let mut col = self.engine.state.cursor.col;
        let mut chars = self.engine.buffer.line_chars(line);
        let mut len = content_len(&chars);

        // If the cursor is inside a word, first move to that word's start so
        // the motion really lands on the *previous* word, not the current end.
        if col < len && !chars[col].is_whitespace() {
            let class = word_class(chars[col], big);
            while col > 0 && word_class(chars[col - 1], big) == class {
                col -= 1;
            }
        }

        loop {
            if col > 0 {
                col -= 1;
            } else if line > 1 {
                line -= 1;
                chars = self.engine.buffer.line_chars(line);
                len = content_len(&chars);
                if len == 0 {
                    continue;
                }
                col = len - 1;
            } else {
                self.engine.state.cursor.line = 1;
                self.engine.state.cursor.col = 0;
                self.needs_render = true;
                return;
            }
            let c = chars.get(col).copied().unwrap_or('\n');
            if !c.is_whitespace() && c != '\n' {
                break;
            }
        }

        let class = word_class(chars[col], big);
        while col + 1 < len && word_class(chars[col + 1], big) == class {
            col += 1;
        }
        self.engine.state.cursor.line = line;
        self.engine.state.cursor.col = col;
        self.needs_render = true;
    }

    /// Vim's `}`: next paragraph (blank line), or the end of the buffer.
    pub(crate) fn paragraph_forward(&mut self) {
        let current = self.engine.state.cursor.line;
        let last = self.engine.buffer.content_line_count();
        for line in (current + 1)..=last {
            if self.engine.buffer.get_line(line).trim().is_empty() {
                self.engine.state.cursor.line = line;
                self.engine.state.cursor.col = 0;
                self.needs_render = true;
                return;
            }
        }
        self.engine.state.cursor.line = last;
        self.engine.state.cursor.col = 0;
        self.needs_render = true;
    }

    /// Vim's `{`: previous paragraph (blank line), or the start of the buffer.
    pub(crate) fn paragraph_backward(&mut self) {
        let current = self.engine.state.cursor.line;
        for line in (1..current).rev() {
            if self.engine.buffer.get_line(line).trim().is_empty() {
                self.engine.state.cursor.line = line;
                self.engine.state.cursor.col = 0;
                self.needs_render = true;
                return;
            }
        }
        self.engine.state.cursor.line = 1;
        self.engine.state.cursor.col = 0;
        self.needs_render = true;
    }

    /// Vim's `)`: next sentence end.
    pub(crate) fn sentence_forward(&mut self) {
        let last = self.engine.buffer.content_line_count();
        let mut line = self.engine.state.cursor.line;
        let mut col = self.engine.state.cursor.col;
        loop {
            let chars = self.engine.buffer.line_chars(line);
            let len = content_len(&chars);
            while col + 1 < len {
                col += 1;
                let c = chars[col];
                if matches!(c, '.' | '!' | '?')
                    && (col + 1 >= len || chars[col + 1].is_whitespace())
                {
                    col += 1;
                    while col < len && chars[col].is_whitespace() {
                        col += 1;
                    }
                    self.engine.state.cursor.line = line;
                    self.engine.state.cursor.col = col.min(len.saturating_sub(1));
                    self.needs_render = true;
                    return;
                }
            }
            if line >= last {
                self.engine.state.cursor.line = last;
                self.engine.state.cursor.col = content_len(&chars).saturating_sub(1);
                self.needs_render = true;
                return;
            }
            line += 1;
            col = 0;
        }
    }

    /// Vim's `(`: previous sentence start.
    pub(crate) fn sentence_backward(&mut self) {
        let mut line = self.engine.state.cursor.line;
        let mut col = self.engine.state.cursor.col;
        loop {
            if col > 0 {
                col -= 1;
            } else if line > 1 {
                line -= 1;
                col = content_len(&self.engine.buffer.line_chars(line));
                if col == 0 {
                    continue;
                }
                col -= 1;
            } else {
                self.engine.state.cursor.line = 1;
                self.engine.state.cursor.col = 0;
                self.needs_render = true;
                return;
            }
            let chars = self.engine.buffer.line_chars(line);
            let len = content_len(&chars);
            let is_end = matches!(chars.get(col), Some(&('.' | '!' | '?')));
            let prev_ws = col == 0 || chars.get(col - 1).is_some_and(|c| c.is_whitespace());
            if is_end && (col + 1 >= len || chars[col + 1].is_whitespace()) {
                while col > 0 && chars[col - 1].is_whitespace() {
                    col -= 1;
                }
                self.engine.state.cursor.line = line;
                self.engine.state.cursor.col = col;
                self.needs_render = true;
                return;
            }
            if prev_ws && col > 0 {
                // Candidate sentence start: move to the first non-blank.
                while col > 0 && chars[col - 1].is_whitespace() {
                    col -= 1;
                }
            }
        }
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

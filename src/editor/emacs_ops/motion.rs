use crate::editor::Editor;

impl Editor {
    /// Number of buffer lines, including the empty line after a final newline.
    fn point_last_line(&self) -> usize {
        self.engine.buffer.line_count().max(1)
    }

    /// Emacs `C-e`: point sits *before* the newline, i.e. one past the last
    /// character. Kept separate from Vim's `$`, which sits on the last char.
    pub(crate) fn cursor_line_end_point(&mut self) {
        self.engine.state.cursor.col = self
            .engine
            .buffer
            .line_content_len(self.engine.state.cursor.line);
        self.needs_render = true;
    }

    /// Emacs `C-f`: move point one position forward, crossing a newline into
    /// the next line. At the end of the buffer point does not move.
    fn point_right_once(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let len = self.engine.buffer.line_content_len(line);
        if col < len {
            self.engine.state.cursor.col += 1;
        } else if line < self.point_last_line() {
            self.engine.state.cursor.line = line + 1;
            self.engine.state.cursor.col = 0;
        }
    }

    /// Emacs `C-b`: move point one position backward, crossing a newline into
    /// the end of the previous line.
    fn point_left_once(&mut self) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        if col > 0 {
            self.engine.state.cursor.col = col - 1;
        } else if line > 1 {
            self.engine.state.cursor.line = line - 1;
            self.engine.state.cursor.col = self.engine.buffer.line_content_len(line - 1);
        }
    }

    pub(crate) fn point_right(&mut self, n: usize) {
        for _ in 0..n {
            self.point_right_once();
        }
        self.needs_render = true;
    }

    pub(crate) fn point_left(&mut self, n: usize) {
        for _ in 0..n {
            self.point_left_once();
        }
        self.needs_render = true;
    }

    /// Emacs point-aware vertical motion: the goal column may be the line's
    /// content length (the position before the newline), unlike Vim which
    /// clamps to the last character.
    fn point_vertical(&mut self, forward: bool, n: usize) {
        let last = self.point_last_line();
        for _ in 0..n {
            let line = self.engine.state.cursor.line;
            let col = self.engine.state.cursor.col;
            let next_line = if forward {
                if line >= last {
                    break;
                }
                line + 1
            } else {
                if line <= 1 {
                    break;
                }
                line - 1
            };
            self.engine.state.cursor.line = next_line;
            self.engine.state.cursor.col = col.min(self.engine.buffer.line_content_len(next_line));
        }
        self.needs_render = true;
    }

    pub(crate) fn point_down(&mut self, n: usize) {
        self.point_vertical(true, n);
    }

    pub(crate) fn point_up(&mut self, n: usize) {
        self.point_vertical(false, n);
    }

    fn point_page(&mut self, forward: bool, n: usize) {
        let rows = self.terminal.rows() as usize;
        self.point_vertical(forward, rows.saturating_mul(n));
    }

    pub(crate) fn point_page_down(&mut self, n: usize) {
        self.point_page(true, n);
    }

    pub(crate) fn point_page_up(&mut self, n: usize) {
        self.point_page(false, n);
    }

    /// Emacs `M-f`: move to the end of the next word.
    pub(crate) fn point_forward_word(&mut self, n: usize) {
        for _ in 0..n {
            let start = self.engine.state.cursor;
            match self.forward_word_bounds(start) {
                Some((_, to)) => self.engine.state.cursor = to,
                None => break,
            }
        }
        self.needs_render = true;
    }

    /// Emacs `M-b`: move to the start of the previous word.
    pub(crate) fn point_backward_word(&mut self, n: usize) {
        for _ in 0..n {
            let start = self.engine.state.cursor;
            match self.backward_word_bounds(start) {
                Some((from, _)) => self.engine.state.cursor = from,
                None => break,
            }
        }
        self.needs_render = true;
    }
}

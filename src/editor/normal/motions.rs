use crate::editor::Editor;

impl Editor {
    pub(crate) fn line_content_len(&self, line: usize) -> usize {
        self.engine
            .buffer
            .get_line(line)
            .chars()
            .take_while(|c| *c != '\n')
            .count()
    }

    /// Delete the character range `[start, end)` on the cursor line.
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
    pub(crate) fn exclusive_target(
        &self,
        from_line: usize,
        target: (usize, usize),
    ) -> (usize, usize) {
        let (line, col) = target;
        if line > from_line && col == 0 {
            (line - 1, self.line_content_len(line - 1))
        } else {
            target
        }
    }

    /// Target of `{count}w` / `{count}e`, honoring the count across lines.
    pub(crate) fn word_motion_target(
        &self,
        count: usize,
        big: bool,
        to_word_end: bool,
    ) -> (usize, usize) {
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
}

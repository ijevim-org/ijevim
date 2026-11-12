use crate::editor::Editor;
use crate::undo::{Edit, EditType};

impl Editor {
    pub(crate) fn delete_cols(&mut self, register: char, start: usize, end: usize) {
        if let Some(edit) = self.delete_cols_edit(register, start, end) {
            self.engine.undo_manager.push(edit);
        }
    }

    /// Delete the range and return its undo entry without pushing it.
    ///
    /// Multi-step operators (`3dw`, `dj`, ...) collect the entries and push
    /// them as one group, so the single `u` a user expects restores them all.
    pub(crate) fn delete_cols_edit(
        &mut self,
        register: char,
        start: usize,
        end: usize,
    ) -> Option<Edit> {
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
    pub(crate) fn delete_lines_range(
        &mut self,
        register: char,
        start_line: usize,
        end_line: usize,
    ) {
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
    pub(crate) fn change_lines(&mut self, register: char, count: usize) {
        let marker = self.undo_marker();
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
        self.defer_undo_since(marker);
        self.engine.state.cursor.line = start.min(self.engine.buffer.line_count());
        self.engine.state.cursor.col = 0;
        self.enter_insert_after_change();
    }

    /// `cj` / `ck` / `cG`: delete whole lines, keep one empty line, insert.
    pub(crate) fn change_line_range(&mut self, register: char, first: usize, last: usize) {
        let marker = self.undo_marker();
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
        self.defer_undo_since(marker);
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
    pub(super) fn yank_word(&mut self, register: char, count: usize, big: bool) {
        let from = (self.engine.state.cursor.line, self.engine.state.cursor.col);
        let target = self.word_motion_target(count, big, false);
        let target = self.exclusive_target(from.0, target);
        self.yank_span(register, from, target);
    }

    /// `ye` / `yE`: yank through the end of the word, inclusive.
    pub(super) fn yank_word_end(&mut self, register: char, count: usize, big: bool) {
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
        let marker = self.undo_marker();
        if on_blank {
            self.delete_word(register, count, big);
        } else {
            self.delete_word_end(register, count, big);
        }
        self.defer_undo_since(marker);
        // Insert at the start of the deleted text, allowing the end-of-line
        // column that Normal mode would clamp away.
        self.enter_insert_at(crate::types::Position { line, col });
    }

    /// `D`: delete from the cursor to the end of the line, without removing the
    /// line break. With a count, the span reaches the end of the count-th line.
    pub(crate) fn delete_to_line_end(&mut self, register: char, count: usize) {
        let line = self.engine.state.cursor.line;
        let col = self.engine.state.cursor.col;
        let count = count.max(1);
        let last = (line + count - 1).min(self.engine.buffer.content_line_count().max(1));
        // Vim makes a multi-line `D` linewise when it starts in column 0.
        if col == 0 && count > 1 {
            self.delete_lines_range(register, line, last);
            return;
        }
        let end = self.line_content_len(last);
        self.delete_span(register, (line, col), (last, end));
    }
}

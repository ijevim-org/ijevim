use crate::editor::Editor;
use crate::types::{Mode, Position, VisualType};

impl Editor {
    /// Save the current visual selection so `gv` and `:'<,'>` can use it.
    pub(super) fn save_visual_selection(&mut self) {
        self.engine.state.last_visual_start = self.engine.state.visual_start;
        self.engine.state.last_visual_end = Some(self.engine.state.cursor);
        self.engine.state.last_visual_type = self.engine.state.visual_type;
    }

    /// Leave Visual mode, remembering the selection for `gv`.
    pub(super) fn exit_visual(&mut self, to: Mode) {
        self.save_visual_selection();
        self.engine.state.visual_start = None;
        self.engine.state.visual_type = None;
        self.transition_mode(to);
    }

    /// Register selected by a `"x` prefix, defaulting to the unnamed register.
    pub(super) fn visual_register(&mut self) -> char {
        match self.engine.operator_state.register.take() {
            Some(c) if c != '\0' => c,
            _ => '"',
        }
    }

    pub(super) fn visual_count(&mut self) -> usize {
        self.take_count()
    }

    pub(super) fn goto_line_first_non_blank(&mut self, line: usize) {
        let line = line.clamp(1, self.engine.buffer.content_line_count());
        self.engine.state.cursor.line = line;
        self.engine.state.cursor.col = self.first_non_blank_col(line);
        self.needs_render = true;
    }

    pub(super) fn visual_goto_line(&mut self, line: usize) {
        self.goto_line_first_non_blank(line);
    }

    /// Screen-relative line motion: H (top), M (middle), L (bottom).
    pub(super) fn visual_screen_line(&mut self, key: char) {
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
    pub(super) fn select_range(&mut self, start: Position, end: Position) {
        self.engine.state.visual_start = Some(start);
        self.engine.state.cursor = end;
        self.needs_render = true;
    }

    /// Inclusive selection bounds, normalized.
    pub(super) fn selection_bounds(&self) -> Option<(Position, Position)> {
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

    pub(super) fn selection_text(&self) -> String {
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

    /// Block selection bounds: `(start_line, end_line, start_col, end_col)`,
    /// all inclusive.
    pub(super) fn block_bounds(&self) -> (usize, usize, usize, usize) {
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

    pub(super) fn normalize_selection(
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

    pub(super) fn normalize_line_selection(
        &self,
        start_line: usize,
        end_line: usize,
    ) -> (usize, usize) {
        if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        }
    }
}

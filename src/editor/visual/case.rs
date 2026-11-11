use crate::editor::Editor;
use crate::types::{Mode, Position, VisualType};
use crate::undo::{Edit, EditType};

#[derive(Clone, Copy)]
pub(super) enum CaseTransform {
    Toggle,
    Lower,
    Upper,
}

impl Editor {
    pub(super) fn transform_selection(&mut self, transform: CaseTransform) {
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
    pub(super) fn transform_line(
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
}

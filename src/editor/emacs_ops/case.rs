use super::point_after;
use crate::editor::Editor;
use crate::types::{MessageLevel, Position};
use crate::undo::{Edit, EditType};

fn capitalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_word_start = true;
    for c in text.chars() {
        if c.is_alphabetic() {
            if at_word_start {
                out.extend(c.to_uppercase());
                at_word_start = false;
            } else {
                out.extend(c.to_lowercase());
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[derive(Clone, Copy)]
enum CaseKind {
    Upper,
    Lower,
    Capitalize,
}

impl CaseKind {
    fn apply(self, text: &str) -> String {
        match self {
            CaseKind::Upper => text.to_uppercase(),
            CaseKind::Lower => text.to_lowercase(),
            CaseKind::Capitalize => capitalize_text(text),
        }
    }
}

impl Editor {
    fn transform_case_span(
        &mut self,
        from: Position,
        to: Position,
        kind: CaseKind,
        move_point: bool,
    ) {
        let old = self
            .engine
            .buffer
            .get_char_range(from.line, from.col, to.line, to.col);
        if old.is_empty() {
            return;
        }
        let new = kind.apply(&old);
        if old == new {
            if move_point {
                self.engine.state.cursor = to;
            }
            self.needs_render = true;
            return;
        }

        let mod_count = self.engine.buffer.modification_count();
        let cursor_before = self.engine.state.cursor;
        self.engine
            .buffer
            .delete_range(from.line, from.col, to.line, to.col);
        self.engine.buffer.insert(from.line, from.col, &new);
        let cursor_after = if move_point {
            point_after(from, &new)
        } else {
            self.engine.state.cursor
        };
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Replace {
                line: from.line,
                col: from.col,
                old_text: old,
                new_text: new,
            },
            cursor_before,
            cursor_after,
            modification_count: mod_count,
        });
        if move_point {
            self.engine.state.cursor = cursor_after;
        }
        self.on_buffer_modified();
        self.needs_render = true;
    }

    fn case_word(&mut self, kind: CaseKind) {
        let start = self.engine.state.cursor;
        if let Some((from, to)) = self.forward_word_bounds(start) {
            self.transform_case_span(from, to, kind, true);
        }
    }

    fn case_region(&mut self, kind: CaseKind) {
        if !self.has_active_region() {
            self.engine.state.push_message(
                MessageLevel::Info,
                "The mark is not set now, so there is no region".to_string(),
            );
            self.needs_render = true;
            return;
        }
        let mark = self.engine.state.mark.unwrap_or_default();
        let point = self.engine.state.cursor;
        let (from, to) = if (mark.line, mark.col) <= (point.line, point.col) {
            (mark, point)
        } else {
            (point, mark)
        };
        self.transform_case_span(from, to, kind, false);
    }

    pub fn upcase_word(&mut self) {
        self.case_word(CaseKind::Upper);
    }

    pub fn downcase_word(&mut self) {
        self.case_word(CaseKind::Lower);
    }

    pub fn capitalize_word(&mut self) {
        self.case_word(CaseKind::Capitalize);
    }

    pub fn upcase_region(&mut self) {
        self.case_region(CaseKind::Upper);
    }

    pub fn downcase_region(&mut self) {
        self.case_region(CaseKind::Lower);
    }
}

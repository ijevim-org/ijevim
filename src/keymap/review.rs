use crate::editor::Editor;
use crate::keymap::KeymapHandler;
use crate::review::state::{Annotation, CommentTarget};
use crate::review::{annotations, DiffStyle, ReviewSubMode};
use crossterm::event::{KeyCode, KeyModifiers};

/// Keymap handler for Review mode.
pub struct ReviewKeymap;

impl ReviewKeymap {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReviewKeymap {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapHandler for ReviewKeymap {
    fn handle_key<'e>(
        &self,
        editor: &'e mut Editor,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'e>> {
        Box::pin(async move {
            let sub_mode = editor
                .engine
                .review_state
                .as_ref()
                .map(|s| s.sub_mode)
                .unwrap_or(ReviewSubMode::FileList);

            // `q` exits review, except while typing a comment where it is text.
            if key == KeyCode::Char('q')
                && modifiers.is_empty()
                && sub_mode != ReviewSubMode::CommentInput
            {
                editor.exit_review_mode();
                return;
            }

            if key == KeyCode::Esc {
                handle_escape(editor, sub_mode);
                return;
            }

            match sub_mode {
                ReviewSubMode::FileList => handle_file_list(editor, key, modifiers),
                ReviewSubMode::DiffView => handle_diff_view(editor, key, modifiers),
                ReviewSubMode::Preview => handle_preview(editor, key, modifiers),
                ReviewSubMode::CommentInput => handle_comment_input(editor, key, modifiers),
                ReviewSubMode::Annotations => handle_annotations(editor, key, modifiers),
                ReviewSubMode::Help => {
                    if let Some(state) = &mut editor.engine.review_state {
                        state.sub_mode = ReviewSubMode::FileList;
                    }
                    editor.needs_render = true;
                }
            }
        })
    }
}

fn handle_escape(editor: &mut Editor, sub_mode: ReviewSubMode) {
    let Some(state) = &mut editor.engine.review_state else {
        return;
    };

    match sub_mode {
        ReviewSubMode::FileList => {
            editor.exit_review_mode();
            return;
        }
        ReviewSubMode::Help | ReviewSubMode::DiffView => {
            state.sub_mode = ReviewSubMode::FileList;
        }
        ReviewSubMode::Preview | ReviewSubMode::Annotations => {
            state.sub_mode = ReviewSubMode::DiffView;
        }
        ReviewSubMode::CommentInput => {
            state.comment_buffer.clear();
            state.comment_cursor = 0;
            state.comment_target = None;
            state.sub_mode = ReviewSubMode::DiffView;
        }
    }
    editor.needs_render = true;
}

// ── File List sub-mode ───────────────────────────────────────────────

fn handle_file_list(editor: &mut Editor, key: KeyCode, _modifiers: KeyModifiers) {
    let Some(state) = &mut editor.engine.review_state else {
        return;
    };

    match key {
        KeyCode::Char('j') | KeyCode::Down => {
            state.select_file(state.selected_file + 1);
            editor.needs_render = true;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if state.selected_file > 0 {
                state.select_file(state.selected_file - 1);
            }
            editor.needs_render = true;
        }
        KeyCode::Enter => {
            state.sub_mode = ReviewSubMode::DiffView;
            state.set_cursor_row(0);
            editor.needs_render = true;
        }
        KeyCode::Char('a') => {
            state.sub_mode = ReviewSubMode::Annotations;
            state.select_annotation(state.annotation_selected);
            editor.needs_render = true;
        }
        KeyCode::Char('d') => {
            state.diff_style = match state.diff_style {
                DiffStyle::Unified => DiffStyle::SideBySide,
                DiffStyle::SideBySide => DiffStyle::Unified,
            };
            editor.needs_render = true;
        }
        KeyCode::Char('?') => {
            state.sub_mode = ReviewSubMode::Help;
            editor.needs_render = true;
        }
        _ => {}
    }
}

// ── Diff View sub-mode ───────────────────────────────────────────────

fn handle_diff_view(editor: &mut Editor, key: KeyCode, modifiers: KeyModifiers) {
    let Some(state) = &mut editor.engine.review_state else {
        return;
    };

    let half_page = (state.viewport_height / 2).max(1) as isize;
    let mut handled = true;
    let mut action: Option<DiffAction> = None;

    match key {
        KeyCode::Char('j') | KeyCode::Down => state.move_cursor(1),
        KeyCode::Char('k') | KeyCode::Up => state.move_cursor(-1),
        KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
            state.move_cursor(half_page)
        }
        KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
            state.move_cursor(-half_page)
        }
        KeyCode::PageDown => {
            let page = state.viewport_height.max(1) as isize;
            state.move_cursor(page)
        }
        KeyCode::PageUp => {
            let page = state.viewport_height.max(1) as isize;
            state.move_cursor(-page)
        }
        KeyCode::Char('g') => state.set_cursor_row(0),
        KeyCode::Char('G') => {
            let last = state.current_rows().len().saturating_sub(1);
            state.set_cursor_row(last);
        }
        KeyCode::Char(']') | KeyCode::Char(')') => state.jump_hunk(true),
        KeyCode::Char('[') | KeyCode::Char('(') => state.jump_hunk(false),
        KeyCode::Char('n') => jump_annotation(editor, true),
        KeyCode::Char('p') => jump_annotation(editor, false),
        KeyCode::Enter => {
            toggle_stage(editor);
        }
        KeyCode::Char('c') => {
            let target = state.current_row().and_then(|row| {
                row.comment_line().map(|line| CommentTarget {
                    file_path: state
                        .current_file()
                        .map(|f| f.display_path().to_string())
                        .unwrap_or_default(),
                    line,
                })
            });
            match target {
                Some(target) => {
                    state.comment_target = Some(target);
                    state.comment_buffer.clear();
                    state.comment_cursor = 0;
                    state.sub_mode = ReviewSubMode::CommentInput;
                }
                None => editor
                    .engine
                    .state
                    .set_message("review: move the cursor onto a diff line first"),
            }
        }
        KeyCode::Char('a') => {
            state.sub_mode = ReviewSubMode::Annotations;
            state.select_annotation(select_annotation_near_cursor(state));
        }
        KeyCode::Char('o') => {
            state.sub_mode = ReviewSubMode::Preview;
            state.scroll.scroll_top = 0;
        }
        KeyCode::Char('d') => {
            state.diff_style = match state.diff_style {
                DiffStyle::Unified => DiffStyle::SideBySide,
                DiffStyle::SideBySide => DiffStyle::Unified,
            };
        }
        KeyCode::Char('S') => action = Some(DiffAction::StageFile),
        KeyCode::Char('U') => action = Some(DiffAction::UnstageFile),
        KeyCode::Char('X') => action = Some(DiffAction::DiscardFile),
        KeyCode::Char('r') => action = Some(DiffAction::Reload),
        KeyCode::Char('?') => state.sub_mode = ReviewSubMode::Help,
        _ => handled = false,
    }

    if let Some(action) = action {
        perform_diff_action(editor, action);
        return;
    }

    if handled {
        editor.needs_render = true;
    }
}

#[derive(Clone, Copy)]
enum DiffAction {
    StageFile,
    UnstageFile,
    DiscardFile,
    Reload,
}

fn perform_diff_action(editor: &mut Editor, action: DiffAction) {
    let message = {
        let Some(state) = &mut editor.engine.review_state else {
            return;
        };
        let Some(root) = state.project_root.clone() else {
            editor
                .engine
                .state
                .set_message("review: not inside a git repository");
            return;
        };

        if let DiffAction::DiscardFile = action {
            let Some(file) = state.current_file() else {
                return;
            };
            let path = file.display_path().to_string();
            state.pending_discard = Some(path.clone());
            editor.engine.state.set_confirmation(
                format!("Discard changes in {}?", path),
                crate::types::ConfirmAction::ReviewDiscard,
            );
            editor.needs_render = true;
            return;
        }

        let Some(file) = state.current_file().cloned() else {
            return;
        };

        let result = match action {
            DiffAction::StageFile => {
                crate::review::diff::stage_file(&file, &root).map(|_| "file staged")
            }
            DiffAction::UnstageFile => {
                crate::review::diff::unstage_file(&file, &root).map(|_| "file unstaged")
            }
            DiffAction::Reload => crate::review::diff::reload(state).map(|_| "reloaded"),
            DiffAction::DiscardFile => unreachable!(),
        };

        match result {
            Ok(verb) => {
                if !matches!(action, DiffAction::Reload) {
                    if let Err(err) = crate::review::diff::reload(state) {
                        format!("review: {} (reload failed: {})", verb, err)
                    } else {
                        format!("review: {}", verb)
                    }
                } else {
                    format!("review: {}", verb)
                }
            }
            Err(err) => format!("review: {}", err),
        }
    };

    editor.engine.state.set_message(message);
    editor.needs_render = true;
}

fn toggle_stage(editor: &mut Editor) {
    let message = {
        let Some(state) = &mut editor.engine.review_state else {
            return;
        };
        let file_index = state.selected_file;
        let hunk_index = state
            .current_row()
            .map(|row| row.hunk_index)
            .unwrap_or(state.selected_hunk);

        let Some(root) = state.project_root.clone() else {
            editor
                .engine
                .state
                .set_message("review: not inside a git repository");
            return;
        };

        let (was_staged, result) = {
            let Some(file) = state.files.get(file_index) else {
                return;
            };
            let Some(hunk) = file.hunks.get(hunk_index) else {
                return;
            };
            let result = if hunk.staged {
                crate::review::diff::unstage_hunk(hunk, file, &root)
            } else {
                crate::review::diff::stage_hunk(hunk, file, &root)
            };
            (hunk.staged, result)
        };

        match result {
            Ok(()) => {
                let verb = if was_staged { "unstaged" } else { "staged" };
                match crate::review::diff::reload(state) {
                    Ok(()) => format!("review: hunk {}", verb),
                    Err(err) => format!("review: hunk {} (reload failed: {})", verb, err),
                }
            }
            Err(err) => format!("review: {}", err),
        }
    };

    editor.engine.state.set_message(message);
    editor.needs_render = true;
}

/// Next/previous annotation relative to the cursor.
fn jump_annotation(editor: &mut Editor, forward: bool) {
    let Some(state) = &mut editor.engine.review_state else {
        return;
    };
    let Some(file) = state.current_file() else {
        return;
    };
    let file_path = file.display_path().to_string();
    let current_line = state.current_line().unwrap_or(0);

    let target = if forward {
        state
            .annotations
            .iter()
            .filter(|a| a.file_path == file_path && a.line > current_line)
            .min_by_key(|a| a.line)
            .or_else(|| {
                state
                    .annotations
                    .iter()
                    .filter(|a| a.file_path == file_path)
                    .min_by_key(|a| a.line)
            })
    } else {
        state
            .annotations
            .iter()
            .filter(|a| a.file_path == file_path && a.line < current_line)
            .max_by_key(|a| a.line)
            .or_else(|| {
                state
                    .annotations
                    .iter()
                    .filter(|a| a.file_path == file_path)
                    .max_by_key(|a| a.line)
            })
    };

    if let Some(target) = target {
        let (path, line) = (target.file_path.clone(), target.line);
        jump_to_location(state, &path, line);
        editor.needs_render = true;
    }
}

fn select_annotation_near_cursor(state: &crate::review::ReviewState) -> usize {
    let Some(file) = state.current_file() else {
        return 0;
    };
    let path = file.display_path();
    let line = state.current_line().unwrap_or(0);
    state
        .annotations
        .iter()
        .position(|a| a.file_path == path && a.line == line)
        .unwrap_or(0)
}

fn jump_to_location(state: &mut crate::review::ReviewState, file_path: &str, line: usize) {
    let file_index = state
        .files
        .iter()
        .position(|f| f.display_path() == file_path);

    if let Some(index) = file_index {
        state.select_file(index);
        let rows = state.rows_for(index);
        if let Some(row) = crate::review::rows::row_for_line(&rows, line) {
            state.set_cursor_row(row);
        }
    } else {
        // Annotation points at a file that is no longer in the diff.
        let line_hint = format!("{}:{}", file_path, line);
        state.comment_target = Some(CommentTarget {
            file_path: line_hint,
            line,
        });
    }
}

// ── Preview sub-mode ─────────────────────────────────────────────────

fn handle_preview(editor: &mut Editor, key: KeyCode, modifiers: KeyModifiers) {
    let Some(state) = &mut editor.engine.review_state else {
        return;
    };

    let half_page = (state.viewport_height / 2).max(1);
    match key {
        KeyCode::Char('j') | KeyCode::Down => {
            state.scroll.scroll_top = state.scroll.scroll_top.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.scroll.scroll_top = state.scroll.scroll_top.saturating_sub(1);
        }
        KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
            state.scroll.scroll_top = state.scroll.scroll_top.saturating_add(half_page);
        }
        KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
            state.scroll.scroll_top = state.scroll.scroll_top.saturating_sub(half_page);
        }
        KeyCode::Char('g') => state.scroll.scroll_top = 0,
        _ => return,
    }
    editor.needs_render = true;
}

// ── Annotation browser sub-mode ──────────────────────────────────────

fn handle_annotations(editor: &mut Editor, key: KeyCode, _modifiers: KeyModifiers) {
    let Some(state) = &mut editor.engine.review_state else {
        return;
    };

    match key {
        KeyCode::Char('j') | KeyCode::Down => {
            state.select_annotation(state.annotation_selected + 1);
            editor.needs_render = true;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let next = state.annotation_selected.saturating_sub(1);
            state.select_annotation(next);
            editor.needs_render = true;
        }
        KeyCode::Enter => {
            let Some(ann) = state.annotations.get(state.annotation_selected) else {
                return;
            };
            let (path, line) = (ann.file_path.clone(), ann.line);
            jump_to_location(state, &path, line);
            state.sub_mode = ReviewSubMode::DiffView;
            editor.needs_render = true;
        }
        KeyCode::Char('x') => {
            state.annotation_pending_delete = false;
            let Some(ann) = state.annotations.get(state.annotation_selected) else {
                return;
            };
            let id = ann.id.clone();
            let Some(root) = state.project_root.clone() else {
                editor
                    .engine
                    .state
                    .set_message("review: annotations are memory-only here");
                return;
            };
            match annotations::toggle_resolved(&root, &id) {
                Ok(annotations) => {
                    state.annotations = annotations;
                    state.select_annotation(state.annotation_selected);
                    editor
                        .engine
                        .state
                        .set_message("review: annotation toggled");
                }
                Err(err) => editor.engine.state.set_message(format!("review: {}", err)),
            }
            editor.needs_render = true;
        }
        KeyCode::Char('d') => {
            if state.annotation_pending_delete {
                state.annotation_pending_delete = false;
                let Some(ann) = state.annotations.get(state.annotation_selected) else {
                    return;
                };
                let id = ann.id.clone();
                let Some(root) = state.project_root.clone() else {
                    editor
                        .engine
                        .state
                        .set_message("review: annotations are memory-only here");
                    return;
                };
                match annotations::remove_annotation(&root, &id) {
                    Ok(annotations) => {
                        state.annotations = annotations;
                        state.select_annotation(state.annotation_selected);
                        editor
                            .engine
                            .state
                            .set_message("review: annotation deleted");
                    }
                    Err(err) => editor.engine.state.set_message(format!("review: {}", err)),
                }
                editor.needs_render = true;
            } else {
                state.annotation_pending_delete = true;
                editor.needs_render = true;
            }
        }
        _ => {
            state.annotation_pending_delete = false;
        }
    }
}

// ── Comment Input sub-mode ───────────────────────────────────────────

fn handle_comment_input(editor: &mut Editor, key: KeyCode, modifiers: KeyModifiers) {
    if key == KeyCode::Char('s') && modifiers.contains(KeyModifiers::CONTROL) {
        save_comment(editor);
        return;
    }

    let Some(state) = &mut editor.engine.review_state else {
        return;
    };

    match key {
        KeyCode::Enter => {
            state.comment_buffer.insert(state.comment_cursor, '\n');
            state.comment_cursor += 1;
        }
        KeyCode::Backspace => {
            if state.comment_cursor > 0 {
                let start = prev_char_boundary(&state.comment_buffer, state.comment_cursor);
                state.comment_buffer.drain(start..state.comment_cursor);
                state.comment_cursor = start;
            }
        }
        KeyCode::Delete => {
            let len = state.comment_buffer.len();
            if state.comment_cursor < len {
                let end = next_char_boundary(&state.comment_buffer, state.comment_cursor);
                state.comment_buffer.drain(state.comment_cursor..end);
            }
        }
        KeyCode::Left => {
            state.comment_cursor = prev_char_boundary(&state.comment_buffer, state.comment_cursor);
        }
        KeyCode::Right => {
            state.comment_cursor = next_char_boundary(&state.comment_buffer, state.comment_cursor);
        }
        KeyCode::Up => {
            state.comment_cursor = move_vertical(&state.comment_buffer, state.comment_cursor, true);
        }
        KeyCode::Down => {
            state.comment_cursor =
                move_vertical(&state.comment_buffer, state.comment_cursor, false);
        }
        KeyCode::Home => {
            state.comment_cursor = line_start(&state.comment_buffer, state.comment_cursor);
        }
        KeyCode::End => {
            state.comment_cursor = line_end(&state.comment_buffer, state.comment_cursor);
        }
        KeyCode::Char(ch) => {
            state.comment_buffer.insert(state.comment_cursor, ch);
            state.comment_cursor += ch.len_utf8();
        }
        _ => return,
    }
    editor.needs_render = true;
}

fn save_comment(editor: &mut Editor) {
    let Some(state) = &mut editor.engine.review_state else {
        return;
    };

    let text = state.comment_buffer.trim().to_string();
    if text.is_empty() {
        state.sub_mode = ReviewSubMode::DiffView;
        state.comment_target = None;
        editor
            .engine
            .state
            .set_message("review: empty comment discarded");
        editor.needs_render = true;
        return;
    }

    let target = state.comment_target.clone().or_else(|| {
        let line = state.current_line()?;
        let path = state.current_file()?.display_path().to_string();
        Some(CommentTarget {
            file_path: path,
            line,
        })
    });

    let Some(target) = target else {
        editor
            .engine
            .state
            .set_message("review: no line selected for the comment");
        return;
    };

    let annotation = Annotation {
        id: annotations::generate_annotation_id(),
        file_path: target.file_path,
        line: target.line,
        end_line: None,
        text,
        resolved: false,
        created_at: annotations::now_iso8601(),
    };

    if let Some(root) = state.project_root.clone() {
        match annotations::add_annotation(&root, annotation.clone()) {
            Ok(saved) => {
                state.annotations = saved;
                editor.engine.state.set_message("review: annotation saved");
            }
            Err(err) => {
                state.annotations.push(annotation);
                editor
                    .engine
                    .state
                    .set_message(format!("review: annotation kept in memory ({})", err));
            }
        }
    } else {
        state.annotations.push(annotation);
        editor
            .engine
            .state
            .set_message("review: annotation saved in memory");
    }

    state.comment_buffer.clear();
    state.comment_cursor = 0;
    state.comment_target = None;
    state.sub_mode = ReviewSubMode::DiffView;
    editor.needs_render = true;
}

// ── Text helpers ─────────────────────────────────────────────────────

fn prev_char_boundary(s: &str, pos: usize) -> usize {
    let mut new_pos = pos.saturating_sub(1);
    while new_pos > 0 && !s.is_char_boundary(new_pos) {
        new_pos -= 1;
    }
    new_pos
}

fn next_char_boundary(s: &str, pos: usize) -> usize {
    let len = s.len();
    if pos >= len {
        return len;
    }
    let mut new_pos = pos + 1;
    while new_pos < len && !s.is_char_boundary(new_pos) {
        new_pos += 1;
    }
    new_pos
}

fn line_start(s: &str, pos: usize) -> usize {
    s[..pos.min(s.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0)
}

fn line_end(s: &str, pos: usize) -> usize {
    let pos = pos.min(s.len());
    s[pos..].find('\n').map(|i| pos + i).unwrap_or(s.len())
}

fn move_vertical(s: &str, pos: usize, up: bool) -> usize {
    let start = line_start(s, pos);
    let column = s[start..pos].chars().count();

    if up {
        if start == 0 {
            return pos;
        }
        let prev_start = line_start(s, start - 1);
        let prev_end = start - 1;
        prev_start + byte_for_column(&s[prev_start..prev_end], column)
    } else {
        let end = line_end(s, pos);
        if end >= s.len() {
            return pos;
        }
        let next_start = end + 1;
        let next_end = line_end(s, next_start);
        next_start + byte_for_column(&s[next_start..next_end], column)
    }
}

fn byte_for_column(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map(|(index, _)| index)
        .unwrap_or(line.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_movement_keeps_column() {
        let text = "abc\ndefg\nhi";
        let pos = 5; // between 'e' and 'f' on line 2 (column 1)
        assert_eq!(move_vertical(text, pos, true), 1);
        assert_eq!(move_vertical(text, pos, false), 10);
    }

    #[test]
    fn home_and_end_stay_on_current_line() {
        let text = "abc\ndefg\nhi";
        let pos = 7; // line 2, column 2
        assert_eq!(line_start(text, pos), 4);
        assert_eq!(line_end(text, pos), 8);
    }

    #[test]
    fn char_boundaries_handle_multibyte() {
        let text = "aé日";
        assert_eq!(prev_char_boundary(text, text.len()), "aé".len());
        assert_eq!(next_char_boundary(text, 1), "aé".len());
    }
}

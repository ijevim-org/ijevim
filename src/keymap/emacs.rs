use crate::editor::Editor;
use crate::keymap::KeymapHandler;
use crate::types::PluginEvent;
use crate::types::{MessageLevel, Mode, Position};
use crossterm::event::{KeyCode, KeyModifiers};

pub struct EmacsKeymap;

impl EmacsKeymap {
    pub fn new() -> Self {
        Self
    }
}

/// Enter the shared command line with `prefix` already typed.
///
/// Emacs reaches the same line editor as Vim: `C-s` starts a forward search,
/// `C-r` a backward search, and `M-x` a command. `Editor::handle_key` routes
/// every key in `Mode::Command` to `handle_command` regardless of keymap.
fn start_command_line(editor: &mut Editor, prefix: char) {
    start_command_line_with_text(editor, &prefix.to_string());
}

/// Enter the shared command line with `text` already typed.
fn start_command_line_with_text(editor: &mut Editor, text: &str) {
    let prev_mode = editor.engine.state.mode;
    editor.engine.state.mode = Mode::Command;
    editor.engine.state.command_buffer.clear();
    editor.engine.state.command_cursor_pos = 0;
    editor.reset_command_line_nav();
    editor.engine.state.command_buffer.push_str(text);
    // The prefix is already typed, so later keys append after it.
    editor.engine.state.command_cursor_pos = editor.engine.state.command_buffer.len();
    editor.engine.plugin_manager.emit(PluginEvent::ModeChange {
        from: prev_mode,
        to: Mode::Command,
    });
    editor.needs_render = true;
}

/// Directory used as the minibuffer default for `C-x C-f` / `C-x d`.
fn default_directory(editor: &Editor) -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let dir = match editor.engine.state.file_path.as_ref() {
        Some(path) if path.is_dir() => path.clone(),
        Some(path) => path.parent().map(|p| p.to_path_buf()).unwrap_or(cwd),
        None => cwd,
    };
    let mut text = dir.to_string_lossy().to_string();
    if !text.ends_with(std::path::MAIN_SEPARATOR) {
        text.push(std::path::MAIN_SEPARATOR);
    }
    text
}

/// Start a numeric argument (`C-u`): each use multiplies by four.
fn start_universal_arg(editor: &mut Editor) {
    let state = &mut editor.engine.operator_state;
    state.emacs_numeric_arg = Some(match state.emacs_numeric_arg {
        None => 4,
        Some(n) => n.saturating_mul(4),
    });
    state.emacs_numeric_arg_digits = false;
}

/// Append a digit to the numeric argument (`C-u 3`, `M-3`).
fn push_arg_digit(editor: &mut Editor, digit: char) {
    let Some(digit) = digit.to_digit(10).map(|d| d as i32) else {
        return;
    };
    let state = &mut editor.engine.operator_state;
    if !state.emacs_numeric_arg_digits {
        // The first digit replaces the implicit 4 from `C-u`, keeping a sign
        // that `M--` may have set.
        let sign = if state.emacs_numeric_arg.unwrap_or(1) < 0 {
            -1
        } else {
            1
        };
        state.emacs_numeric_arg = Some(sign * digit);
        state.emacs_numeric_arg_digits = true;
    } else {
        let n = state.emacs_numeric_arg.unwrap_or(0);
        state.emacs_numeric_arg = Some(if n < 0 {
            n.saturating_mul(10).saturating_sub(digit)
        } else {
            n.saturating_mul(10).saturating_add(digit)
        });
    }
}

/// Start or negate the numeric argument with a minus sign.
fn push_arg_minus(editor: &mut Editor) {
    let state = &mut editor.engine.operator_state;
    state.emacs_numeric_arg = Some(-state.emacs_numeric_arg.unwrap_or(1).abs());
    state.emacs_numeric_arg_digits = false;
}

/// Consume the numeric argument, returning `(count, reverse)`.
fn take_numeric_arg(editor: &mut Editor) -> (usize, bool) {
    let state = &mut editor.engine.operator_state;
    let arg = state.emacs_numeric_arg.take().unwrap_or(1);
    state.emacs_numeric_arg_digits = false;
    (arg.unsigned_abs() as usize, arg < 0)
}

/// Human-readable form of a key for "undefined sequence" messages.
fn describe_key(key: KeyCode, modifiers: KeyModifiers) -> String {
    let prefix = if modifiers.contains(KeyModifiers::CONTROL) {
        "C-"
    } else if modifiers.contains(KeyModifiers::ALT) {
        "M-"
    } else {
        ""
    };
    match key {
        KeyCode::Char(c) => format!("{}{}", prefix, c),
        other => format!("{}{:?}", prefix, other),
    }
}

impl KeymapHandler for EmacsKeymap {
    fn handle_key<'e>(
        &self,
        editor: &'e mut Editor,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'e>> {
        Box::pin(async move {
            let has_ctrl = modifiers.contains(KeyModifiers::CONTROL);
            let has_alt = modifiers.contains(KeyModifiers::ALT);

            editor.emit_key_event(key, modifiers);

            // ── Pending prefix (C-x, ...) ──
            // A prefix always consumes its second key. An undefined sequence
            // must not fall through to the normal bindings: `C-x C-q` is not
            // `C-q`, it is simply unbound.
            if let Some(prefix) = editor.engine.operator_state.emacs_prefix.take() {
                // A numeric argument before a prefix sequence is ignored by
                // the prefix commands themselves; consume it so it cannot
                // leak into the key after the sequence.
                let _ = take_numeric_arg(editor);
                if prefix == 'g' {
                    let handled = match (has_ctrl, key) {
                        (false, KeyCode::Char('g')) => {
                            start_command_line_with_text(editor, "goto ");
                            true
                        }
                        _ => false,
                    };
                    if !handled {
                        editor.engine.state.push_message(
                            MessageLevel::Info,
                            format!("M-g {} is undefined", describe_key(key, modifiers)),
                        );
                        editor.needs_render = true;
                    }
                    return;
                }
                if prefix == 'x' {
                    let handled = match (has_ctrl, key) {
                        (true, KeyCode::Char('s')) => {
                            editor.save_file_async().await;
                            true
                        }
                        (true, KeyCode::Char('c')) => {
                            editor.handle_quit();
                            true
                        }
                        (true, KeyCode::Char('f')) => {
                            let default = default_directory(editor);
                            start_command_line_with_text(editor, &format!("e {}", default));
                            true
                        }
                        (true, KeyCode::Char('w')) => {
                            let default = editor
                                .engine
                                .state
                                .file_path
                                .as_ref()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_default();
                            start_command_line_with_text(editor, &format!("w {}", default));
                            true
                        }
                        (true, KeyCode::Char('h')) => {
                            // `C-x C-h` is unbound in Emacs; only `C-x h` is.
                            false
                        }
                        (false, KeyCode::Char('h')) => {
                            editor.mark_whole_buffer();
                            true
                        }
                        (true, KeyCode::Char('x')) => {
                            editor.exchange_point_and_mark();
                            true
                        }
                        (true, KeyCode::Char('u')) => {
                            editor.upcase_region();
                            true
                        }
                        (true, KeyCode::Char('l')) => {
                            editor.downcase_region();
                            true
                        }
                        // `C-x u` and `C-x C-/` are undo in Emacs, not redo.
                        (false, KeyCode::Char('u')) | (true, KeyCode::Char('/')) => {
                            editor.undo();
                            editor.needs_render = true;
                            true
                        }
                        (true, KeyCode::Char('g')) => true, // keyboard-quit
                        // ── Buffers / files ──
                        (false, KeyCode::Char('b')) => {
                            start_command_line_with_text(editor, "b ");
                            true
                        }
                        (false, KeyCode::Char('k')) => {
                            if editor.engine.buffer.is_dirty() {
                                editor.engine.state.push_message(
                                    MessageLevel::Info,
                                    "Buffer modified; save it first (C-x C-s)".to_string(),
                                );
                            } else if let Err(e) = editor.engine.buffer_delete() {
                                editor.engine.state.push_message(MessageLevel::Error, e);
                            }
                            editor.needs_render = true;
                            true
                        }
                        (false, KeyCode::Char('d')) => {
                            let default = default_directory(editor);
                            start_command_line_with_text(editor, &format!("e {}", default));
                            true
                        }
                        // ── Windows ──
                        (false, KeyCode::Char('2')) => {
                            if let Err(e) = editor.engine.split_horizontal() {
                                editor.engine.state.push_message(MessageLevel::Error, e);
                            }
                            editor.needs_render = true;
                            true
                        }
                        (false, KeyCode::Char('3')) => {
                            if let Err(e) = editor.engine.split_vertical() {
                                editor.engine.state.push_message(MessageLevel::Error, e);
                            }
                            editor.needs_render = true;
                            true
                        }
                        (false, KeyCode::Char('o')) => {
                            if let Err(e) = editor.engine.focus_next_window() {
                                editor.engine.state.push_message(MessageLevel::Error, e);
                            }
                            editor.needs_render = true;
                            true
                        }
                        (false, KeyCode::Char('O')) => {
                            if let Err(e) = editor.engine.focus_prev_window() {
                                editor.engine.state.push_message(MessageLevel::Error, e);
                            }
                            editor.needs_render = true;
                            true
                        }
                        (false, KeyCode::Char('0')) => {
                            if let Err(e) = editor.engine.close_focused_window() {
                                editor.engine.state.push_message(MessageLevel::Error, e);
                            }
                            editor.needs_render = true;
                            true
                        }
                        (false, KeyCode::Char('1')) => {
                            if let Err(e) = editor.engine.close_other_windows() {
                                editor.engine.state.push_message(MessageLevel::Error, e);
                            }
                            editor.needs_render = true;
                            true
                        }
                        _ => false,
                    };
                    if !handled {
                        editor.engine.state.push_message(
                            MessageLevel::Info,
                            format!("C-x {} is undefined", describe_key(key, modifiers)),
                        );
                        editor.needs_render = true;
                    }
                }
                return;
            }

            // ── Dired (directory listing) ──
            // Emacs dired keys override the normal text bindings here:
            // Enter visits the entry, `g` reverts, `^` goes up, `q` quits.
            if editor.engine.buffer.is_directory_listing() && !has_ctrl && !has_alt {
                let handled = match key {
                    KeyCode::Enter => {
                        editor.open_selected_entry();
                        true
                    }
                    KeyCode::Char('g') => {
                        editor.refresh_directory();
                        true
                    }
                    KeyCode::Char('^') => {
                        editor.go_up_directory();
                        true
                    }
                    KeyCode::Char('q') => {
                        if editor.engine.window_layout.windows.len() > 1 {
                            if let Err(e) = editor.engine.close_focused_window() {
                                editor.engine.state.push_message(MessageLevel::Error, e);
                            }
                        } else if let Err(e) = editor.engine.buffer_delete() {
                            editor.engine.state.push_message(MessageLevel::Error, e);
                        }
                        editor.needs_render = true;
                        true
                    }
                    _ => false,
                };
                if handled {
                    return;
                }
            }

            // ── Numeric argument (`C-u`, `M-3`, `M--`) ──
            if has_ctrl && !has_alt && key == KeyCode::Char('u') {
                start_universal_arg(editor);
                editor.needs_render = true;
                return;
            }
            if has_alt && !has_ctrl {
                match key {
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        push_arg_digit(editor, c);
                        editor.needs_render = true;
                        return;
                    }
                    KeyCode::Char('-') => {
                        push_arg_minus(editor);
                        editor.needs_render = true;
                        return;
                    }
                    _ => {}
                }
            }
            // Digits extend an argument already started by `C-u` / `M-3`.
            if !has_ctrl && !has_alt {
                if let KeyCode::Char(c) = key {
                    if c.is_ascii_digit()
                        && editor.engine.operator_state.emacs_numeric_arg.is_some()
                    {
                        push_arg_digit(editor, c);
                        editor.needs_render = true;
                        return;
                    }
                }
            }

            let (count, reverse) = take_numeric_arg(editor);

            // ── Redo extension ──
            // Emacs itself binds undo, not redo. Keep redo reachable without
            // stealing a standard sequence.
            if has_ctrl && has_alt && key == KeyCode::Char('/') {
                editor.redo();
                editor.needs_render = true;
                return;
            }

            // A kill command may extend the previous kill-ring entry; any
            // other command terminates the kill chain.
            let is_kill = has_ctrl && matches!(key, KeyCode::Char('k') | KeyCode::Char('w'));
            if !is_kill {
                editor.engine.operator_state.last_command_was_kill = false;
            }

            // `M-y` is only meaningful immediately after a yank.
            let is_yank =
                (has_ctrl && key == KeyCode::Char('y')) || (has_alt && key == KeyCode::Char('y'));
            if !is_yank {
                editor.engine.cursor_state.yank_start = None;
                editor.engine.cursor_state.yank_length = 0;
                editor.engine.cursor_state.yank_text.clear();
            }

            match (has_ctrl, key) {
                (true, KeyCode::Char('x')) => {
                    editor.engine.operator_state.emacs_prefix = Some('x');
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('f')) => {
                    if reverse {
                        editor.cursor_left(count);
                    } else {
                        editor.cursor_right(count);
                    }
                }
                (true, KeyCode::Char('b')) => {
                    if reverse {
                        editor.cursor_right(count);
                    } else {
                        editor.cursor_left(count);
                    }
                }
                (true, KeyCode::Char('n')) => {
                    if reverse {
                        editor.cursor_up(count);
                    } else {
                        editor.cursor_down(count);
                    }
                }
                (true, KeyCode::Char('p')) => {
                    if reverse {
                        editor.cursor_down(count);
                    } else {
                        editor.cursor_up(count);
                    }
                }
                (true, KeyCode::Char('a')) => {
                    editor.cursor_line_start();
                }
                (true, KeyCode::Char('e')) => {
                    editor.cursor_line_end_point();
                }
                (true, KeyCode::Char('d')) => {
                    for _ in 0..count {
                        if reverse {
                            editor.delete_char_backward();
                        } else {
                            editor.delete_char_forward();
                        }
                    }
                }
                (true, KeyCode::Char('k')) => {
                    for _ in 0..count {
                        editor.kill_line();
                    }
                }
                (true, KeyCode::Char('h')) => {
                    for _ in 0..count {
                        if reverse {
                            editor.delete_char_forward();
                        } else {
                            editor.delete_char_backward();
                        }
                    }
                }
                (true, KeyCode::Char(' ')) | (true, KeyCode::Char('@')) | (true, KeyCode::Null) => {
                    editor.set_mark();
                }
                (true, KeyCode::Char('w')) => {
                    if editor.has_active_region() {
                        editor.kill_region();
                    } else {
                        editor.engine.state.push_message(
                            MessageLevel::Info,
                            "The mark is not set now, so there is no region".to_string(),
                        );
                        editor.engine.operator_state.last_command_was_kill = false;
                        editor.needs_render = true;
                    }
                }
                (true, KeyCode::Char('y')) => {
                    for _ in 0..count {
                        editor.yank_from_kill_ring();
                    }
                }
                (true, KeyCode::Char('s')) => {
                    start_command_line(editor, '/');
                }
                (true, KeyCode::Char('r')) => {
                    start_command_line(editor, '?');
                }
                (true, KeyCode::Char('t')) => {
                    for _ in 0..count {
                        editor.transpose_chars();
                    }
                }
                (true, KeyCode::Char('v')) => {
                    // Emacs `C-v` scrolls forward (and moves point).
                    for _ in 0..count {
                        editor.page_down();
                    }
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('l')) => {
                    // Emacs `C-l` recenters point; it does not clear the screen.
                    editor.scroll_cursor_to_center();
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('o')) => {
                    // Emacs `C-o` opens a line at point.
                    for _ in 0..count {
                        editor.open_line_at_point();
                    }
                }
                (true, KeyCode::Char('g')) => {
                    editor.deactivate_region();
                    editor.abort();
                }
                (true, KeyCode::Char('/')) => {
                    editor.undo();
                }
                (true, KeyCode::Char('_')) => {
                    editor.undo();
                }
                (true, KeyCode::Char('?')) => {
                    editor.undo();
                }
                (false, KeyCode::Char(c)) if !c.is_control() && !has_alt => {
                    for _ in 0..count {
                        editor.insert_char(c);
                    }
                }
                (false, KeyCode::Null) => {
                    editor.set_mark();
                }
                (false, KeyCode::Backspace) if !has_alt => {
                    for _ in 0..count {
                        editor.delete_char_backward();
                    }
                }
                (false, KeyCode::Delete) if !has_alt => {
                    for _ in 0..count {
                        editor.delete_char_forward();
                    }
                }
                (false, KeyCode::Enter) => {
                    for _ in 0..count {
                        editor.insert_newline();
                    }
                }
                (false, KeyCode::Tab) => {
                    for _ in 0..count {
                        editor.insert_tab();
                    }
                }
                (false, KeyCode::Up) => {
                    if reverse {
                        editor.cursor_down(count);
                    } else {
                        editor.cursor_up(count);
                    }
                }
                (false, KeyCode::Down) => {
                    if reverse {
                        editor.cursor_up(count);
                    } else {
                        editor.cursor_down(count);
                    }
                }
                (false, KeyCode::Left) => {
                    if reverse {
                        editor.cursor_right(count);
                    } else {
                        editor.cursor_left(count);
                    }
                }
                (false, KeyCode::Right) => {
                    if reverse {
                        editor.cursor_left(count);
                    } else {
                        editor.cursor_right(count);
                    }
                }
                (false, KeyCode::Home) => {
                    editor.cursor_line_start();
                }
                (false, KeyCode::End) => {
                    editor.cursor_line_end_point();
                }
                (false, KeyCode::PageUp) => {
                    for _ in 0..count {
                        editor.scroll_up();
                    }
                }
                (false, KeyCode::PageDown) => {
                    for _ in 0..count {
                        editor.scroll_down();
                    }
                }
                _ => {
                    // Alt/Meta keybindings
                    if has_alt {
                        match key {
                            KeyCode::Char('f') => {
                                for _ in 0..count {
                                    if reverse {
                                        editor.move_word_backward();
                                    } else {
                                        editor.move_word_forward();
                                    }
                                }
                                editor.needs_render = true;
                            }
                            KeyCode::Char('<') => {
                                editor.engine.state.cursor = Position { line: 1, col: 0 };
                                editor.needs_render = true;
                            }
                            KeyCode::Char('>') => {
                                let last = editor.engine.buffer.content_line_count().max(1);
                                let col = editor.engine.buffer.line_content_len(last);
                                editor.engine.state.cursor = Position { line: last, col };
                                editor.needs_render = true;
                            }
                            KeyCode::Char('m') => {
                                let line = editor.engine.state.cursor.line;
                                editor.engine.state.cursor.col = editor.first_non_blank_col(line);
                                editor.needs_render = true;
                            }
                            KeyCode::Char('u') => {
                                for _ in 0..count {
                                    editor.upcase_word();
                                }
                            }
                            KeyCode::Char('l') => {
                                for _ in 0..count {
                                    editor.downcase_word();
                                }
                            }
                            KeyCode::Char('c') => {
                                for _ in 0..count {
                                    editor.capitalize_word();
                                }
                            }
                            KeyCode::Char('d') => {
                                for _ in 0..count {
                                    editor.kill_word();
                                }
                            }
                            KeyCode::Backspace => {
                                for _ in 0..count {
                                    editor.kill_word_backward();
                                }
                            }
                            KeyCode::Delete => {
                                for _ in 0..count {
                                    editor.kill_word();
                                }
                            }
                            KeyCode::Char('n') => {
                                for _ in 0..count {
                                    editor.search_next();
                                }
                                editor.needs_render = true;
                            }
                            KeyCode::Char('p') => {
                                for _ in 0..count {
                                    editor.search_prev();
                                }
                                editor.needs_render = true;
                            }
                            KeyCode::Char('g') => {
                                editor.engine.operator_state.emacs_prefix = Some('g');
                                editor.needs_render = true;
                            }
                            KeyCode::Char('b') => {
                                for _ in 0..count {
                                    if reverse {
                                        editor.move_word_forward();
                                    } else {
                                        editor.move_word_backward();
                                    }
                                }
                                editor.needs_render = true;
                            }
                            KeyCode::Char('v') => {
                                // Emacs `M-v` scrolls backward.
                                for _ in 0..count {
                                    editor.page_up();
                                }
                                editor.needs_render = true;
                            }
                            KeyCode::Char('w') if editor.has_active_region() => {
                                editor.copy_region();
                            }
                            KeyCode::Char('w') => {
                                editor.engine.state.push_message(
                                    MessageLevel::Info,
                                    "The mark is not set now, so there is no region".to_string(),
                                );
                                editor.needs_render = true;
                            }
                            KeyCode::Char('y') => {
                                // M-y: yank-pop (cycle kill-ring)
                                editor.yank_pop();
                                editor.needs_render = true;
                            }
                            KeyCode::Char('x') => {
                                start_command_line_with_text(editor, "");
                            }
                            _ => {}
                        }
                    }
                }
            }
        })
    }
}

impl Default for EmacsKeymap {
    fn default() -> Self {
        Self::new()
    }
}

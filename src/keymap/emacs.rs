use crate::editor::Editor;
use crate::keymap::KeymapHandler;
use crate::types::PluginEvent;
use crate::types::{MessageLevel, Mode};
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
    let prev_mode = editor.engine.state.mode;
    editor.engine.state.mode = Mode::Command;
    editor.engine.state.command_buffer.clear();
    editor.engine.state.command_cursor_pos = 0;
    editor.reset_command_line_nav();
    editor.engine.state.command_buffer.push(prefix);
    // The prefix is already typed, so later keys append after it.
    editor.engine.state.command_cursor_pos = editor.engine.state.command_buffer.len();
    editor.engine.plugin_manager.emit(PluginEvent::ModeChange {
        from: prev_mode,
        to: Mode::Command,
    });
    editor.needs_render = true;
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
            // must not fall through to the normal bindings: `C-x C-f` is not
            // `C-f`, it is simply unbound.
            if let Some(prefix) = editor.engine.operator_state.emacs_prefix.take() {
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
                        // `C-x u` and `C-x C-/` are undo in Emacs, not redo.
                        (false, KeyCode::Char('u')) | (true, KeyCode::Char('/')) => {
                            editor.undo();
                            editor.needs_render = true;
                            true
                        }
                        (true, KeyCode::Char('g')) => true, // keyboard-quit
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
                    editor.cursor_right(1);
                }
                (true, KeyCode::Char('b')) => {
                    editor.cursor_left(1);
                }
                (true, KeyCode::Char('n')) => {
                    editor.cursor_down(1);
                }
                (true, KeyCode::Char('p')) => {
                    editor.cursor_up(1);
                }
                (true, KeyCode::Char('a')) => {
                    editor.cursor_line_start();
                }
                (true, KeyCode::Char('e')) => {
                    editor.cursor_line_end_point();
                }
                (true, KeyCode::Char('d')) => {
                    editor.delete_char_forward();
                }
                (true, KeyCode::Char('k')) => {
                    editor.kill_line();
                }
                (true, KeyCode::Char('h')) => {
                    editor.delete_char_backward();
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
                    editor.yank_from_kill_ring();
                }
                (true, KeyCode::Char('s')) => {
                    start_command_line(editor, '/');
                }
                (true, KeyCode::Char('r')) => {
                    start_command_line(editor, '?');
                }
                (true, KeyCode::Char('t')) => {
                    editor.transpose_chars();
                }
                (true, KeyCode::Char('v')) => {
                    // Emacs `C-v` scrolls forward (and moves point).
                    editor.page_down();
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('l')) => {
                    // Emacs `C-l` recenters point; it does not clear the screen.
                    editor.scroll_cursor_to_center();
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('o')) => {
                    // Emacs `C-o` opens a line at point.
                    editor.open_line_at_point();
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
                    editor.insert_char(c);
                }
                (false, KeyCode::Null) => {
                    editor.set_mark();
                }
                (false, KeyCode::Backspace) => {
                    editor.delete_char_backward();
                }
                (false, KeyCode::Delete) => {
                    editor.delete_char_forward();
                }
                (false, KeyCode::Enter) => {
                    editor.insert_newline();
                }
                (false, KeyCode::Tab) => {
                    editor.insert_tab();
                }
                (false, KeyCode::Up) => {
                    editor.cursor_up(1);
                }
                (false, KeyCode::Down) => {
                    editor.cursor_down(1);
                }
                (false, KeyCode::Left) => {
                    editor.cursor_left(1);
                }
                (false, KeyCode::Right) => {
                    editor.cursor_right(1);
                }
                (false, KeyCode::Home) => {
                    editor.cursor_line_start();
                }
                (false, KeyCode::End) => {
                    editor.cursor_line_end_point();
                }
                (false, KeyCode::PageUp) => {
                    editor.scroll_up();
                }
                (false, KeyCode::PageDown) => {
                    editor.scroll_down();
                }
                _ => {
                    // Alt/Meta keybindings
                    if has_alt {
                        match key {
                            KeyCode::Char('f') => {
                                editor.move_word_forward();
                                editor.needs_render = true;
                            }
                            KeyCode::Char('b') => {
                                editor.move_word_backward();
                                editor.needs_render = true;
                            }
                            KeyCode::Char('v') => {
                                // Emacs `M-v` scrolls backward.
                                editor.page_up();
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
                                start_command_line(editor, ':');
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

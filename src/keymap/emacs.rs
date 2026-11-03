use crate::editor::Editor;
use crate::keymap::KeymapHandler;
use crate::types::Mode;
use crate::types::PluginEvent;
use crossterm::event::{KeyCode, KeyModifiers};

pub struct EmacsKeymap;

impl EmacsKeymap {
    pub fn new() -> Self {
        Self
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

            // Handle C-x prefix state (stored in operator_state)
            if editor.engine.operator_state.emacs_cx_prefix {
                editor.engine.operator_state.emacs_cx_prefix = false;

                match (has_ctrl, key) {
                    (true, KeyCode::Char('s')) => {
                        editor.save_file_async().await;
                        return;
                    }
                    (true, KeyCode::Char('c')) => {
                        editor.handle_quit();
                        return;
                    }
                    (true, KeyCode::Char('h')) => {
                        editor.cursor_line_start();
                        return;
                    }
                    (true, KeyCode::Char('d')) => {
                        editor.cursor_line_end_point();
                        return;
                    }
                    // C-x u / C-x C-/: redo
                    (false, KeyCode::Char('u')) => {
                        editor.redo();
                        editor.needs_render = true;
                        return;
                    }
                    (true, KeyCode::Char('/')) => {
                        editor.redo();
                        editor.needs_render = true;
                        return;
                    }
                    _ => {}
                }
            }

            match (has_ctrl, key) {
                (true, KeyCode::Char('x')) => {
                    editor.engine.operator_state.emacs_cx_prefix = true;
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
                (true, KeyCode::Char(' ')) | (true, KeyCode::Char('@')) => {
                    editor.set_mark();
                }
                (true, KeyCode::Char('w')) => {
                    if editor.has_active_region() {
                        editor.kill_region();
                    } else {
                        editor.kill_word();
                    }
                }
                (true, KeyCode::Char('y')) => {
                    editor.yank_from_kill_ring();
                }
                (true, KeyCode::Char('o')) => {
                    // C-o is a legacy direct save trigger.
                    // Preferred Emacs-style save is C-x C-s, but C-o remains
                    // for compatibility with existing ijevim behavior.
                    editor.save_file_async().await;
                }
                (true, KeyCode::Char('s')) => {
                    let prev_mode = editor.engine.state.mode;
                    editor.engine.state.mode = Mode::Command;
                    editor.engine.state.command_buffer.clear();
                    editor.engine.state.command_cursor_pos = 0;
                    editor.engine.state.command_buffer.push('/');
                    editor.engine.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Command,
                    });
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('r')) => {
                    // C-r: backward search (enter command mode with ?)
                    let prev_mode = editor.engine.state.mode;
                    editor.engine.state.mode = Mode::Command;
                    editor.engine.state.command_buffer.clear();
                    editor.engine.state.command_cursor_pos = 0;
                    editor.engine.state.command_buffer.push('?');
                    editor.engine.plugin_manager.emit(PluginEvent::ModeChange {
                        from: prev_mode,
                        to: Mode::Command,
                    });
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('t')) => {
                    editor.transpose_chars();
                }
                (true, KeyCode::Char('v')) => {
                    editor.scroll_up_one();
                    editor.needs_render = true;
                }
                (true, KeyCode::Char('l')) => {
                    editor.clear_screen();
                    editor.scroll_cursor_to_center();
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
                                editor.scroll_down();
                                editor.needs_render = true;
                            }
                            KeyCode::Char('w') if editor.has_active_region() => {
                                editor.copy_region();
                            }
                            KeyCode::Char('y') => {
                                // M-y: yank-pop (cycle kill-ring)
                                editor.yank_pop();
                                editor.needs_render = true;
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

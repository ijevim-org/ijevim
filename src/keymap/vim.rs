use crate::editor::Editor;
use crate::keymap::KeymapHandler;
use crate::types::Mode;
use crossterm::event::{KeyCode, KeyModifiers};

pub struct VimKeymap;

impl VimKeymap {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VimKeymap {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapHandler for VimKeymap {
    fn handle_key<'e>(
        &self,
        editor: &'e mut Editor,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'e>> {
        Box::pin(async move {
            editor.emit_key_event(key, modifiers);

            // Ctrl-o in Insert mode: run one Normal-mode command, then return
            // to Insert. Keep routing to Normal until any pending operator
            // (count, operator, `g`, Ctrl-w) completes.
            if editor.one_shot_normal {
                editor.one_shot_normal = false;
                let was_insert = editor.engine.state.mode == Mode::Insert;
                editor.handle_normal(key).await;
                let pending = !matches!(
                    editor.engine.operator_state.pending,
                    crate::types::PendingOperator::Idle
                ) || editor.engine.operator_state.count.is_some()
                    || editor.engine.operator_state.pending_g
                    || editor.engine.operator_state.ctrl_w_prefix;
                if pending {
                    editor.one_shot_normal = true;
                } else if was_insert && editor.engine.state.mode == Mode::Normal {
                    editor.transition_mode(Mode::Insert);
                }
                editor.needs_render = true;
                return;
            }

            if modifiers.contains(KeyModifiers::CONTROL) {
                match editor.engine.state.mode {
                    Mode::Normal => match key {
                        KeyCode::Char('w') => {
                            // Ctrl-w prefix: enter Ctrl-w prefix mode
                            editor.engine.operator_state.ctrl_w_prefix = true;
                            return;
                        }
                        KeyCode::Char('d') => {
                            editor.scroll_by(editor.terminal.rows() as usize / 2, true);
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('u') => {
                            editor.scroll_by(editor.terminal.rows() as usize / 2, false);
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('r') => {
                            editor.redo();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('y') => {
                            editor.scroll_up_one();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('e') => {
                            editor.scroll_down_one();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('g') => {
                            editor.show_file_info();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('b') => {
                            editor.page_up();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('f') => {
                            editor.page_down();
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('a') => {
                            // Ctrl-a: increment the number under the cursor.
                            let count = editor.take_count();
                            editor.increment_number(1, count);
                            return;
                        }
                        KeyCode::Char('x') => {
                            // Ctrl-x: decrement the number under the cursor.
                            let count = editor.take_count();
                            editor.increment_number(-1, count);
                            return;
                        }
                        KeyCode::Char('v') => {
                            // Ctrl-v: enter visual block mode
                            let prev_mode = editor.engine.state.mode;
                            editor.engine.state.mode = Mode::Visual;
                            editor.engine.state.visual_start = Some(editor.engine.state.cursor);
                            editor.engine.state.visual_type = Some(crate::types::VisualType::Block);
                            editor.engine.plugin_manager.emit(
                                crate::types::PluginEvent::ModeChange {
                                    from: prev_mode,
                                    to: Mode::Visual,
                                },
                            );
                            editor.needs_render = true;
                            return;
                        }
                        _ => {}
                    },
                    Mode::Insert | Mode::Replace => match key {
                        KeyCode::Char('t') => {
                            // Ctrl-t: indent the current line.
                            editor.insert_indent(true);
                            return;
                        }
                        KeyCode::Char('d') => {
                            // Ctrl-d: dedent the current line.
                            editor.insert_indent(false);
                            return;
                        }
                        KeyCode::Char('o') => {
                            // Ctrl-o: run one Normal-mode command.
                            editor.one_shot_normal = true;
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('v') => {
                            // Ctrl-v: insert the next character literally.
                            editor.engine.insert_state.waiting_literal = true;
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('w') => {
                            // Ctrl-w: delete word backward in Insert mode
                            editor.insert_delete_word_backward();
                            return;
                        }
                        KeyCode::Char('r') => {
                            // Insert: insert register (Ctrl-r)
                            editor.engine.insert_state.waiting_register = true;
                            editor.needs_render = true;
                            return;
                        }
                        KeyCode::Char('u') => {
                            editor.insert_delete_to_bol();
                            return;
                        }
                        _ => {}
                    },
                    Mode::Visual => {
                        if let KeyCode::Char('v') = key {
                            editor.engine.state.visual_type = Some(crate::types::VisualType::Block);
                            editor.needs_render = true;
                            return;
                        }
                    }
                    Mode::Command => {}
                }
            }

            // Capture mode BEFORE dispatching to prevent mode-transition keys
            // (like 'i', 'a', 'v', ':') from being re-dispatched to the new mode.
            let mode = editor.engine.state.mode;
            match mode {
                Mode::Normal => editor.handle_normal(key).await,
                Mode::Insert => editor.handle_insert(key).await,
                Mode::Command => editor.handle_command(key, modifiers).await,
                Mode::Visual => editor.handle_visual(key).await,
                Mode::Replace => editor.handle_replace(key).await,
            }
        })
    }
}

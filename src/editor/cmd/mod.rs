use crate::editor::Editor;
use crate::types::{ConfirmAction, MessageLevel, Mode, SearchDirection};
use crossterm::event::{KeyCode, KeyModifiers};

mod ex;
mod files;
mod git;
mod set;
mod substitute;

impl Editor {
    pub(crate) async fn handle_command(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        if !matches!(key, KeyCode::Tab) {
            self.completion_session = None;
        }

        match key {
            KeyCode::Enter => {
                let raw_line = self.engine.state.command_buffer.clone();
                self.push_command_history(&raw_line);
                let committed_preview = if Self::is_search_line(&raw_line) {
                    self.commit_search_preview()
                } else {
                    false
                };
                let cmd = raw_line.trim().to_string();
                let cmd = cmd.strip_prefix(':').unwrap_or(&cmd);
                self.engine.state.command_buffer.clear();
                self.engine.state.command_cursor_pos = 0;
                self.engine.state.command_hint = None;

                match cmd {
                    "q" => {
                        self.handle_quit();
                    }
                    "q!" => {
                        self.running = false;
                    }
                    "w" | "w!" => {
                        self.save_and_emit().await;
                    }
                    "wq" | "wq!" | "wqa" | "wa" => {
                        self.save_and_emit().await;
                        self.running = false;
                    }
                    "qa" => {
                        self.running = false;
                    }
                    "e" => {
                        self.reload_file().await;
                    }
                    "e!" => {
                        self.reload_file_discard().await;
                    }
                    cmd if cmd.starts_with("keymap ") => {
                        let keymap_name = cmd.trim_start_matches("keymap ").trim();
                        self.switch_keymap(keymap_name);
                    }
                    cmd if cmd == "b" || cmd.starts_with("b ") => {
                        let name = cmd.strip_prefix('b').unwrap_or(cmd).trim();
                        if let Err(e) = self.engine.buffer_switch_by_name(name) {
                            self.engine.state.push_message(MessageLevel::Error, e);
                        }
                        self.needs_render = true;
                    }
                    "bn" | "bnext" => {
                        if let Err(e) = self.engine.buffer_next() {
                            self.engine.state.push_message(MessageLevel::Error, e);
                        }
                        self.needs_render = true;
                    }
                    "bp" | "bprevious" | "bprev" => {
                        if let Err(e) = self.engine.buffer_prev() {
                            self.engine.state.push_message(MessageLevel::Error, e);
                        }
                        self.needs_render = true;
                    }
                    "bd" | "bdelete" => {
                        if let Err(e) = self.engine.buffer_delete() {
                            self.engine.state.push_message(MessageLevel::Error, e);
                        }
                        self.needs_render = true;
                    }
                    "Ex" | "Explore" => {
                        self.engine.state.show_dashboard = false;
                        match std::env::current_dir() {
                            Ok(cwd) => {
                                let path = cwd.to_string_lossy().to_string();
                                if let Err(e) = self.engine.buffer_open_dir(&path) {
                                    self.engine.state.push_message(MessageLevel::Error, e);
                                }
                            }
                            Err(e) => {
                                self.engine.state.push_message(
                                    MessageLevel::Error,
                                    format!("Cannot get current directory: {}", e),
                                );
                            }
                        }
                        self.needs_render = true;
                    }
                    "ls" | "buffers" => {
                        let list = self.engine.buffer_list();
                        self.engine.state.push_message(MessageLevel::Info, list);
                    }
                    "sp" | "split" => {
                        if let Err(e) = self.engine.split_horizontal() {
                            self.engine.state.push_message(MessageLevel::Error, e);
                        }
                        self.needs_render = true;
                    }
                    "vs" | "vsplit" => {
                        if let Err(e) = self.engine.split_vertical() {
                            self.engine.state.push_message(MessageLevel::Error, e);
                        }
                        self.needs_render = true;
                    }
                    cmd if cmd.starts_with("goto ") => {
                        let arg = cmd.trim_start_matches("goto ").trim();
                        match arg.parse::<usize>() {
                            Ok(n) => {
                                let last = self.engine.buffer.content_line_count().max(1);
                                self.engine.state.cursor = crate::types::Position {
                                    line: n.clamp(1, last),
                                    col: 0,
                                };
                                self.needs_render = true;
                            }
                            Err(_) => self.engine.state.push_message(
                                MessageLevel::Warning,
                                format!("Invalid line number: {}", arg),
                            ),
                        }
                    }
                    "messages" => {
                        if self.engine.state.message_history.is_empty() {
                            self.engine
                                .state
                                .push_message(MessageLevel::Info, "No messages".to_string());
                        } else {
                            // Read the history without re-inserting it, otherwise
                            // every `:messages` duplicates the whole log.
                            let text = self
                                .engine
                                .state
                                .message_history
                                .iter()
                                .map(|msg| {
                                    let prefix = match msg.level {
                                        MessageLevel::Error => "[ERROR]",
                                        MessageLevel::Warning => "[WARN]",
                                        MessageLevel::Info => "[INFO]",
                                    };
                                    format!("{} {}", prefix, msg.text)
                                })
                                .collect::<Vec<_>>()
                                .join(" | ");
                            self.engine.state.last_message = Some(text);
                        }
                    }
                    cmd if cmd.starts_with("e ") || cmd == "e." => {
                        self.engine.state.show_dashboard = false;
                        let path = if cmd == "e." {
                            "."
                        } else {
                            cmd.trim_start_matches("e ").trim()
                        };
                        let p = std::path::Path::new(path);
                        if p.is_dir() {
                            if let Err(e) = self.engine.buffer_open_dir(path) {
                                self.engine.state.push_message(MessageLevel::Error, e);
                            }
                        } else if let Err(e) = self.engine.buffer_open(path) {
                            self.engine.state.push_message(MessageLevel::Error, e);
                        }
                        self.needs_render = true;
                    }
                    _ => {
                        if let Some((range, rest)) = self.split_ex_range(cmd) {
                            self.handle_ex_range(range, rest);
                        } else if let Some(query) = cmd.strip_prefix('/') {
                            if !committed_preview {
                                let direction = SearchDirection::Forward;
                                self.do_search(query, direction);
                            }
                        } else if let Some(query) = cmd.strip_prefix('?') {
                            if !committed_preview {
                                let direction = SearchDirection::Backward;
                                self.do_search(query, direction);
                            }
                        } else if cmd.starts_with("set ") {
                            self.handle_set_command(cmd);
                        } else if cmd == "nohlsearch" || cmd == "noh" {
                            self.engine.state.hlsearch = false;
                        } else if cmd == "review" || cmd.starts_with("review ") {
                            let args_str = cmd.trim_start_matches("review").trim();
                            let args = crate::review::ReviewArgs::parse(args_str);
                            self.enter_review_mode(args);
                            self.needs_render = true;
                        } else if cmd.starts_with("w ") || cmd.starts_with("w! ") {
                            self.handle_write_path(cmd).await;
                        } else if cmd.starts_with("wq ") || cmd.starts_with("wq! ") {
                            self.handle_write_quit_path(cmd).await;
                        } else if cmd == "branch" {
                            self.handle_git_command(&["branch"]);
                        } else if cmd.starts_with("branch ") {
                            let args = cmd.trim_start_matches("branch ").trim();
                            if args.starts_with("-d") || args.starts_with("-D") {
                                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                                let flag = parts[0];
                                let name = parts.get(1).unwrap_or(&"");
                                self.handle_git_command(&["branch", flag, name]);
                            } else if args == "-a" || args == "-r" {
                                self.handle_git_command(&["branch", args]);
                            } else {
                                let branch_name = args;
                                self.handle_git_command(&["checkout", branch_name]);
                                self.reload_file_after_checkout();
                            }
                        } else if cmd.starts_with("git ") {
                            let git_args_str = cmd.trim_start_matches("git ").trim();
                            let git_args: Vec<&str> = git_args_str.split_whitespace().collect();
                            if !git_args.is_empty() {
                                self.handle_git_command(&git_args);
                                // Reload file if switching branches
                                if git_args.first() == Some(&"checkout") {
                                    self.reload_file_after_checkout();
                                }
                            }
                        } else if Self::is_substitute_command(cmd) {
                            self.handle_substitute(cmd);
                        } else {
                            // Split cmd into command name and args
                            let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
                            let cmd_name = parts[0];
                            let args: Vec<String> = if parts.len() > 1 {
                                parts[1].split(' ').map(|s| s.to_string()).collect()
                            } else {
                                vec![]
                            };
                            if !self.engine.plugin_manager.execute_command(cmd_name, args) {
                                self.engine.state.push_message(
                                    MessageLevel::Warning,
                                    format!("Unknown command: {}", cmd),
                                );
                            }
                        }
                    }
                }

                if !self.engine.state.has_confirmation() {
                    self.transition_mode(Mode::Normal);
                    self.needs_render = true;
                }
            }
            KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('g')
                if key == KeyCode::Esc || modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.cancel_search_preview();
                self.engine.state.command_buffer.clear();
                self.engine.state.command_cursor_pos = 0;
                self.engine.state.command_hint = None;
                self.completion_session = None;
                self.transition_mode(Mode::Normal);
                self.needs_render = true;
            }
            KeyCode::Tab => {
                self.complete_command_line();
            }
            KeyCode::Up => {
                self.history_prev();
                self.needs_render = true;
            }
            KeyCode::Down => {
                self.history_next();
                self.needs_render = true;
            }
            KeyCode::Backspace => {
                let pos = self.engine.state.command_cursor_pos;
                if pos > 0 {
                    // Find the previous char boundary.
                    let prev = self.engine.state.command_buffer[..pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.engine.state.command_buffer.drain(prev..pos);
                    self.engine.state.command_cursor_pos = prev;
                }
                self.update_search_preview();
                self.needs_render = true;
            }
            KeyCode::Delete => {
                let pos = self.engine.state.command_cursor_pos;
                if pos < self.engine.state.command_buffer.len() {
                    let next = self.engine.state.command_buffer[pos..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| pos + i)
                        .unwrap_or(self.engine.state.command_buffer.len());
                    self.engine.state.command_buffer.drain(pos..next);
                }
                self.update_search_preview();
                self.needs_render = true;
            }
            KeyCode::Home => {
                self.engine.state.command_cursor_pos = 0;
                self.needs_render = true;
            }
            KeyCode::End => {
                self.engine.state.command_cursor_pos = self.engine.state.command_buffer.len();
                self.needs_render = true;
            }
            // Ctrl-p / Ctrl-n: history navigation
            KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.history_prev();
                self.needs_render = true;
            }
            KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.history_next();
                self.needs_render = true;
            }
            // Ctrl-b / Ctrl-f: move one character
            KeyCode::Char('b') if modifiers.contains(KeyModifiers::CONTROL) => {
                let pos = self.engine.state.command_cursor_pos;
                if pos > 0 {
                    let prev = self.engine.state.command_buffer[..pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.engine.state.command_cursor_pos = prev;
                }
                self.needs_render = true;
            }
            KeyCode::Char('f') if modifiers.contains(KeyModifiers::CONTROL) => {
                let pos = self.engine.state.command_cursor_pos;
                if pos < self.engine.state.command_buffer.len() {
                    let next = self.engine.state.command_buffer[pos..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| pos + i)
                        .unwrap_or(self.engine.state.command_buffer.len());
                    self.engine.state.command_cursor_pos = next;
                }
                self.needs_render = true;
            }
            // Ctrl-k: delete from cursor to end
            KeyCode::Char('k') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.engine
                    .state
                    .command_buffer
                    .truncate(self.engine.state.command_cursor_pos);
                self.update_search_preview();
                self.needs_render = true;
            }
            // Ctrl-u: delete from start to cursor
            KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                let pos = self.engine.state.command_cursor_pos;
                self.engine.state.command_buffer.drain(..pos);
                self.engine.state.command_cursor_pos = 0;
                self.update_search_preview();
                self.needs_render = true;
            }
            // Ctrl-d: delete the character under the cursor
            KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
                let pos = self.engine.state.command_cursor_pos;
                if pos < self.engine.state.command_buffer.len() {
                    let next = self.engine.state.command_buffer[pos..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| pos + i)
                        .unwrap_or(self.engine.state.command_buffer.len());
                    self.engine.state.command_buffer.drain(pos..next);
                }
                self.update_search_preview();
                self.needs_render = true;
            }
            // Ctrl-a: move cursor to beginning of command line
            KeyCode::Char('a') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.engine.state.command_cursor_pos = 0;
                self.needs_render = true;
            }
            // Ctrl-e: move cursor to end of command line
            KeyCode::Char('e') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.engine.state.command_cursor_pos = self.engine.state.command_buffer.len();
                self.needs_render = true;
            }
            // Ctrl-w: delete word backward
            KeyCode::Char('w') if modifiers.contains(KeyModifiers::CONTROL) => {
                let pos = self.engine.state.command_cursor_pos;
                if pos > 0 {
                    let buf = &self.engine.state.command_buffer[..pos];
                    // Skip trailing whitespace, then skip non-whitespace.
                    let without_trailing = buf.trim_end_matches(' ');
                    let without_word = without_trailing
                        .rsplit_once(char::is_whitespace)
                        .map(|(before, _)| before.len())
                        .unwrap_or(0);
                    self.engine.state.command_buffer.drain(without_word..pos);
                    self.engine.state.command_cursor_pos = without_word;
                }
                self.update_search_preview();
                self.needs_render = true;
            }
            KeyCode::Char(c) => {
                let pos = self.engine.state.command_cursor_pos;
                self.engine.state.command_buffer.insert(pos, c);
                self.engine.state.command_cursor_pos = pos + c.len_utf8();
                self.update_search_preview();
                self.needs_render = true;
            }
            // Left arrow: move cursor left
            KeyCode::Left => {
                let pos = self.engine.state.command_cursor_pos;
                if pos > 0 {
                    let prev = self.engine.state.command_buffer[..pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.engine.state.command_cursor_pos = prev;
                }
                self.needs_render = true;
            }
            // Right arrow: move cursor right
            KeyCode::Right => {
                let pos = self.engine.state.command_cursor_pos;
                if pos < self.engine.state.command_buffer.len() {
                    let next = self.engine.state.command_buffer[pos..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| pos + i)
                        .unwrap_or(self.engine.state.command_buffer.len());
                    self.engine.state.command_cursor_pos = next;
                }
                self.needs_render = true;
            }
            _ => {}
        }
    }

    pub fn handle_quit(&mut self) {
        if self.engine.buffer.is_dirty() {
            self.engine.state.set_confirmation(
                "No write since last change. Quit anyway? (y/n Enter/Esc: yes, n: no)".to_string(),
                ConfirmAction::Quit,
            );
            self.needs_render = true;
        } else {
            self.running = false;
            self.engine.state.mode = Mode::Normal;
        }
    }

    pub(crate) async fn handle_confirmation(&mut self, key: KeyCode) {
        let should_quit = match key {
            KeyCode::Char('y') | KeyCode::Enter => true,
            KeyCode::Char('n') | KeyCode::Esc => false,
            _ => {
                self.engine.state.clear_confirmation();
                self.needs_render = true;
                return;
            }
        };

        let action = self
            .engine
            .state
            .confirmation_prompt
            .as_ref()
            .unwrap()
            .action
            .clone();
        self.engine.state.clear_confirmation();
        self.needs_render = true;

        match action {
            ConfirmAction::Quit => {
                if should_quit {
                    self.running = false;
                    self.engine.state.mode = Mode::Normal;
                }
            }
            ConfirmAction::QuitDiscard => {
                self.running = false;
                self.engine.state.mode = Mode::Normal;
            }
            ConfirmAction::WriteQuitAll => {
                if !should_quit {
                    return;
                }
                if let Err(e) = self.engine.buffer.save_file().await {
                    self.engine
                        .state
                        .push_message(MessageLevel::Error, format!("Save failed: {}", e));
                } else {
                    self.engine
                        .plugin_manager
                        .emit(crate::types::PluginEvent::BufferSave {
                            file_path: self.engine.state.file_path.clone(),
                        });
                    self.running = false;
                    self.engine.state.mode = Mode::Normal;
                }
            }
            ConfirmAction::ReloadDiscard => {
                if should_quit {
                    self.reload_file().await;
                }
            }
        }
    }
}

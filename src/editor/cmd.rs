use crate::editor::Editor;
use crate::types::{ConfirmAction, MessageLevel, Mode, SearchDirection};
use crossterm::event::{KeyCode, KeyModifiers};

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
            KeyCode::Esc | KeyCode::Char('c')
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

    fn handle_set_command(&mut self, cmd: &str) {
        let args = cmd.trim_start_matches("set ").trim();

        match args {
            "number" => {
                self.engine.state.show_line_numbers = true;
            }
            "nonumber" | "nonumber!" => {
                self.engine.state.show_line_numbers = false;
            }
            "number!" => {
                self.engine.state.show_line_numbers = !self.engine.state.show_line_numbers;
            }
            "relativenumber" => {
                self.engine.state.show_relative_numbers = true;
            }
            "norelativenumber" | "norelativenumber!" => {
                self.engine.state.show_relative_numbers = false;
            }
            "relativenumber!" => {
                self.engine.state.show_relative_numbers = !self.engine.state.show_relative_numbers;
            }
            "hlsearch" => {
                self.engine.state.hlsearch = true;
            }
            "nohlsearch" | "nohlsearch!" => {
                self.engine.state.hlsearch = false;
            }
            "hlsearch!" => {
                self.engine.state.hlsearch = !self.engine.state.hlsearch;
            }
            "expandtab" | "et" => {
                self.engine.state.expandtab = true;
            }
            "noexpandtab" | "noet" => {
                self.engine.state.expandtab = false;
            }
            "expandtab!" | "et!" => {
                self.engine.state.expandtab = !self.engine.state.expandtab;
            }
            args if args.starts_with("tabstop=") || args.starts_with("ts=") => {
                let val = args.split_once('=').map(|x| x.1).unwrap_or("8");
                if let Ok(n) = val.parse::<usize>() {
                    self.engine.state.tabstop = n;
                } else {
                    self.engine
                        .state
                        .push_message(MessageLevel::Warning, format!("Invalid value: {}", val));
                }
            }
            args if args.starts_with("shiftwidth=") || args.starts_with("sw=") => {
                let val = args.split_once('=').map(|x| x.1).unwrap_or("4");
                if let Ok(n) = val.parse::<usize>() {
                    self.engine.state.shiftwidth = n;
                } else {
                    self.engine
                        .state
                        .push_message(MessageLevel::Warning, format!("Invalid value: {}", val));
                }
            }
            args if args.starts_with("scrolloff=") || args.starts_with("so=") => {
                let val = args.split_once('=').map(|x| x.1).unwrap_or("0");
                if let Ok(n) = val.parse::<usize>() {
                    self.engine.state.scrolloff = n;
                } else {
                    self.engine
                        .state
                        .push_message(MessageLevel::Warning, format!("Invalid value: {}", val));
                }
            }
            _ => {
                self.engine.state.push_message(
                    MessageLevel::Warning,
                    format!("Unknown set option: {}", args),
                );
            }
        }
    }

    /// True for `:s/...`, `:%s/...`, `:3s/...`, `:1,5s/...` and `:$s/...`.
    fn is_substitute_command(cmd: &str) -> bool {
        let Some(idx) = cmd.find("s/") else {
            return false;
        };
        let spec = &cmd[..idx];
        match spec.chars().next() {
            // No range at all: `:s/...`.
            None => true,
            // A range-looking prefix is routed here so a malformed one is
            // reported as an invalid range rather than an unknown command.
            Some(c) => c.is_ascii_digit() || c == '%' || c == '.' || c == '$',
        }
    }

    /// Handle `:[range]s/pattern/replacement/[g]`.
    ///
    /// The replacement follows Vim's syntax (`&`, `\1`..`\9`) and is
    /// translated to the `regex` crate's (`$0`, `$1`..`$9`).
    fn handle_substitute(&mut self, cmd: &str) {
        let Some(idx) = cmd.find("s/") else {
            self.engine.state.push_message(
                MessageLevel::Warning,
                "Invalid substitute command".to_string(),
            );
            return;
        };
        let range_spec = &cmd[..idx];
        let rest = &cmd[idx + 2..];

        // Split into pattern / replacement / flags.
        let mut parts = rest.splitn(3, '/');
        let pattern_str = parts.next().unwrap_or("");
        let Some(replacement) = parts.next() else {
            self.engine.state.push_message(
                MessageLevel::Warning,
                "Invalid substitute syntax".to_string(),
            );
            return;
        };
        let flags = parts.next().unwrap_or("");

        if pattern_str.is_empty() {
            self.engine.state.push_message(
                MessageLevel::Warning,
                "Empty substitute pattern".to_string(),
            );
            return;
        }

        let re = match regex::Regex::new(pattern_str) {
            Ok(re) => re,
            Err(e) => {
                self.engine
                    .state
                    .push_message(MessageLevel::Warning, format!("Regex error: {}", e));
                return;
            }
        };
        let replacement = translate_replacement(replacement);

        let global = flags.contains('g');
        let line_count = self.engine.buffer.line_count();

        let Some((start_line, end_line)) =
            parse_line_range(range_spec, self.engine.state.cursor.line, line_count)
        else {
            self.engine.state.push_message(
                MessageLevel::Warning,
                format!("Invalid range: {}", range_spec),
            );
            return;
        };

        let mut replacements = 0;
        let mut edits: Vec<crate::undo::Edit> = Vec::new();

        // Process lines bottom-up so earlier line indices stay valid, and treat
        // the trailing newline as part of the line. Otherwise substituting N
        // lines joins them all into one.
        for line_idx in (start_line..=end_line).rev() {
            let old_text = self.engine.buffer.get_line(line_idx);
            let new_text = if global {
                re.replace_all(&old_text, replacement.as_str()).to_string()
            } else {
                re.replace(&old_text, replacement.as_str()).to_string()
            };
            if new_text == old_text {
                continue;
            }
            replacements += 1;
            let mod_count = self.engine.buffer.modification_count();

            let char_start = self.engine.buffer.line_to_char(line_idx - 1);
            let char_end = char_start + old_text.chars().count();
            self.engine.buffer.remove_range(char_start, char_end);
            self.engine.buffer.insert(line_idx, 0, &new_text);

            edits.push(crate::undo::Edit {
                edit_type: crate::undo::EditType::Replace {
                    line: line_idx,
                    col: 0,
                    old_text,
                    new_text,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
        }

        // One `u` reverts the whole `:s`, like Vim. The group records the
        // modification count from before the first substitution so undoing it
        // restores the clean state.
        if let Some(first) = edits.first() {
            let modification_count = first.modification_count;
            let cursor_before = first.cursor_before;
            self.engine.undo_manager.push(crate::undo::Edit {
                edit_type: crate::undo::EditType::Group { edits },
                cursor_before,
                cursor_after: self.engine.state.cursor,
                modification_count,
            });
        }

        self.engine.state.dirty = self.engine.buffer.is_dirty();
        self.engine.state.push_message(
            MessageLevel::Info,
            format!(
                "{} substitutions on {} line{}",
                replacements,
                if end_line > start_line {
                    format!("{}", end_line - start_line + 1)
                } else {
                    "1".to_string()
                },
                if replacements == 1 { "" } else { "s" }
            ),
        );
        self.needs_render = true;
    }

    async fn reload_file(&mut self) {
        // Directory listings cannot be reloaded as text files — refresh instead.
        if self.engine.buffer.is_directory_listing() {
            self.refresh_directory();
            return;
        }
        if let Some(path) = &self.engine.state.file_path {
            match crate::buffer::TextBuffer::load_file(path.to_str().unwrap_or("")).await {
                Ok(buf) => {
                    self.engine.buffer = buf;
                }
                Err(e) => {
                    self.engine
                        .state
                        .push_message(MessageLevel::Error, format!("Failed to reload file: {}", e));
                }
            }
        }
    }

    async fn reload_file_discard(&mut self) {
        if self.engine.buffer.is_dirty() {
            self.engine.state.set_confirmation(
                "No write since last change. Reload anyway? (y/n)".to_string(),
                ConfirmAction::ReloadDiscard,
            );
            self.needs_render = true;
        } else {
            self.reload_file().await;
        }
    }

    /// Save the current buffer and emit a BufferSave plugin event.
    /// If the save fails, set an error message instead.
    async fn save_and_emit(&mut self) {
        match self.engine.buffer.save_file().await {
            Ok(()) => {
                self.engine.state.dirty = false;
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::BufferSave {
                        file_path: self.engine.state.file_path.clone(),
                    });
            }
            Err(e) => {
                self.engine
                    .state
                    .push_message(MessageLevel::Error, format!("Save failed: {}", e));
            }
        }
    }

    async fn handle_write_path(&mut self, cmd: &str) {
        let path_str = cmd.trim_start_matches("w!").trim_start_matches("w ").trim();
        if path_str.is_empty() {
            self.engine
                .state
                .push_message(MessageLevel::Warning, "Expected filename after 'w'");
            return;
        }
        let path = std::path::PathBuf::from(path_str);
        match self.engine.buffer.save_to_path(&path).await {
            Ok(_) => {
                self.engine.state.file_path = Some(path);
                self.engine.state.dirty = false;
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::BufferSave {
                        file_path: self.engine.state.file_path.clone(),
                    });
            }
            Err(e) => {
                self.engine
                    .state
                    .push_message(MessageLevel::Error, format!("Save failed: {}", e));
            }
        }
    }

    async fn handle_write_quit_path(&mut self, cmd: &str) {
        let path_str = cmd
            .trim_start_matches("wq!")
            .trim_start_matches("wq ")
            .trim();
        if path_str.is_empty() {
            self.engine
                .state
                .push_message(MessageLevel::Warning, "Expected filename after 'wq'");
            return;
        }
        let path = std::path::PathBuf::from(path_str);
        match self.engine.buffer.save_to_path(&path).await {
            Ok(_) => {
                self.engine.state.file_path = Some(path);
                self.engine.state.dirty = false;
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::BufferSave {
                        file_path: self.engine.state.file_path.clone(),
                    });
                self.running = false;
            }
            Err(e) => {
                self.engine
                    .state
                    .push_message(MessageLevel::Error, format!("Save failed: {}", e));
            }
        }
    }

    /// Run a git command and show its output in messages.
    pub(crate) fn handle_git_command(&mut self, args: &[&str]) {
        let cmd_str = args.join(" ");
        match crate::git::run(args) {
            Ok(stdout) => {
                if stdout.is_empty() {
                    self.engine
                        .state
                        .push_message(MessageLevel::Info, format!("git {}: OK", cmd_str));
                } else {
                    self.engine
                        .state
                        .push_message(MessageLevel::Info, format!("git {}\n{}", cmd_str, stdout));
                    // For branch listing, also push a compact summary.
                    if stdout.lines().count() > 1 && args == ["branch"] {
                        let branches: Vec<&str> = stdout
                            .lines()
                            .map(|l| l.trim_start_matches("* ").trim())
                            .collect();
                        self.engine.state.push_message(
                            MessageLevel::Info,
                            format!("Branches: {}", branches.join(", ")),
                        );
                    }
                }
            }
            Err(stderr) => {
                self.engine.state.push_message(
                    MessageLevel::Error,
                    format!("git {} failed: {}", cmd_str, stderr),
                );
            }
        }
    }

    /// Reload the current file after a `git checkout` — file contents may have
    /// changed (or the file may no longer exist).
    fn reload_file_after_checkout(&mut self) {
        self.needs_render = true;
        let Some(path) = &self.engine.state.file_path.clone() else {
            return;
        };
        let path_str = path.to_string_lossy().to_string();
        if path.exists() {
            match std::fs::read_to_string(&path_str) {
                Ok(content) => {
                    self.engine.buffer = crate::buffer::TextBuffer::with_text(&content);
                    self.engine.state.file_path = Some(path.clone());
                    self.engine.state.dirty = false;
                    self.engine.state.push_message(
                        MessageLevel::Info,
                        format!("Reloaded after checkout: {}", path_str),
                    );
                }
                Err(e) => {
                    self.engine.state.push_message(
                        MessageLevel::Warning,
                        format!("Could not reload {}: {}", path_str, e),
                    );
                }
            }
        } else {
            self.engine.buffer = crate::buffer::TextBuffer::new();
            self.engine.state.file_path = None;
            self.engine.state.push_message(
                MessageLevel::Warning,
                format!("File not found in this branch: {}", path_str),
            );
        }
    }
}

/// Commands that accept a leading Ex range in this editor.
fn is_range_command(rest: &str) -> bool {
    matches!(rest, "d" | "y" | ">" | "<") || rest.starts_with("s/")
}

impl Editor {
    /// Split a leading Ex range off `cmd`.
    ///
    /// Supports `'<,'>`, `%`, `N`, `N,M`, `.`, `$` and combinations. Returns
    /// the resolved inclusive line range and the command that follows it.
    fn split_ex_range<'a>(&self, cmd: &'a str) -> Option<((usize, usize), &'a str)> {
        let line_count = self.engine.buffer.content_line_count().max(1);
        if let Some(rest) = cmd.strip_prefix("'<,'>") {
            let start = self
                .engine
                .state
                .last_visual_start
                .map(|p| p.line)
                .unwrap_or(self.engine.state.cursor.line)
                .clamp(1, line_count);
            let end = self
                .engine
                .state
                .last_visual_end
                .or(self.engine.state.last_visual_start)
                .map(|p| p.line)
                .unwrap_or(self.engine.state.cursor.line)
                .clamp(1, line_count);
            let rest = rest.trim_start();
            return is_range_command(rest).then_some(((start.min(end), start.max(end)), rest));
        }

        // The command starts at the first letter or `>`/`<`; everything
        // before it must look like a range.
        let idx = cmd
            .char_indices()
            .find(|(_, c)| c.is_ascii_alphabetic() || matches!(c, '>' | '<'))?
            .0;
        if idx == 0 {
            return None;
        }
        let spec = cmd[..idx].trim();
        if spec.is_empty() {
            return None;
        }
        let rest = cmd[idx..].trim_start();
        if !is_range_command(rest) {
            return None;
        }
        let range = parse_line_range(spec, self.engine.state.cursor.line, line_count)?;
        Some((range, rest))
    }

    /// Run a range command (`d`, `y`, `>`, `<`, `s/...`).
    fn handle_ex_range(&mut self, range: (usize, usize), rest: &str) {
        let (start, end) = range;
        match rest {
            "d" => {
                if start <= end {
                    self.delete_lines_range('"', start, end);
                }
            }
            "y" => {
                if start <= end {
                    self.yank_lines_range('"', start, end);
                }
            }
            ">" => self.indent_lines(start, end, true),
            "<" => self.indent_lines(start, end, false),
            _ if rest.starts_with("s/") => {
                self.handle_substitute(&format!("{start},{end}{rest}"));
            }
            _ => {
                self.engine
                    .state
                    .push_message(MessageLevel::Warning, format!("Unknown command: {}", rest));
            }
        }
        self.needs_render = true;
    }
}

/// Parse a `:s` range: `%`, ``, `3`, `1,5`, `.,$`, `$`.
///
/// Returns `(start, end)` clamped to the buffer, or `None` when the text is
/// not a range (so the caller can report it instead of guessing).
fn parse_line_range(spec: &str, current: usize, line_count: usize) -> Option<(usize, usize)> {
    if spec == "%" {
        return Some((1, line_count.max(1)));
    }
    match spec.split_once(',') {
        Some((a, b)) => {
            let start = parse_line_bound(a, current, line_count)?;
            let end = parse_line_bound(b, current, line_count)?;
            Some((start.min(end), start.max(end)))
        }
        None => {
            let n = parse_line_bound(spec, current, line_count)?;
            Some((n, n))
        }
    }
}

fn parse_line_bound(token: &str, current: usize, line_count: usize) -> Option<usize> {
    let token = token.trim();
    match token {
        "" | "." => Some(current),
        "$" => Some(line_count.max(1)),
        _ => token
            .parse::<usize>()
            .ok()
            .map(|n| n.clamp(1, line_count.max(1))),
    }
}

/// Translate Vim replacement syntax into the `regex` crate's.
///
/// Vim: `&` = whole match, `\1`..`\9` = capture groups, `\&` = literal `&`.
/// regex: `$0` = whole match, `$1`..`$9` = capture groups, `$$` = literal `$`.
fn translate_replacement(replacement: &str) -> String {
    let mut out = String::new();
    let mut chars = replacement.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(d @ '1'..='9') => {
                    out.push('$');
                    out.push(d);
                }
                Some('&') => out.push('&'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            '&' => out.push_str("$0"),
            '$' => out.push_str("$$"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod substitute_tests {
    use super::*;

    #[test]
    fn ranges_parse() {
        assert_eq!(parse_line_range("%", 3, 10), Some((1, 10)));
        assert_eq!(parse_line_range("", 3, 10), Some((3, 3)));
        assert_eq!(parse_line_range("3", 7, 10), Some((3, 3)));
        assert_eq!(parse_line_range("1,5", 7, 10), Some((1, 5)));
        assert_eq!(parse_line_range(".,$", 4, 10), Some((4, 10)));
        assert_eq!(parse_line_range("5,2", 1, 10), Some((2, 5)));
        assert_eq!(parse_line_range("99", 1, 10), Some((10, 10)), "clamped");
        assert_eq!(parse_line_range("x", 1, 10), None);
        assert_eq!(parse_line_range("1,x", 1, 10), None);
    }

    #[test]
    fn replacement_translates_vim_syntax() {
        assert_eq!(translate_replacement("<&>"), "<$0>");
        assert_eq!(translate_replacement(r"\1-\2"), "$1-$2");
        assert_eq!(translate_replacement(r"\&"), "&");
        assert_eq!(translate_replacement("cost: $5"), "cost: $$5");
    }
}

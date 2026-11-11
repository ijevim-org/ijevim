use crate::editor::Editor;
use crate::types::MessageLevel;
use crossterm::event::KeyCode;

impl Editor {
    // ── Dashboard key handling ─────────────────────────────────

    /// Handle a key press while the welcome dashboard is shown.
    ///
    /// Returns `true` if the key was consumed by the dashboard.
    /// Returns `false` if the key should fall through to normal processing
    /// (which also dismisses the dashboard).
    pub(crate) fn handle_dashboard_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Char('e') => {
                self.engine.state.show_dashboard = false;
                // Trigger :Ex — open file explorer in the current directory
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
                true
            }
            KeyCode::Char('f') => {
                self.engine.state.show_dashboard = false;
                // Run `git ls-files` and show results as a message, then
                // let the user `:e <path>` to open one.
                match crate::git::run(&["ls-files"]) {
                    Ok(stdout) => {
                        let files: Vec<&str> = stdout.lines().collect();
                        if files.is_empty() {
                            self.engine.state.push_message(
                                MessageLevel::Info,
                                "No tracked files in this repository".to_string(),
                            );
                        } else {
                            let msg = format!(
                                "Git-tracked files ({}): {}",
                                files.len(),
                                files.join(", ")
                            );
                            self.engine.state.push_message(MessageLevel::Info, msg);
                        }
                    }
                    Err(stderr) => {
                        self.engine.state.push_message(
                            MessageLevel::Info,
                            format!(
                                "Not a git repository (run :e <path> to open a file): {}",
                                stderr
                            ),
                        );
                    }
                }
                self.needs_render = true;
                true
            }
            KeyCode::Char('q') => {
                self.handle_quit();
                true
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                // Dismiss dashboard and show the empty buffer
                self.engine.state.show_dashboard = false;
                self.needs_render = true;
                true
            }
            _ => {
                // Any other key dismisses the dashboard and falls through
                // to normal key processing
                self.engine.state.show_dashboard = false;
                self.needs_render = true;
                false
            }
        }
    }
}

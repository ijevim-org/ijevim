use crate::editor::Editor;
use crate::types::MessageLevel;

impl Editor {
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
    pub(super) fn reload_file_after_checkout(&mut self) {
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
            )
        }
    }
}

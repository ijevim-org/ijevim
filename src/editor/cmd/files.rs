use crate::editor::Editor;
use crate::types::{ConfirmAction, MessageLevel};

impl Editor {
    pub(super) async fn reload_file(&mut self) {
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

    pub(super) async fn reload_file_discard(&mut self) {
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
    pub(super) async fn save_and_emit(&mut self) {
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

    pub(super) async fn handle_write_path(&mut self, cmd: &str) {
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

    pub(super) async fn handle_write_quit_path(&mut self, cmd: &str) {
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
}

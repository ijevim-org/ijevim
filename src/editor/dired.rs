use crate::editor::Editor;
use crate::types::MessageLevel;

impl Editor {
    // ── Directory browsing (file browser) ──────────────────────────

    /// Reset operator state and consume any pending count.
    /// Called before every directory-browsing action to prevent stale
    /// operator/count from leaking into the next buffer.
    fn reset_dir_operator_state(&mut self) {
        self.engine.operator_state.reset();
        // Discard any count the user typed before the action.
        let _ = self.engine.operator_state.count.take();
    }

    /// Open the directory entry under the cursor.
    /// Directories navigate into them; files open as normal buffers.
    pub(crate) fn open_selected_entry(&mut self) {
        self.reset_dir_operator_state();
        let line = self.engine.state.cursor.line;
        if let Some(entry) = self.engine.buffer.selected_entry(line) {
            let path = entry.path.to_string_lossy().to_string();
            if entry.is_dir {
                if let Err(e) = self.engine.buffer_open_dir(&path) {
                    self.engine.state.push_message(MessageLevel::Error, e);
                }
            } else if let Err(e) = self.engine.buffer_open(&path) {
                self.engine.state.push_message(MessageLevel::Error, e);
            }
            self.needs_render = true;
        }
    }

    /// Navigate to the parent directory.
    pub(crate) fn go_up_directory(&mut self) {
        self.reset_dir_operator_state();
        if let Some(dir_path) = self.engine.buffer.dir_path().map(|p| p.to_path_buf()) {
            if let Some(parent) = dir_path.parent() {
                let path = parent.to_string_lossy().to_string();
                if path.is_empty() {
                    return;
                }
                if let Err(e) = self.engine.buffer_open_dir(&path) {
                    self.engine.state.push_message(MessageLevel::Error, e);
                }
                self.needs_render = true;
            }
        }
    }

    /// Refresh (re-read) the current directory listing.
    pub(crate) fn refresh_directory(&mut self) {
        self.reset_dir_operator_state();
        if let Some(dir_path) = self.engine.buffer.dir_path().map(|p| p.to_path_buf()) {
            let path = dir_path.to_string_lossy().to_string();
            if let Err(e) = self.engine.buffer_open_dir(&path) {
                self.engine.state.push_message(MessageLevel::Error, e);
            }
            self.needs_render = true;
        }
    }
}

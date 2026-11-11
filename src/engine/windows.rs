use crate::engine::Engine;

impl Engine {
    /// Switch focus to the window at the given index.
    /// Swaps the active buffer with the target window's buffer.
    pub fn focus_window(&mut self, idx: usize) -> Result<(), String> {
        if idx >= self.window_layout.windows.len() {
            return Err("Invalid window index".to_string());
        }
        if idx == self.window_layout.focused {
            return Ok(());
        }

        let target_buf_idx = self.window_layout.windows[idx].buf_idx;
        let current_focused = self.window_layout.focused;

        // Save current window's state
        self.window_layout.windows[current_focused].cursor = self.state.cursor;
        self.window_layout.windows[current_focused].scroll_top = self.state.cursor.line; // Approximate

        // If target is an inactive buffer, swap buffers
        if target_buf_idx == 0 {
            // Target is the active buffer (already current), just update focus
        } else if target_buf_idx > 0 && target_buf_idx - 1 < self.inactive_buffers.len() {
            // Swap active buffer with target inactive buffer
            let target_idx = target_buf_idx - 1;
            let current = (
                std::mem::take(&mut self.buffer),
                std::mem::take(&mut self.undo_manager),
                self.state.file_path.take(),
                self.state.cursor,
            );
            // Update the inactive buffer we're leaving. The slot may not exist
            // yet when the focused window index is ahead of the buffer list.
            match self.inactive_buffers.get_mut(current_focused) {
                Some(slot) => *slot = current,
                None => self.inactive_buffers.push(current),
            }
            // Swap in the target buffer
            let target = self.inactive_buffers.remove(target_idx);
            self.buffer = target.0;
            self.undo_manager = target.1;
            self.state.file_path = target.2;
            self.state.cursor = target.3;
            self.state.dirty = self.buffer.is_dirty();
        }

        // Update window layout
        self.window_layout.windows[idx].buf_idx = 0;
        if current_focused < self.window_layout.windows.len() {
            self.window_layout.windows[current_focused].buf_idx = idx + 1; // Move old active to inactive slot
        }
        self.window_layout.focused = idx;

        // Restore window state
        self.state.cursor = self.window_layout.windows[idx].cursor;
        self.state.dirty = self.buffer.is_dirty();

        Ok(())
    }

    /// Focus the next window (Ctrl-w w).
    pub fn focus_next_window(&mut self) -> Result<(), String> {
        let next = (self.window_layout.focused + 1) % self.window_layout.windows.len();
        self.focus_window(next)
    }

    /// Focus window in direction.
    pub fn focus_window_direction(&mut self, direction: crate::types::Direction) {
        self.window_layout.focus_direction(direction);
    }

    /// Split current window horizontally (:split).
    pub fn split_horizontal(&mut self) -> Result<(), String> {
        // Save current window state
        let focused = self.window_layout.focused;
        self.window_layout.windows[focused].cursor = self.state.cursor;
        self.window_layout.windows[focused].scroll_top = self.state.cursor.line;

        // Split the window
        self.window_layout.split_horizontal();

        // New window gets same buffer (buf_idx = 0 for both)
        let new_focused = self.window_layout.focused;
        self.window_layout.windows[new_focused].buf_idx = 0;
        self.window_layout.windows[new_focused].cursor = self.state.cursor;
        self.window_layout.windows[new_focused].scroll_top = self.state.cursor.line;

        Ok(())
    }

    /// Split current window vertically (:vsplit).
    pub fn split_vertical(&mut self) -> Result<(), String> {
        // Save current window state
        let focused = self.window_layout.focused;
        self.window_layout.windows[focused].cursor = self.state.cursor;
        self.window_layout.windows[focused].scroll_top = self.state.cursor.line;

        // Split the window
        self.window_layout.split_vertical();

        // New window gets same buffer
        let new_focused = self.window_layout.focused;
        self.window_layout.windows[new_focused].buf_idx = 0;
        self.window_layout.windows[new_focused].cursor = self.state.cursor;
        self.window_layout.windows[new_focused].scroll_top = self.state.cursor.line;

        Ok(())
    }

    /// Focus the previous window (Emacs `C-x O`).
    pub fn focus_prev_window(&mut self) -> Result<(), String> {
        let prev = if self.window_layout.focused == 0 {
            self.window_layout.windows.len().saturating_sub(1)
        } else {
            self.window_layout.focused - 1
        };
        self.focus_window(prev)
    }

    /// Close every window except the focused one (Emacs `C-x 1`).
    pub fn close_other_windows(&mut self) -> Result<(), String> {
        if self.window_layout.windows.len() <= 1 {
            return Err("Already only one window".to_string());
        }
        self.window_layout.close_other_windows();
        Ok(())
    }

    /// Close focused window (Ctrl-w c).
    pub fn close_focused_window(&mut self) -> Result<(), String> {
        if self.window_layout.windows.len() <= 1 {
            return Err("Cannot close last window".to_string());
        }
        let focused = self.window_layout.focused;
        self.window_layout.close_focused();
        // If we closed the focused window, focus shifts to next
        if self.window_layout.focused != focused {
            self.focus_window(self.window_layout.focused)?;
        }
        Ok(())
    }

    /// Equalize all window sizes (Ctrl-w =).
    pub fn equalize_windows(&mut self, rows: usize, cols: usize) {
        self.window_layout.equalize(rows, cols);
    }

    /// Maximize focused window vertically (Ctrl-w _).
    pub fn maximize_vertical(&mut self, rows: usize, cols: usize) {
        self.window_layout.maximize_vertical(rows, cols);
    }

    /// Maximize focused window horizontally (Ctrl-w |).
    pub fn maximize_horizontal(&mut self, rows: usize, cols: usize) {
        self.window_layout.maximize_horizontal(rows, cols);
    }

    /// Update window layout dimensions on terminal resize.
    ///
    /// Scales the existing splits; it must not rebuild the layout, or every
    /// `:split` closes the moment the terminal is resized.
    pub fn update_window_layout(&mut self, rows: usize, cols: usize) {
        self.window_layout.resize(rows, cols);
    }
}

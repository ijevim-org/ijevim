use crate::buffer::TextBuffer;
use crate::highlight::Style;
use crate::plugin::PluginManager;
use crate::register::Register;
use crate::review::ReviewState;
use crate::state::{CursorState, InsertState, OperatorState, SearchState};
use crate::types::{ConfirmAction, DirEntry, DotAction, EditorState, KillRing, Mode, Position};
use crate::undo::UndoManager;
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;
use std::time::Instant;

/// Signals from Engine to the UI layer after an operation.
///
/// Each Engine method returns an `EngineResult` that the caller (Editor or Application)
/// uses to update UI state (render, quit, messages, etc.).
#[derive(Debug, Default)]
pub(crate) struct EngineResult {
    /// Whether the screen needs a re-render.
    pub needs_render: bool,
    /// If set, update the running (quit) flag accordingly.
    pub running: Option<bool>,
    /// If set, display this message in the status line.
    pub message: Option<String>,
    /// If set, a confirmation prompt should be shown.
    pub confirmation: Option<(String, ConfirmAction)>,
}

impl EngineResult {
    pub fn with_render() -> Self {
        Self {
            needs_render: true,
            ..Self::default()
        }
    }

    pub fn with_message(msg: impl Into<String>) -> Self {
        Self {
            message: Some(msg.into()),
            ..Self::default()
        }
    }
}

/// Core editor engine — owns all editor state and operations.
///
/// Engine contains the buffer, undo history, registers, cursor state, operator
/// state machine, and search state. It does NOT own UI components (terminal,
/// renderer, keymap handler) — those belong to the Editor layer.
pub struct Engine {
    pub buffer: TextBuffer,
    pub undo_manager: UndoManager,
    pub register: Register,
    pub state: EditorState,
    pub operator_state: OperatorState,
    pub search_state: SearchState,
    pub insert_state: InsertState,
    pub cursor_state: CursorState,
    pub plugin_manager: PluginManager,
    pub kill_ring: KillRing,
    pub(crate) dot_last_action: Option<DotAction>,
    /// Edits accumulated during Replace mode, flushed as a single undo group on Esc.
    pub(crate) replace_undo_group: Vec<crate::undo::Edit>,
    /// Inactive buffers: (TextBuffer, UndoManager, file_path, cursor_position).
    pub inactive_buffers: Vec<(TextBuffer, UndoManager, Option<PathBuf>, Position)>,
    /// Window layout for split views.
    pub window_layout: crate::types::WindowLayout,
    #[cfg(feature = "syntax")]
    pub(crate) highlighter: crate::highlight::Highlighter,
    #[cfg(feature = "syntax")]
    pub(crate) last_highlight_mod_count: usize,
    pub(crate) highlights: Vec<Vec<(Style, String)>>,
    pub last_keypress_time: Instant,
    /// Review mode state — `None` when not in review mode.
    pub review_state: Option<ReviewState>,
}

impl Engine {
    pub fn new(buffer: TextBuffer, plugin_manager: PluginManager, state: EditorState) -> Self {
        Self {
            buffer,
            undo_manager: UndoManager::new(),
            register: Register::new(),
            state,
            operator_state: OperatorState::default(),
            search_state: SearchState::default(),
            insert_state: InsertState::default(),
            cursor_state: CursorState::default(),
            plugin_manager,
            kill_ring: KillRing::new(),
            dot_last_action: None,
            replace_undo_group: Vec::new(),
            inactive_buffers: Vec::new(),
            window_layout: crate::types::WindowLayout::new_single(24, 80), // Default, updated by Editor
            #[cfg(feature = "syntax")]
            highlighter: crate::highlight::Highlighter::new(),
            #[cfg(feature = "syntax")]
            last_highlight_mod_count: 0,
            highlights: Vec::new(),
            last_keypress_time: Instant::now(),
            review_state: None,
        }
    }

    /// Emit a key-press event to the plugin system and record macros if recording.
    pub(crate) fn emit_key_event(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        let key_str = Self::encode_key(key, modifiers);

        self.plugin_manager.emit(crate::types::PluginEvent::Key {
            mode: self.state.mode,
            key: key_str.clone(),
        });

        if self.state.macros.is_recording() {
            self.state.macros.add_key(key_str);
        }
    }

    /// Encode a key with optional modifiers into a stable string for macro storage.
    ///
    /// Format: `{modifier-}{key}` where modifier is `C-` for Ctrl, `M-` for Alt.
    /// Unmodified keys are stored as-is for backward compatibility.
    fn encode_key(key: KeyCode, modifiers: KeyModifiers) -> String {
        let prefix = if modifiers.contains(KeyModifiers::CONTROL) {
            "C-"
        } else if modifiers.contains(KeyModifiers::ALT) {
            "M-"
        } else {
            ""
        };
        let key_body = match key {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "BackTab".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::F(n) => format!("F{}", n),
            KeyCode::Null => "Null".to_string(),
            KeyCode::CapsLock => "CapsLock".to_string(),
            KeyCode::ScrollLock => "ScrollLock".to_string(),
            KeyCode::NumLock => "NumLock".to_string(),
            KeyCode::PrintScreen => "PrintScreen".to_string(),
            KeyCode::Pause => "Pause".to_string(),
            KeyCode::Menu => "Menu".to_string(),
            KeyCode::KeypadBegin => "KeypadBegin".to_string(),
            KeyCode::Media(m) => format!("Media({:?})", m),
            KeyCode::Modifier(m) => format!("Modifier({:?})", m),
        };
        format!("{}{}", prefix, key_body)
    }

    /// Emit BufferChange event and sync dirty flag.
    pub(crate) fn emit_buffer_change(&mut self) {
        self.state.dirty = self.buffer.is_dirty();
        self.plugin_manager
            .emit(crate::types::PluginEvent::BufferChange);
    }

    pub(crate) fn undo(&mut self) -> EngineResult {
        if let Some(edit) = self.undo_manager.undo(&mut self.buffer) {
            self.state.cursor = edit.cursor_before;
            self.state.dirty = self.buffer.is_dirty();
            EngineResult::with_render()
        } else {
            EngineResult::default()
        }
    }

    pub(crate) fn redo(&mut self) -> EngineResult {
        if let Some(edit) = self.undo_manager.redo(&mut self.buffer) {
            self.state.cursor = edit.cursor_after;
            self.state.dirty = self.buffer.is_dirty();
            EngineResult::with_render()
        } else {
            EngineResult::default()
        }
    }

    pub(crate) fn show_file_info(&mut self) -> EngineResult {
        let line_count = self.buffer.line_count();
        let _col = self.state.cursor.col + 1;
        let total = self.buffer.len_chars();
        let path = self
            .state
            .file_path
            .as_ref()
            .map(|p| p.to_str().unwrap_or(""))
            .unwrap_or("[No Name]");
        EngineResult::with_message(format!(
            "\"{}\" {} lines, {} characters",
            path, line_count, total
        ))
    }

    pub(crate) async fn save_file_async(&mut self) -> EngineResult {
        if let Err(e) = self.buffer.save_file().await {
            return EngineResult::with_message(format!("Save failed: {}", e));
        }
        self.plugin_manager
            .emit(crate::types::PluginEvent::BufferSave {
                file_path: self.state.file_path.clone(),
            });
        self.state.dirty = false;
        EngineResult::with_render()
    }

    pub fn set_buffer_for_test(&mut self, text: &str) {
        self.buffer = TextBuffer::with_text(text);
        self.state.cursor.line = 1;
        self.state.cursor.col = 0;
        self.state.mode = Mode::Normal;
        self.state.dirty = false;
        self.undo_manager.clear();
        self.buffer.mark_clean();
    }

    pub fn snapshot_for_test(&self) -> (String, usize, usize, Mode) {
        (
            self.buffer.to_string(),
            self.state.cursor.line,
            self.state.cursor.col,
            self.state.mode,
        )
    }

    /// Reset operator state, mode, command buffer on keymap switch.
    pub fn reset_state_for_keymap_switch(&mut self) {
        self.operator_state.reset();
        self.state.mode = Mode::Normal;
        self.state.visual_start = None;
        self.state.visual_type = None;
        self.state.command_buffer.clear();
        self.state.command_cursor_pos = 0;
    }

    pub fn take_count(&mut self) -> usize {
        self.operator_state.take_count()
    }

    // ── Multi-buffer support ──────────────────────────────────────

    /// Switch to the next buffer (`:bn`).
    pub fn buffer_next(&mut self) -> Result<(), String> {
        if self.inactive_buffers.is_empty() {
            return Err("No other buffer".to_string());
        }
        // Save current buffer.
        let current = (
            std::mem::take(&mut self.buffer),
            std::mem::take(&mut self.undo_manager),
            self.state.file_path.take(),
            self.state.cursor,
        );
        self.inactive_buffers.push(current);
        // Swap in the next buffer (rotate left).
        let first = self.inactive_buffers.remove(0);
        self.buffer = first.0;
        self.undo_manager = first.1;
        self.state.file_path = first.2;
        self.state.cursor = first.3;
        self.state.dirty = self.buffer.is_dirty();
        Ok(())
    }

    /// Switch to the previous buffer (`:bp`).
    pub fn buffer_prev(&mut self) -> Result<(), String> {
        if self.inactive_buffers.is_empty() {
            return Err("No other buffer".to_string());
        }
        // Save current buffer.
        let current = (
            std::mem::take(&mut self.buffer),
            std::mem::take(&mut self.undo_manager),
            self.state.file_path.take(),
            self.state.cursor,
        );
        self.inactive_buffers.insert(0, current);
        // Swap in the last buffer.
        let last = self.inactive_buffers.pop().unwrap();
        self.buffer = last.0;
        self.undo_manager = last.1;
        self.state.file_path = last.2;
        self.state.cursor = last.3;
        self.state.dirty = self.buffer.is_dirty();
        Ok(())
    }

    /// Switch to a buffer by file name (`C-x b`).
    ///
    /// An empty name means "the other buffer" (same as `buffer_next`). A name
    /// matches either the exact file name or the full path.
    pub fn buffer_switch_by_name(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return self.buffer_next();
        }

        let name_of = |path: &Option<PathBuf>| -> String {
            path.as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "[No Name]".to_string())
        };

        if name_of(&self.state.file_path) == name
            || self
                .state
                .file_path
                .as_ref()
                .is_some_and(|p| p.to_string_lossy() == name)
        {
            return Ok(());
        }

        let target_idx = self
            .inactive_buffers
            .iter()
            .position(|(_, _, path, _)| {
                name_of(path) == name || path.as_ref().is_some_and(|p| p.to_string_lossy() == name)
            })
            .ok_or_else(|| format!("No buffer matching '{}'", name))?;

        let current = (
            std::mem::take(&mut self.buffer),
            std::mem::take(&mut self.undo_manager),
            self.state.file_path.take(),
            self.state.cursor,
        );
        let target = self.inactive_buffers.remove(target_idx);
        self.inactive_buffers.push(current);
        self.buffer = target.0;
        self.undo_manager = target.1;
        self.state.file_path = target.2;
        self.state.cursor = target.3;
        self.state.dirty = self.buffer.is_dirty();
        Ok(())
    }

    /// Delete the current buffer and switch to the next (`:bdelete`).
    pub fn buffer_delete(&mut self) -> Result<(), String> {
        if self.inactive_buffers.is_empty() {
            // No other buffer to switch to — just clear the current one.
            self.buffer = TextBuffer::new();
            self.undo_manager = UndoManager::new();
            self.state.file_path = None;
            self.state.cursor = Position { line: 1, col: 0 };
            self.state.dirty = false;
            return Ok(());
        }
        // Discard current buffer, swap in next.
        let first = self.inactive_buffers.remove(0);
        self.buffer = first.0;
        self.undo_manager = first.1;
        self.state.file_path = first.2;
        self.state.cursor = first.3;
        self.state.dirty = self.buffer.is_dirty();
        Ok(())
    }

    /// Open a file in a new buffer (`:e {path}` without switching).
    pub fn buffer_open(&mut self, path: &str) -> Result<(), String> {
        use std::path::Path;
        let p = Path::new(path).to_path_buf();
        let content =
            std::fs::read_to_string(&p).map_err(|e| format!("Cannot read '{}': {}", path, e))?;
        let mut new_buffer = TextBuffer::new();
        new_buffer.set_file_path(Some(p.clone()));
        new_buffer.insert(1, 0, &content);

        // Switch current buffer to inactive, open new one as active.
        let current = (
            std::mem::replace(&mut self.buffer, new_buffer),
            std::mem::take(&mut self.undo_manager),
            self.state.file_path.take(),
            self.state.cursor,
        );
        self.inactive_buffers.push(current);
        self.state.file_path = Some(p);
        self.state.cursor = Position { line: 1, col: 0 };
        self.state.dirty = false;
        Ok(())
    }

    /// Open a directory listing in a new buffer (`:e <dir>` / `:Ex`).
    pub fn buffer_open_dir(&mut self, path: &str) -> Result<(), String> {
        use std::path::Path;
        let dir = Path::new(path).to_path_buf();

        let read_dir = std::fs::read_dir(&dir)
            .map_err(|e| format!("Cannot read directory '{}': {}", path, e))?;

        let mut entries: Vec<DirEntry> = read_dir
            .filter_map(|e| e.ok())
            .map(|e| {
                // Use metadata() which follows symlinks — a symlink to a
                // directory should be treated as a directory entry.
                let is_dir = e.metadata().map(|m| m.is_dir()).unwrap_or(false);
                let name = if is_dir {
                    format!("{}/", e.file_name().to_string_lossy())
                } else {
                    e.file_name().to_string_lossy().to_string()
                };
                DirEntry {
                    name,
                    path: e.path(),
                    is_dir,
                }
            })
            .collect();

        // Sort: directories first, then files; alphabetically within each group.
        entries.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
                a.name.cmp(&b.name)
            }
        });

        // Generate listing text (one line per entry, no trailing empty line).
        let listing = entries
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<&str>>()
            .join("\n");

        let mut new_buffer = TextBuffer::new();
        new_buffer.set_dir_listing(entries, dir.clone());
        if listing.is_empty() {
            // Show a placeholder so the buffer isn't completely blank.
            new_buffer.insert(1, 0, "(empty)");
        } else {
            new_buffer.insert(1, 0, &listing);
        }

        // Switch current buffer to inactive, open new one as active.
        let current = (
            std::mem::replace(&mut self.buffer, new_buffer),
            std::mem::take(&mut self.undo_manager),
            self.state.file_path.take(),
            self.state.cursor,
        );
        self.inactive_buffers.push(current);
        self.state.file_path = Some(dir);
        self.state.cursor = Position { line: 1, col: 0 };
        self.state.dirty = false;
        Ok(())
    }

    /// List all buffers (`:ls`). Returns a description string.
    pub fn buffer_list(&self) -> String {
        let mut lines = Vec::new();
        // Current buffer.
        let current_name = self
            .state
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "[No Name]".to_string());
        let current_dirty = if self.buffer.is_dirty() { " +" } else { "" };
        lines.push(format!("  * {}{}", current_name, current_dirty));
        // Inactive buffers.
        for (i, (_, _, path, _)) in self.inactive_buffers.iter().enumerate() {
            let name = path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "[No Name]".to_string());
            // Check if dirty by checking modification count.
            let dirty_marker = "";
            lines.push(format!("  {} {}{}", i + 1, name, dirty_marker));
        }
        lines.join("\n")
    }

    // ── Window management ──────────────────────────────────────────

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

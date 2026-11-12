use crate::buffer::TextBuffer;
use crate::highlight::Style;
use crate::plugin::PluginManager;
use crate::register::Register;
use crate::review::ReviewState;
use crate::state::{CursorState, EmacsState, InsertState, OperatorState, SearchState};
use crate::types::{ConfirmAction, DotAction, EditorState, KillRing, Mode, Position};
use crate::undo::UndoManager;
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;
use std::time::Instant;

mod buffers;
mod windows;

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
    /// Emacs keymap input state, kept separate from Vim's operator machine.
    pub emacs_state: EmacsState,
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
            emacs_state: EmacsState::default(),
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
        self.emacs_state.reset();
        self.insert_state = InsertState::default();
        self.cursor_state = CursorState::default();
        self.state.mode = Mode::Normal;
        self.state.visual_start = None;
        self.state.visual_type = None;
        // Emacs regions are keymap-local; the mark itself is user data and
        // survives a switch, but the active region must not leak into Vim.
        self.state.region_active = false;
        self.state.command_buffer.clear();
        self.state.command_cursor_pos = 0;
        self.state.command_hint = None;
    }

    pub fn take_count(&mut self) -> usize {
        self.operator_state.take_count()
    }
}

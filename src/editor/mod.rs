pub(crate) mod cmd;
pub(crate) mod cmd_complete;
mod dashboard;
mod dired;
pub(crate) mod emacs_ops;
pub(crate) mod insert;
pub(crate) mod macro_marks;
pub(crate) mod motion;
pub(crate) mod normal;
pub(crate) mod number;
pub(crate) mod search;
pub(crate) mod text_object;
pub(crate) mod visual;

#[cfg(test)]
mod tests;

use crate::buffer::TextBuffer;
use crate::config::Config;
use crate::engine::{Engine, EngineResult};
use crate::keymap::{create_keymap, KeymapHandler};
use crate::plugin::PluginManager;
use crate::renderer::Renderer;
use crate::review::ReviewArgs;
use crate::state::SearchState;
use crate::terminal::Terminal;
use crate::types::{
    EditorState, Keymap, MessageLevel, Mode, Position, Result, SearchDirection, SearchResult,
};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::cell::RefCell;
#[allow(unused_imports)]
use std::collections::VecDeque;
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// State for a Tab-completion cycle in the command line.
pub(crate) struct CompletionSession {
    /// Byte offset where the completed token starts.
    pub start: usize,
    pub candidates: Vec<String>,
    pub index: usize,
}

/// Pending block-wise insert (`Ctrl-v` + `I`/`A`/`c`).
///
/// The first line is edited live while typing; when Insert mode ends the
/// accumulated text is replayed at `col` on the remaining lines.
pub(crate) struct BlockInsert {
    /// Column the typed text is replayed at on the trailing lines.
    pub col: usize,
    /// Column the cursor rests on once the block insert finishes.
    pub cursor_col: usize,
    pub start_line: usize,
    pub end_line: usize,
}

/// State for the incremental search preview while typing `/` or `?`.
pub(crate) struct SearchPreview {
    pub origin: Position,
    pub direction: SearchDirection,
    pub pattern: String,
    pub prev_pattern: Option<String>,
    pub prev_search: SearchState,
    pub results: Vec<SearchResult>,
    pub current_idx: usize,
}

pub struct Editor {
    pub engine: Engine,
    pub terminal: Terminal,
    pub renderer: Renderer,
    pub(crate) keymap_handler: Rc<RefCell<dyn KeymapHandler>>,
    pub running: bool,
    pub(crate) needs_render: bool,
    pub config: Config,
    /// Last modification count when auto-scroll was computed.
    /// Used to skip wrapped_rows computation when nothing changed.
    last_scroll_mod_count: usize,
    /// Cached cursor line used to skip auto-scroll on identical state.
    last_scroll_cursor_line: usize,
    /// Currently active keymap (tracks the enum variant).
    current_keymap: Keymap,
    /// Keymap to restore when exiting review mode.
    saved_keymap: Option<Keymap>,
    /// Command-line history (`:` commands).
    pub(crate) command_history: Vec<String>,
    /// Search-pattern history (`/` and `?`).
    pub(crate) search_history: Vec<String>,
    /// Current position while browsing history; `None` means editing a draft.
    pub(crate) history_index: Option<usize>,
    /// Draft line preserved while browsing history.
    pub(crate) history_draft: String,
    /// Active Tab-completion cycle.
    pub(crate) completion_session: Option<CompletionSession>,
    /// Active incremental-search preview.
    pub(crate) search_preview: Option<SearchPreview>,
    /// Set when a command moved the viewport itself; suppresses auto-scroll.
    pub(crate) viewport_override: bool,
    /// `Ctrl-o` in Insert mode: the next command runs in Normal mode.
    pub(crate) one_shot_normal: bool,
    /// Active block insert, replayed on Escape.
    pub(crate) block_insert: Option<BlockInsert>,
}

impl Editor {
    /// Notify buffer modification to plugins and update dirty flag.
    pub(crate) fn on_buffer_modified(&mut self) {
        self.engine.emit_buffer_change();
        self.needs_render = true;
    }

    /// Transition mode with plugin event.
    pub(crate) fn transition_mode(&mut self, to: Mode) {
        let from = self.engine.state.mode;
        self.engine.state.mode = to;
        self.engine
            .plugin_manager
            .emit(crate::types::PluginEvent::ModeChange { from, to });
        self.needs_render = true;
    }

    /// Apply an EngineResult to the Editor's UI state.
    fn apply_result(&mut self, r: EngineResult) {
        if r.needs_render {
            self.needs_render = true;
        }
        if let Some(running) = r.running {
            self.running = running;
        }
        if let Some(msg) = r.message {
            self.engine.state.push_message(MessageLevel::Info, msg);
        }
        if let Some((msg, action)) = r.confirmation {
            self.engine.state.set_confirmation(msg, action);
        }
    }

    /// Shared constructor.
    fn new_shared(
        terminal: Terminal,
        buffer: TextBuffer,
        plugin_manager: PluginManager,
        mut state: EditorState,
        keymap: Keymap,
        cfg: Config,
    ) -> Self {
        state.keymap = keymap;
        Self {
            engine: Engine::new(buffer, plugin_manager, state),
            terminal,
            renderer: Renderer::new(),
            keymap_handler: create_keymap(keymap),
            running: false,
            needs_render: true,
            config: cfg,
            last_scroll_mod_count: 0,
            last_scroll_cursor_line: 0,
            current_keymap: keymap,
            saved_keymap: None,
            command_history: Vec::new(),
            search_history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
            completion_session: None,
            search_preview: None,
            viewport_override: false,
            one_shot_normal: false,
            block_insert: None,
        }
    }

    pub fn new_headless_for_test(keymap: Keymap) -> Result<Self> {
        let terminal = Terminal::new()?;
        let buffer = TextBuffer::new();
        let state = EditorState::default();

        Ok(Editor::new_shared(
            terminal,
            buffer,
            PluginManager::new(),
            state,
            keymap,
            Config::default(),
        ))
    }

    pub async fn new(file_path: Option<&str>, keymap: Keymap, cfg: Config) -> Result<Self> {
        let mut terminal = Terminal::new()?;
        terminal.enable_raw_mode()?;

        let buffer = if let Some(path) = file_path {
            match TextBuffer::load_file(path).await {
                Ok(buf) => buf,
                Err(e) => {
                    eprintln!("Failed to load file: {}", e);
                    TextBuffer::new()
                }
            }
        } else {
            TextBuffer::new()
        };

        let mut plugin_manager = PluginManager::new();
        if let Err(e) = plugin_manager.load_all() {
            eprintln!("Plugin loading warning: {}", e);
        }

        let show_dashboard = file_path.is_none();
        let state = EditorState {
            show_dashboard,
            file_path: buffer.file_path().map(|p| p.to_path_buf()),
            cursor: Position { line: 1, col: 0 },
            show_line_numbers: cfg.show_line_numbers,
            show_relative_numbers: cfg.show_relative_numbers,
            wrap: cfg.wrap,
            ..EditorState::default()
        };

        Ok(Editor::new_shared(
            terminal,
            buffer,
            plugin_manager,
            state,
            keymap,
            cfg,
        ))
    }

    pub async fn run(&mut self) -> io::Result<()> {
        self.running = true;

        // Initialize the window layout to the actual terminal size.
        // Without this the layout stays at the hardcoded 24×80 default from
        // Engine::new() and content would be drawn for the wrong dimensions.
        {
            let rows = self.terminal.rows() as usize;
            let cols = self.terminal.cols() as usize;
            self.engine.update_window_layout(rows, cols);
        }

        self.update_review_viewport();
        if let Err(e) = self.renderer.render(
            &mut self.terminal,
            &self.engine.buffer,
            &self.engine.inactive_buffers,
            &self.engine.state,
            &self.engine.window_layout,
            &self.engine.highlights,
            self.engine.review_state.as_ref(),
        ) {
            self.engine
                .state
                .push_message(MessageLevel::Error, format!("Render error: {}", e));
        }
        self.needs_render = false;

        let poll_timeout = Duration::from_millis(150);

        while self.running {
            self.engine.state.dirty = self.engine.buffer.is_dirty();

            if event::poll(poll_timeout)? {
                match event::read() {
                    Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        self.engine.last_keypress_time = Instant::now();
                        let modifiers = key.modifiers;
                        self.handle_key(key.code, modifiers).await;
                    }
                    Ok(Event::Resize(_, _)) => {
                        self.terminal.update_size();
                        self.terminal.clear_cache();
                        let rows = self.terminal.rows() as usize;
                        let cols = self.terminal.cols() as usize;
                        self.engine.update_window_layout(rows, cols);
                        self.needs_render = true;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        self.engine
                            .state
                            .push_message(MessageLevel::Error, format!("Event read error: {}", e));
                    }
                }
            }

            #[cfg(feature = "syntax")]
            {
                let now = Instant::now();
                if self.engine.buffer.modification_count() > self.engine.last_highlight_mod_count
                    && now.duration_since(self.engine.last_keypress_time)
                        > Duration::from_millis(150)
                {
                    self.engine.highlights = self.engine.highlighter.update(
                        &self.engine.buffer.to_string(),
                        self.engine.state.file_path.as_deref(),
                    );
                    self.engine.last_highlight_mod_count = self.engine.buffer.modification_count();
                }
            }

            // ── Sync window cursor + auto-scroll ──
            let mod_count = self.engine.buffer.modification_count();
            let cursor_line = self.engine.state.cursor.line;
            // Skip expensive wrapped_rows computation when nothing changed.
            if self.needs_render
                || mod_count != self.last_scroll_mod_count
                || cursor_line != self.last_scroll_cursor_line
            {
                self.last_scroll_mod_count = mod_count;
                self.last_scroll_cursor_line = cursor_line;

                // Sync cursor for ALL windows, but only auto-scroll the focused one.
                let focused_idx = self.engine.window_layout.focused;
                for (i, w) in self.engine.window_layout.windows.iter_mut().enumerate() {
                    if i == focused_idx {
                        w.cursor = self.engine.state.cursor;
                    }
                }

                if let Some(w) = self.engine.window_layout.windows.get_mut(focused_idx) {
                    if self.viewport_override {
                        // An explicit scroll command (Ctrl-e/y, zz/zt/zb) owns
                        // the viewport for this frame.
                        self.viewport_override = false;
                    } else {
                        // Auto-scroll: keep cursor visible.
                        let show_num = self.engine.state.show_line_numbers
                            || self.engine.state.show_relative_numbers;
                        let line_num_width = if show_num {
                            self.engine.buffer.line_count().max(1).to_string().len()
                        } else {
                            0
                        };
                        let line_num_prefix = if show_num { line_num_width + 2 } else { 0 };
                        let window_rows = w.row_end.saturating_sub(w.row_start);
                        let window_cols = w.col_end.saturating_sub(w.col_start);
                        let content_width = window_cols.saturating_sub(line_num_prefix).max(1);
                        let tabstop = self.engine.state.tabstop;

                        let cursor_col = w.cursor.col;
                        let total_lines = self.engine.buffer.line_count();

                        let mut visual_offset = 0usize;
                        let end_scan = cursor_line.min(total_lines);
                        for line_num in w.scroll_top.max(1)..end_scan {
                            let line_text = self.engine.buffer.get_line(line_num);
                            visual_offset =
                                visual_offset.saturating_add(crate::renderer::wrapped_rows(
                                    &line_text,
                                    content_width,
                                    true,
                                    tabstop,
                                ));
                        }
                        if cursor_line >= w.scroll_top && cursor_line <= total_lines {
                            let line_text = self.engine.buffer.get_line(cursor_line);
                            let nchars = line_text.chars().count();
                            let nsegs = nchars.div_ceil(content_width).max(1);
                            let seg = (cursor_col / content_width).min(nsegs.saturating_sub(1));
                            visual_offset = visual_offset.saturating_add(seg);
                        }

                        // Use configured scrolloff, with a minimum of 1 to keep 1 line visible.
                        let scrolloff = self.engine.state.scrolloff;
                        let margin = if scrolloff > 0 {
                            scrolloff.min(window_rows.saturating_sub(1) / 2)
                        } else {
                            (window_rows / 3).max(1)
                        };

                        if cursor_line < w.scroll_top {
                            w.scroll_top = cursor_line.saturating_sub(margin).max(1);
                            self.needs_render = true;
                        } else if visual_offset >= window_rows.saturating_sub(1) {
                            let mut new_scroll = cursor_line;
                            let mut rows_needed = margin;
                            while new_scroll > 1 && rows_needed > 0 {
                                new_scroll -= 1;
                                let t = self.engine.buffer.get_line(new_scroll);
                                rows_needed = rows_needed.saturating_sub(
                                    crate::renderer::wrapped_rows(&t, content_width, true, tabstop),
                                );
                            }
                            w.scroll_top = new_scroll.max(1);
                            self.needs_render = true;
                        }
                    }
                }
            }

            if self.needs_render {
                self.update_review_viewport();
                if let Err(e) = self.renderer.render(
                    &mut self.terminal,
                    &self.engine.buffer,
                    &self.engine.inactive_buffers,
                    &self.engine.state,
                    &self.engine.window_layout,
                    &self.engine.highlights,
                    self.engine.review_state.as_ref(),
                ) {
                    self.engine
                        .state
                        .push_message(MessageLevel::Error, format!("Render error: {}", e));
                }
                self.needs_render = false;
            }
        }

        Ok(())
    }

    async fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Clear transient message on next key press (Vim-like behaviour).
        self.engine.state.clear_message();

        // ── Pending confirmation (y/n) ──
        // Must run before any mode-specific handler: the prompt is shown while
        // the editor is in Normal mode, where `y` would otherwise be parsed as
        // the yank operator.
        if self.engine.state.has_confirmation() {
            self.handle_confirmation(key).await;
            return;
        }

        // ── Dashboard mode intercept ──
        // If the dashboard is shown, intercept keys here before the keymap
        // handler gets involved.
        // Key not consumed (fall-through): dashboard was dismissed, so
        // continue to normal key processing below.
        if self.engine.state.show_dashboard && self.handle_dashboard_key(key) {
            return;
        }

        // ── Command-line mode ──
        // The command line is shared by every keymap: Vim enters it with `:`,
        // `/`, `?` and Emacs with `C-s`, `C-r`, `M-x`. Routing it here (rather
        // than inside VimKeymap) keeps Emacs from sending command-line keys to
        // the buffer.
        if self.engine.state.mode == Mode::Command {
            self.emit_key_event(key, modifiers);
            self.handle_command(key, modifiers).await;
            return;
        }

        let keymap_handler = Rc::clone(&self.keymap_handler);
        // Borrow the handler, call the method to get the future, then drop the
        // borrow before awaiting. The future borrows `self` (Editor) but not
        // the keymap handler, avoiding RefCell borrow issues across await.
        let future = {
            let handler = keymap_handler.borrow();
            handler.handle_key(self, key, modifiers)
        };
        future.await;
    }

    /// Emit a key-press event to the plugin system and record macros if recording.
    /// Called by both Vim and Emacs keymap handlers.
    pub(crate) fn emit_key_event(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        self.engine.emit_key_event(key, modifiers);
    }

    /// Expose count from operator state.
    pub fn take_count(&mut self) -> usize {
        self.engine.take_count()
    }

    // ── Methods that delegate directly to engine ────────────────

    pub fn undo(&mut self) {
        let result = self.engine.undo();
        self.clear_yank_chain();
        self.apply_result(result);
    }

    pub fn redo(&mut self) {
        let result = self.engine.redo();
        self.clear_yank_chain();
        self.apply_result(result);
    }

    /// A yank-pop cycle must not survive an undo/redo: the replaced text no
    /// longer matches what `yank-pop` would remove.
    fn clear_yank_chain(&mut self) {
        self.engine.cursor_state.yank_start = None;
        self.engine.cursor_state.yank_length = 0;
        self.engine.cursor_state.yank_text.clear();
    }

    pub fn show_file_info(&mut self) {
        let result = self.engine.show_file_info();
        self.apply_result(result);
    }

    pub async fn save_file_async(&mut self) {
        let result = self.engine.save_file_async().await;
        self.apply_result(result);
    }

    pub fn switch_keymap(&mut self, name: &str) {
        let keymap = match name {
            "vim" => Keymap::Vim,
            "emacs" => Keymap::Emacs,
            _ => {
                self.engine.state.push_message(
                    MessageLevel::Warning,
                    format!("Unknown keymap: {} (use vim or emacs)", name),
                );
                return;
            }
        };
        self.keymap_handler = create_keymap(keymap);
        self.current_keymap = keymap;
        self.engine.state.keymap = keymap;
        self.engine.reset_state_for_keymap_switch();

        // Keymap-local transient UI state must not leak into the next keymap.
        self.block_insert = None;
        self.one_shot_normal = false;
        self.search_preview = None;
        self.completion_session = None;
        self.history_index = None;
        self.history_draft.clear();
        self.engine.state.command_hint = None;

        self.needs_render = true;
    }

    /// Keep the diff panel height available to review navigation.
    ///
    /// One row is the status bar and two are the unified file header, so the
    /// navigation model works with a conservative visible height.
    fn update_review_viewport(&mut self) {
        if let Some(review) = &mut self.engine.review_state {
            review.viewport_height = self.terminal.rows().saturating_sub(3) as usize;
        }
    }

    /// Enter review mode with the given arguments.
    ///
    /// Saves the current keymap so it can be restored on exit, sets up mock
    /// diff data (Phase 1), and switches to the ReviewKeymap handler.
    pub(crate) fn enter_review_mode(&mut self, args: ReviewArgs) {
        // Never fabricate a diff: a mock review looks like real data and
        // silently misleads. Report why it failed and stay in the current mode.
        let state = match crate::review::diff::try_get_review_state(&args) {
            Ok(state) => state,
            Err(reason) => {
                self.engine
                    .state
                    .push_message(MessageLevel::Error, format!("review: {}", reason));
                self.needs_render = true;
                return;
            }
        };

        // Save the current keymap for restoration on exit.
        self.saved_keymap = Some(self.current_keymap);
        self.engine.review_state = Some(state);

        // Switch to review keymap
        self.keymap_handler = create_keymap(Keymap::Review);
        self.current_keymap = Keymap::Review;
        self.engine.state.keymap = Keymap::Review;
        self.engine.state.mode = Mode::Normal;
        self.needs_render = true;
    }

    /// Exit review mode and restore the previous keymap.
    pub(crate) fn exit_review_mode(&mut self) {
        let restore = self.saved_keymap.take().unwrap_or(Keymap::Vim);
        self.keymap_handler = create_keymap(restore);
        self.current_keymap = restore;
        self.engine.state.keymap = restore;
        self.engine.review_state = None;
        self.engine.reset_state_for_keymap_switch();
        self.needs_render = true;
    }

    // ── Test helpers ────────────────────────────────────────────

    pub fn set_buffer_for_test(&mut self, text: &str) {
        self.engine.set_buffer_for_test(text);
    }

    pub fn snapshot_for_test(&self) -> (String, usize, usize, Mode) {
        self.engine.snapshot_for_test()
    }

    pub async fn handle_key_for_test(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        self.handle_key(key, modifiers).await;
    }

    /// Expose engine's kill_ring for tests
    pub fn kill_ring(&self) -> &crate::types::KillRing {
        &self.engine.kill_ring
    }

    pub fn kill_ring_mut(&mut self) -> &mut crate::types::KillRing {
        &mut self.engine.kill_ring
    }
}

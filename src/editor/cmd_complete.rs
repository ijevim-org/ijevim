use crate::editor::{CompletionSession, Editor, SearchPreview};
use crate::types::SearchDirection;
use std::path::PathBuf;

/// Names accepted by `:` (used only for Tab completion).
pub(crate) const COMMAND_NAMES: &[&str] = &[
    "q",
    "q!",
    "qa",
    "w",
    "w!",
    "wa",
    "wq",
    "wq!",
    "wqa",
    "e",
    "e!",
    "Ex",
    "Explore",
    "ls",
    "buffers",
    "bn",
    "bnext",
    "bp",
    "bprevious",
    "bprev",
    "bd",
    "bdelete",
    "sp",
    "split",
    "vs",
    "vsplit",
    "set",
    "messages",
    "nohlsearch",
    "noh",
    "review",
    "branch",
    "git",
    "keymap",
];

const SET_OPTIONS: &[&str] = &[
    "number",
    "nonumber",
    "relativenumber",
    "norelativenumber",
    "hlsearch",
    "nohlsearch",
    "expandtab",
    "noexpandtab",
    "tabstop=",
    "shiftwidth=",
    "scrolloff=",
    "incsearch",
    "noincsearch",
];

const KEYMAPS: &[&str] = &["vim", "emacs", "review"];

const PATH_COMMANDS: &[&str] = &[
    "e", "e!", "w", "w!", "wq", "wq!", "sp", "split", "vs", "vsplit", "Explore", "Ex",
];

impl Editor {
    pub(crate) fn is_search_line(buffer: &str) -> bool {
        buffer.starts_with('/') || buffer.starts_with('?')
    }

    /// Reset per-invocation command-line state when entering Command mode.
    pub(crate) fn reset_command_line_nav(&mut self) {
        self.history_index = None;
        self.history_draft.clear();
        self.completion_session = None;
        self.search_preview = None;
        self.engine.state.command_hint = None;
        self.engine.state.incsearch_match = None;
    }

    /// Start an incremental-search preview for `/` or `?`.
    pub(crate) fn begin_search_command(&mut self, direction: SearchDirection) {
        self.reset_command_line_nav();
        self.search_preview = Some(SearchPreview {
            origin: self.engine.state.cursor,
            direction,
            pattern: String::new(),
            prev_pattern: self.engine.state.search_pattern.clone(),
            prev_search: self.engine.search_state.clone(),
            results: Vec::new(),
            current_idx: 0,
        });
    }

    /// Push a submitted command line into the correct history list.
    pub(crate) fn push_command_history(&mut self, buffer: &str) {
        if buffer.is_empty() {
            return;
        }
        if Self::is_search_line(buffer) {
            let pattern = buffer[1..].to_string();
            if pattern.is_empty() {
                return;
            }
            if self.search_history.last() == Some(&pattern) {
                return;
            }
            self.search_history.push(pattern);
            if self.search_history.len() > 100 {
                self.search_history.remove(0);
            }
        } else {
            if self.command_history.last() == Some(&buffer.to_string()) {
                return;
            }
            self.command_history.push(buffer.to_string());
            if self.command_history.len() > 100 {
                self.command_history.remove(0);
            }
        }
    }

    /// Move to the previous history entry (Up / Ctrl-p).
    pub(crate) fn history_prev(&mut self) -> bool {
        let search = Self::is_search_line(&self.engine.state.command_buffer);
        let list = if search {
            self.search_history.clone()
        } else {
            self.command_history.clone()
        };
        if list.is_empty() {
            return false;
        }
        let index = match self.history_index {
            Some(i) => i,
            None => {
                self.history_draft = self.engine.state.command_buffer.clone();
                list.len()
            }
        };
        if index == 0 {
            return false;
        }
        let new_index = index - 1;
        let entry = list[new_index].clone();
        let prefix = if search {
            self.engine
                .state
                .command_buffer
                .chars()
                .next()
                .unwrap_or('/')
        } else {
            '\0'
        };
        self.engine.state.command_buffer = if search {
            format!("{}{}", prefix, entry)
        } else {
            entry
        };
        self.engine.state.command_cursor_pos = self.engine.state.command_buffer.len();
        self.history_index = Some(new_index);
        self.completion_session = None;
        self.update_search_preview();
        true
    }

    /// Move to the next history entry, restoring the draft at the end.
    pub(crate) fn history_next(&mut self) -> bool {
        let Some(index) = self.history_index else {
            return false;
        };
        let search = Self::is_search_line(&self.engine.state.command_buffer);
        let list = if search {
            self.search_history.clone()
        } else {
            self.command_history.clone()
        };
        if index + 1 < list.len() {
            let new_index = index + 1;
            let entry = list[new_index].clone();
            let prefix = if search {
                self.engine
                    .state
                    .command_buffer
                    .chars()
                    .next()
                    .unwrap_or('/')
            } else {
                '\0'
            };
            self.engine.state.command_buffer = if search {
                format!("{}{}", prefix, entry)
            } else {
                entry
            };
            self.history_index = Some(new_index);
        } else {
            self.engine.state.command_buffer = self.history_draft.clone();
            self.history_index = None;
        }
        self.engine.state.command_cursor_pos = self.engine.state.command_buffer.len();
        self.completion_session = None;
        self.update_search_preview();
        true
    }

    /// Tab-complete the token before the command-line cursor.
    pub(crate) fn complete_command_line(&mut self) {
        if self.completion_session.is_some() {
            self.cycle_completion();
            return;
        }

        let buffer = self.engine.state.command_buffer.clone();
        let pos = self.engine.state.command_cursor_pos.min(buffer.len());
        let before = &buffer[..pos];
        let start = before
            .rfind(char::is_whitespace)
            .map(|i| i + before[i..].chars().next().map_or(1, |c| c.len_utf8()))
            .unwrap_or(0);
        let prefix = &before[start..];
        let head = &before[..start];

        let mut candidates = self.completion_candidates(head, prefix);
        candidates.sort();
        candidates.dedup();
        if candidates.is_empty() {
            self.engine.state.command_hint = Some("No matches".to_string());
            self.completion_session = None;
            self.needs_render = true;
            return;
        }

        self.completion_session = Some(CompletionSession {
            start,
            candidates: candidates.clone(),
            index: 0,
        });
        self.apply_completion(0);
    }

    fn cycle_completion(&mut self) {
        let Some(session) = &self.completion_session else {
            return;
        };
        if session.candidates.is_empty() {
            self.completion_session = None;
            return;
        }
        let next = (session.index + 1) % session.candidates.len();
        self.apply_completion(next);
    }

    fn apply_completion(&mut self, index: usize) {
        let Some(session) = &self.completion_session else {
            return;
        };
        let Some(candidate) = session.candidates.get(index).cloned() else {
            return;
        };
        let start = session.start;
        let candidates_len = session.candidates.len();
        self.engine.state.command_buffer.truncate(start);
        self.engine.state.command_buffer.push_str(&candidate);
        self.engine.state.command_cursor_pos = self.engine.state.command_buffer.len();
        if let Some(session) = &mut self.completion_session {
            session.index = index;
        }
        self.engine.state.command_hint =
            Some(format!("[{}/{}] {}", index + 1, candidates_len, candidate));
        self.needs_render = true;
    }

    fn completion_candidates(&self, head: &str, prefix: &str) -> Vec<String> {
        let buffer = &self.engine.state.command_buffer;
        if buffer.starts_with('/') || buffer.starts_with('?') {
            // Vim has no command-name completion for search patterns.
            return Vec::new();
        }

        let head = head.trim_start_matches(':');
        let command = head.split_whitespace().next().unwrap_or("");
        if head.is_empty() {
            return COMMAND_NAMES
                .iter()
                .filter(|c| c.starts_with(prefix))
                .map(|c| c.to_string())
                .collect();
        }

        // `:set <option>` / `:keymap <name>`.
        let args: Vec<&str> = head.split_whitespace().collect();
        if command == "set" && args.len() == 1 {
            return SET_OPTIONS
                .iter()
                .filter(|o| o.starts_with(prefix))
                .map(|o| o.to_string())
                .collect();
        }
        if command == "keymap" && args.len() == 1 {
            return KEYMAPS
                .iter()
                .filter(|o| o.starts_with(prefix))
                .map(|o| o.to_string())
                .collect();
        }

        if PATH_COMMANDS.contains(&command) {
            return path_candidates(prefix);
        }

        Vec::new()
    }

    /// Update the live preview after every command-line edit.
    pub(crate) fn update_search_preview(&mut self) {
        if !self.config.incsearch {
            return;
        }
        let buffer = self.engine.state.command_buffer.clone();
        if !Self::is_search_line(&buffer) {
            return;
        }
        let direction = if buffer.starts_with('?') {
            SearchDirection::Backward
        } else {
            SearchDirection::Forward
        };
        if self.search_preview.is_none() {
            let origin = self.engine.state.cursor;
            self.search_preview = Some(SearchPreview {
                origin,
                direction,
                pattern: String::new(),
                prev_pattern: self.engine.state.search_pattern.clone(),
                prev_search: self.engine.search_state.clone(),
                results: Vec::new(),
                current_idx: 0,
            });
        }

        let pattern = buffer[1..].to_string();
        let Some(preview) = &mut self.search_preview else {
            return;
        };
        preview.pattern = pattern.clone();
        preview.direction = direction;

        if pattern.is_empty() {
            self.engine.state.cursor = preview.origin;
            self.engine.state.incsearch_match = None;
            self.engine.state.search_pattern = preview.prev_pattern.clone();
            self.engine.state.command_hint = None;
            self.needs_render = true;
            return;
        }

        let results = self.engine.buffer.search(&pattern);
        let origin = preview.origin;
        let idx = if results.is_empty() {
            0
        } else if direction == SearchDirection::Forward {
            results
                .iter()
                .position(|r| {
                    r.line > origin.line || (r.line == origin.line && r.start_col > origin.col)
                })
                .unwrap_or(0)
        } else {
            results
                .iter()
                .rposition(|r| {
                    r.line < origin.line || (r.line == origin.line && r.start_col < origin.col)
                })
                .unwrap_or(results.len() - 1)
        };

        preview.current_idx = idx;
        preview.results = results;
        let Some(result) = preview.results.get(idx).cloned() else {
            self.engine.state.cursor = origin;
            self.engine.state.incsearch_match = None;
            self.engine.state.search_pattern = Some(pattern);
            self.engine.state.command_hint = Some("Pattern not found".to_string());
            self.needs_render = true;
            return;
        };

        let len = pattern.chars().count().max(1);
        self.engine.state.cursor.line = result.line;
        self.engine.state.cursor.col = result.start_col;
        self.engine.state.incsearch_match =
            Some((result.line, result.start_col, result.start_col + len));
        self.engine.state.search_pattern = Some(pattern);
        self.engine.state.command_hint = Some(format!("[{}/{}]", idx + 1, preview.results.len()));
        self.needs_render = true;
    }

    /// Accept the previewed match. Returns true when a preview was committed.
    pub(crate) fn commit_search_preview(&mut self) -> bool {
        let Some(preview) = self.search_preview.take() else {
            return false;
        };
        self.engine.state.incsearch_match = None;
        self.engine.state.command_hint = None;
        if preview.pattern.is_empty() {
            // An empty pattern repeats the previous search, like Vim.
            let query = preview.prev_search.query.clone();
            if query.is_empty() {
                return true;
            }
            self.engine.state.cursor = preview.origin;
            self.do_search(&query, preview.direction);
            return true;
        }
        self.engine.search_state.query = preview.pattern.clone();
        self.engine.search_state.direction = preview.direction;
        self.engine.search_state.results = preview.results;
        self.engine.search_state.current_idx = preview.current_idx;
        self.engine.state.search_pattern = Some(preview.pattern);
        self.needs_render = true;
        true
    }

    /// Cancel the preview and restore the cursor / previous search.
    pub(crate) fn cancel_search_preview(&mut self) {
        let Some(preview) = self.search_preview.take() else {
            return;
        };
        self.engine.state.cursor = preview.origin;
        self.engine.state.search_pattern = preview.prev_pattern;
        self.engine.search_state = preview.prev_search;
        self.engine.state.incsearch_match = None;
        self.engine.state.command_hint = None;
        self.needs_render = true;
    }

}

fn path_candidates(prefix: &str) -> Vec<String> {
    let (dir_display, file_prefix) = match prefix.rfind('/') {
        Some(i) => (&prefix[..=i], &prefix[i + 1..]),
        None => ("", prefix),
    };
    let fs_dir = if dir_display.is_empty() {
        ".".to_string()
    } else {
        expand_tilde(dir_display)
    };
    let Ok(entries) = std::fs::read_dir(&fs_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(file_prefix) {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let suffix = if is_dir { "/" } else { "" };
        out.push(format!("{}{}{}", dir_display, name, suffix));
    }
    out
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            let mut p = PathBuf::from(home);
            p.push(rest);
            return p.to_string_lossy().to_string();
        }
    } else if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

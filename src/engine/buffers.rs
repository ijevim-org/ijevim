use crate::buffer::TextBuffer;
use crate::engine::Engine;
use crate::types::{DirEntry, Position};
use crate::undo::UndoManager;
use std::path::PathBuf;

impl Engine {
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
}

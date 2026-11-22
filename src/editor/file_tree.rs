//! VSCode-style file-tree sidebar shown at the left edge.
//!
//! The sidebar owns a flattened list of rows built from the filesystem.
//! Directories expand in place; the current file is highlighted. Opening a
//! file from the tree uses the same `buffer_open` path as `:e`.

use crate::editor::Editor;
use crate::types::{FileTreeRow, FileTreeState, MessageLevel};
use crossterm::event::KeyCode;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Entries never shown in the explorer. `.git` is noise; commit history is
/// available through review mode instead.
const SKIPPED_NAMES: &[&str] = &[".git"];

/// Guard against symlink loops when a user expands a recursive link.
const MAX_TREE_DEPTH: usize = 64;

impl Editor {
    /// Build the initial sidebar state.
    ///
    /// `enabled` comes from the config; `no_file` means the editor was started
    /// without a file, in which case the explorer takes keyboard focus.
    pub(crate) fn init_file_tree(&mut self, enabled: bool, no_file: bool) {
        let file_path = self.engine.state.file_path.clone();
        let root = file_path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let current_file = file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|name| root.join(name));

        {
            let tree = &mut self.engine.state.file_tree;
            tree.root = root;
            tree.current_file = current_file;
            tree.visible = enabled;
            tree.focused = enabled && no_file;
            tree.expanded.clear();
            tree.rows.clear();
            tree.selected = 0;
            tree.scroll_top = 0;
            tree.error = None;
        }

        if let Some(path) = file_path {
            self.tree_expand_ancestors(&path);
        }
        self.rebuild_file_tree();
        self.sync_layout_inset();
        self.needs_render = true;
    }

    /// Reserve (or release) the leftmost columns for the sidebar.
    pub(crate) fn sync_layout_inset(&mut self) {
        let cols = self.terminal.cols() as usize;
        let width = if self.engine.state.file_tree.visible {
            FileTreeState::width_for(cols)
        } else {
            0
        };
        // A terminal too narrow to show the sidebar must not keep keyboard
        // focus trapped in an invisible panel.
        if width == 0 {
            self.engine.state.file_tree.focused = false;
        }
        self.engine.window_layout.set_inset_left(width, cols);
    }

    /// Toggle sidebar visibility (F2).
    pub(crate) fn toggle_file_tree(&mut self) {
        let now_visible = {
            let tree = &mut self.engine.state.file_tree;
            tree.visible = !tree.visible;
            if !tree.visible {
                tree.focused = false;
            }
            tree.visible
        };

        if now_visible && self.engine.state.file_tree.rows.is_empty() {
            self.rebuild_file_tree();
        }
        self.sync_layout_inset();
        self.needs_render = true;
    }

    /// Move keyboard focus into the sidebar (F3), or back to the editor when
    /// already focused.
    pub(crate) fn toggle_file_tree_focus(&mut self) {
        if self.engine.state.file_tree.focused {
            self.engine.state.file_tree.focused = false;
            self.needs_render = true;
        } else {
            self.focus_file_tree();
        }
    }

    /// Show the sidebar and move keyboard focus into it.
    pub(crate) fn focus_file_tree(&mut self) {
        let needs_rows = {
            let tree = &mut self.engine.state.file_tree;
            tree.visible = true;
            tree.focused = true;
            tree.rows.is_empty()
        };
        if needs_rows {
            self.rebuild_file_tree();
        }
        self.sync_layout_inset();
        self.tree_ensure_visible();
        self.needs_render = true;
    }

    /// Point the explorer at `path` and focus it.
    ///
    /// Used by `:Ex` and `:e <directory>`, which previously opened a directory
    /// listing buffer.
    pub(crate) fn set_file_tree_root(&mut self, path: &Path) {
        {
            let tree = &mut self.engine.state.file_tree;
            tree.root = path.to_path_buf();
            tree.expanded.clear();
            tree.rows.clear();
            tree.selected = 0;
            tree.scroll_top = 0;
            tree.error = None;
            tree.visible = true;
            tree.focused = true;
        }
        self.rebuild_file_tree();
        self.sync_layout_inset();
        self.needs_render = true;
    }

    /// Handle a key while the sidebar has focus. Returns `true` when consumed.
    ///
    /// Anything the tree does not own drops focus so the key can reach the
    /// editor keymap. Without this fall-through, starting without a file would
    /// leave every key (including `i` and `:`) trapped in the sidebar.
    pub(crate) fn handle_file_tree_key(&mut self, key: KeyCode) -> bool {
        let consumed = match key {
            KeyCode::Char('j') | KeyCode::Down => {
                self.tree_move(1);
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tree_move(-1);
                true
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.tree_collapse();
                true
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.tree_expand_selected();
                true
            }
            KeyCode::Enter | KeyCode::Char('o') => {
                self.tree_open_selected();
                true
            }
            KeyCode::Char('g') => {
                self.tree_select(0);
                true
            }
            KeyCode::Char('G') => {
                let last = self.engine.state.file_tree.rows.len().saturating_sub(1);
                self.tree_select(last);
                true
            }
            KeyCode::Char('r') => {
                self.rebuild_file_tree();
                self.tree_ensure_visible();
                true
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.engine.state.file_tree.focused = false;
                true
            }
            _ => false,
        };
        if !consumed {
            self.engine.state.file_tree.focused = false;
        }
        self.needs_render = true;
        consumed
    }

    // ── Tree model ───────────────────────────────────────────────

    fn tree_select(&mut self, index: usize) {
        let len = self.engine.state.file_tree.rows.len();
        if len == 0 {
            return;
        }
        self.engine.state.file_tree.selected = index.min(len - 1);
        self.tree_ensure_visible();
    }

    fn tree_move(&mut self, delta: isize) {
        let len = self.engine.state.file_tree.rows.len();
        if len == 0 {
            return;
        }
        let current = self.engine.state.file_tree.selected as isize;
        let next = (current + delta).clamp(0, (len - 1) as isize) as usize;
        self.engine.state.file_tree.selected = next;
        self.tree_ensure_visible();
    }

    fn tree_selected_row(&self) -> Option<FileTreeRow> {
        self.engine
            .state
            .file_tree
            .rows
            .get(self.engine.state.file_tree.selected)
            .cloned()
    }

    fn tree_ensure_visible(&mut self) {
        let height = self.tree_list_height();
        let len = self.engine.state.file_tree.rows.len();
        let tree = &mut self.engine.state.file_tree;
        if len == 0 {
            tree.scroll_top = 0;
            return;
        }
        if tree.selected < tree.scroll_top {
            tree.scroll_top = tree.selected;
        } else if tree.selected >= tree.scroll_top.saturating_add(height) {
            tree.scroll_top = tree.selected + 1 - height;
        }
        tree.scroll_top = tree.scroll_top.min(len.saturating_sub(height));
    }

    /// Rows available below the header/root lines and above the status bar.
    fn tree_list_height(&self) -> usize {
        (self.terminal.rows() as usize)
            .saturating_sub(1) // status bar
            .saturating_sub(2) // EXPLORER + root name
            .max(1)
    }

    fn tree_open_selected(&mut self) {
        let Some(row) = self.tree_selected_row() else {
            return;
        };
        if row.is_dir {
            self.tree_toggle_dir(&row.path);
            return;
        }
        let path = row.path.to_string_lossy().to_string();
        match self.engine.buffer_open(&path) {
            Ok(()) => {
                self.engine.state.file_tree.focused = false;
                self.engine.state.file_tree.current_file = Some(row.path.clone());
            }
            Err(e) => self.engine.state.push_message(MessageLevel::Error, e),
        }
        self.needs_render = true;
    }

    fn tree_expand_selected(&mut self) {
        let Some(row) = self.tree_selected_row() else {
            return;
        };
        if row.is_dir && !row.expanded {
            self.tree_toggle_dir(&row.path);
        }
    }

    /// Collapse an expanded directory, or move up to the parent row.
    fn tree_collapse(&mut self) {
        let Some(row) = self.tree_selected_row() else {
            return;
        };
        if row.is_dir && row.expanded {
            self.tree_toggle_dir(&row.path);
            return;
        }
        if row.depth == 0 {
            return;
        }
        let selected = self.engine.state.file_tree.selected;
        let parent = self.engine.state.file_tree.rows[..selected]
            .iter()
            .rposition(|r| r.depth + 1 == row.depth && r.is_dir);
        if let Some(idx) = parent {
            self.tree_select(idx);
        }
    }

    fn tree_toggle_dir(&mut self, path: &Path) {
        {
            let tree = &mut self.engine.state.file_tree;
            if let Some(pos) = tree.expanded.iter().position(|p| p == path) {
                tree.expanded.remove(pos);
            } else {
                tree.expanded.push(path.to_path_buf());
            }
        }
        self.rebuild_file_tree();
    }

    fn rebuild_file_tree(&mut self) {
        let mut root = self.engine.state.file_tree.root.clone();
        if root.as_os_str().is_empty() {
            root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            self.engine.state.file_tree.root = root.clone();
        }
        let expanded = self.engine.state.file_tree.expanded.clone();
        let selected_path = self.tree_selected_row().map(|r| r.path);
        let previous_selected = self.engine.state.file_tree.selected;

        let (rows, error) = build_tree_rows(&root, &expanded);
        let selected = selected_path
            .as_ref()
            .and_then(|p| rows.iter().position(|r| &r.path == p))
            .unwrap_or_else(|| previous_selected.min(rows.len().saturating_sub(1)));
        let height = self.tree_list_height();

        let tree = &mut self.engine.state.file_tree;
        tree.rows = rows;
        tree.error = error;
        tree.selected = selected;
        // Clamp the scroll position to the rebuilt row count.
        tree.scroll_top = tree.scroll_top.min(tree.rows.len().saturating_sub(height));
        self.tree_ensure_visible();
    }

    /// Expand every ancestor of `path` below the tree root so a file opened
    /// from the command line is visible in the explorer.
    fn tree_expand_ancestors(&mut self, path: &Path) {
        let root = self.engine.state.file_tree.root.clone();
        let mut ancestors = Vec::new();
        let mut current = path.parent();
        while let Some(dir) = current {
            if dir == root || !dir.starts_with(&root) {
                break;
            }
            ancestors.push(dir.to_path_buf());
            current = dir.parent();
        }
        self.engine.state.file_tree.expanded.extend(ancestors);
    }
}

/// Read the tree under `root`, descending only into expanded directories.
fn build_tree_rows(root: &Path, expanded: &[PathBuf]) -> (Vec<FileTreeRow>, Option<String>) {
    let mut rows = Vec::new();
    let mut error = None;
    let mut visited = HashSet::new();
    walk_tree(root, 0, expanded, &mut visited, &mut rows, &mut error);
    (rows, error)
}

fn walk_tree(
    dir: &Path,
    depth: usize,
    expanded: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    rows: &mut Vec<FileTreeRow>,
    error: &mut Option<String>,
) {
    if depth > MAX_TREE_DEPTH {
        return;
    }
    let read_dir = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(e) => {
            if error.is_none() {
                *error = Some(format!("Cannot read '{}': {}", dir.display(), e));
            }
            return;
        }
    };

    // An expanded symlink back into an ancestor would otherwise duplicate the
    // whole subtree once per depth level. Canonical paths catch that even when
    // the link and the ancestor are spelled differently.
    if let Ok(canonical) = std::fs::canonicalize(dir) {
        if !visited.insert(canonical) {
            return;
        }
    }

    // `metadata()` follows symlinks (a link to a directory is shown as one).
    // Resolve it once per entry instead of once per sort comparison.
    let mut entries: Vec<_> = read_dir
        .filter_map(|e| e.ok())
        .map(|e| {
            let is_dir = e.metadata().map(|m| m.is_dir()).unwrap_or(false);
            (e, is_dir)
        })
        .collect();
    entries.sort_by(|(a, a_dir), (b, b_dir)| {
        if a_dir != b_dir {
            b_dir.cmp(a_dir)
        } else {
            a.file_name().cmp(&b.file_name())
        }
    });

    for (entry, is_dir) in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if SKIPPED_NAMES.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        let is_expanded = is_dir && expanded.iter().any(|p| p == &path);
        rows.push(FileTreeRow {
            name,
            path: path.clone(),
            depth,
            is_dir,
            expanded: is_expanded,
        });
        if is_expanded {
            walk_tree(&path, depth + 1, expanded, visited, rows, error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Keymap;

    #[test]
    fn builds_tree_directories_first_and_expands_only_requested_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir(root.join("dir")).unwrap();
        std::fs::create_dir(root.join("dir").join("nested")).unwrap();
        std::fs::write(root.join("b.txt"), "b").unwrap();
        std::fs::write(root.join("a.txt"), "a").unwrap();
        std::fs::write(root.join("dir").join("inner.txt"), "i").unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();

        let (rows, error) = build_tree_rows(root, &[]);
        assert!(error.is_none());
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["dir", "a.txt", "b.txt"]);
        assert!(!rows.iter().any(|r| r.name == ".git"));

        let (rows, _) = build_tree_rows(root, &[root.join("dir")]);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["dir", "nested", "inner.txt", "a.txt", "b.txt"]);
        assert!(rows[0].expanded);
        assert_eq!(rows[2].depth, 1);
    }

    #[test]
    fn sidebar_width_hides_on_narrow_terminals() {
        assert_eq!(FileTreeState::width_for(40), 0);
        assert_eq!(FileTreeState::width_for(60), 20);
        assert_eq!(FileTreeState::width_for(200), 36);
    }

    #[cfg(unix)]
    #[test]
    fn expanded_symlink_loop_does_not_duplicate_the_tree() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(root.join("a.txt"), "a").unwrap();
        symlink(root, sub.join("loop")).unwrap();

        let (rows, _) = build_tree_rows(root, &[sub.clone(), sub.join("loop")]);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["sub", "loop", "a.txt"]);
    }

    #[test]
    fn sync_layout_inset_tracks_explorer_visibility() {
        let mut ed = Editor::new_headless_for_test(Keymap::Vim).unwrap();
        let width = FileTreeState::width_for(ed.terminal.cols() as usize);

        ed.engine.state.file_tree.visible = true;
        ed.sync_layout_inset();
        assert_eq!(ed.engine.window_layout.inset_left, width);

        ed.engine.state.file_tree.visible = false;
        ed.sync_layout_inset();
        assert_eq!(ed.engine.window_layout.inset_left, 0);
    }
}

use crate::buffer::TextBuffer;
use crate::types::Position;

#[derive(Clone, Debug)]
pub enum EditType {
    Insert {
        line: usize,
        col: usize,
        text: String,
    },
    Delete {
        line: usize,
        col: usize,
        text: String,
    },
    InsertLine {
        line: usize,
        text: String,
    },
    DeleteLine {
        line: usize,
        text: String,
    },
    Replace {
        line: usize,
        col: usize,
        old_text: String,
        new_text: String,
    },
    Merge {
        line: usize,
        deleted_newline_col: usize,
    },
    Split {
        line: usize,
        col: usize,
    },
    /// A group of edits that are undone/redone atomically (e.g., Replace mode).
    Group {
        edits: Vec<Edit>,
    },
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub edit_type: EditType,
    pub cursor_before: Position,
    pub cursor_after: Position,
    pub modification_count: usize,
}

/// A node in the undo tree.
#[derive(Clone, Debug)]
struct UndoNode {
    edit: Edit,
    parent: Option<usize>,
    children: Vec<usize>,
    /// Set by [`UndoManager::remove_node`]. Detached nodes stay in `nodes` to
    /// keep the remaining indices stable, but are excluded from the tree and
    /// from the size accounting.
    removed: bool,
}

/// Tree-based undo manager.
///
/// Unlike the linear stack approach, this maintains a tree of edits.
/// When a new edit is pushed after undoing, it creates a new branch
/// instead of discarding the redo history.
pub struct UndoManager {
    nodes: Vec<UndoNode>,
    current: usize,
    max_size: usize,
}

impl UndoManager {
    pub fn new() -> Self {
        // Create a dummy root node (empty edit) so we always have a valid current node.
        let root = UndoNode {
            edit: Edit {
                edit_type: EditType::Group { edits: Vec::new() },
                cursor_before: Position { line: 1, col: 0 },
                cursor_after: Position { line: 1, col: 0 },
                modification_count: 0,
            },
            parent: None,
            children: Vec::new(),
            removed: false,
        };
        Self {
            nodes: vec![root],
            current: 0,
            max_size: 1000,
        }
    }

    /// Push a new edit onto the undo tree.
    ///
    /// If we're at a leaf node, just add a child.
    /// If we're in the middle of a branch (after undo), create a new branch.
    pub fn push(&mut self, edit: Edit) {
        let new_node = UndoNode {
            edit,
            parent: Some(self.current),
            children: Vec::new(),
            removed: false,
        };
        let new_idx = self.nodes.len();
        self.nodes.push(new_node);
        self.nodes[self.current].children.push(new_idx);
        self.current = new_idx;

        // Evict oldest nodes if at capacity. Only live nodes count towards the
        // limit; detached nodes remain in the vector for index stability.
        while self.live_node_count() > self.max_size {
            if !self.evict_oldest() {
                break;
            }
        }
    }

    /// Push an insertion, merging it into the previous undo node when both are
    /// contiguous single-line insertions with no undo boundary between them.
    ///
    /// Emacs groups consecutive self-inserting characters into one undo unit.
    /// Coalescing is only attempted when the current node is a leaf, so typing
    /// after an undo creates a new branch instead of rewriting history.
    pub fn push_insert_coalescing(&mut self, edit: Edit) {
        let current = self.current;
        let joins = self.nodes[current].children.is_empty()
            && match (&self.nodes[current].edit.edit_type, &edit.edit_type) {
                (
                    EditType::Insert { line, col, text },
                    EditType::Insert {
                        line: new_line,
                        col: new_col,
                        text: new_text,
                    },
                ) => {
                    !new_text.contains('\n')
                        && *new_line == *line
                        && *new_col == *col + text.chars().count()
                }
                _ => false,
            };
        if joins {
            if let (EditType::Insert { text, .. }, EditType::Insert { text: new_text, .. }) =
                (&mut self.nodes[current].edit.edit_type, &edit.edit_type)
            {
                text.push_str(new_text);
            }
            self.nodes[current].edit.cursor_after = edit.cursor_after;
            self.nodes[current].edit.modification_count = edit.modification_count;
            return;
        }
        self.push(edit);
    }

    /// Number of nodes still attached to the tree.
    fn live_node_count(&self) -> usize {
        self.nodes.iter().filter(|n| !n.removed).count()
    }

    /// Remove the oldest leaf node to stay within `max_size`.
    ///
    /// The root is never removed, nor is the current node. Returns `false` when
    /// no eviction candidate exists (e.g. a single linear chain whose only leaf
    /// is the current node).
    fn evict_oldest(&mut self) -> bool {
        for i in 1..self.nodes.len() {
            if !self.nodes[i].removed && self.nodes[i].children.is_empty() && i != self.current {
                self.remove_node(i);
                return true;
            }
        }
        false
    }

    /// Detach a node from the tree, reconnecting its children to its parent.
    ///
    /// The node itself stays in `nodes` so existing indices remain valid, but
    /// it is flagged `removed` and unlinked from its parent.
    fn remove_node(&mut self, idx: usize) {
        if idx >= self.nodes.len() || self.nodes[idx].removed {
            return;
        }
        let parent = self.nodes[idx].parent;
        let children = std::mem::take(&mut self.nodes[idx].children);
        if let Some(p) = parent {
            self.nodes[p].children.retain(|&c| c != idx);
            for c in children {
                self.nodes[c].parent = Some(p);
                self.nodes[p].children.push(c);
            }
        }
        self.nodes[idx].parent = None;
        self.nodes[idx].removed = true;
    }

    /// Undo the current edit.
    pub fn undo(&mut self, buffer: &mut TextBuffer) -> Option<Edit> {
        if self.current == 0 {
            return None; // At root, nothing to undo
        }
        let edit = self.nodes[self.current].edit.clone();
        self.apply_undo(buffer, &edit);
        // Move to parent
        if let Some(parent) = self.nodes[self.current].parent {
            self.current = parent;
            // Reset modification count
            buffer.reset_modification_count(edit.modification_count);
        }
        Some(edit)
    }

    /// Redo the next edit.
    pub fn redo(&mut self, buffer: &mut TextBuffer) -> Option<Edit> {
        if self.nodes[self.current].children.is_empty() {
            return None; // No redo available
        }
        // Take the first child (most recent branch)
        let next = self.nodes[self.current].children[0];
        let edit = self.nodes[next].edit.clone();
        self.apply_redo(buffer, &edit);
        self.current = next;
        buffer.reset_modification_count(edit.modification_count);
        Some(edit)
    }

    /// Switch to a specific branch by node index (for `g-` / `g+`).
    pub fn switch_branch(&mut self, buffer: &mut TextBuffer, target: usize) -> Option<Edit> {
        if target >= self.nodes.len() || target == self.current {
            return None;
        }
        let edit = self.nodes[target].edit.clone();
        // For simplicity, we only support moving to direct children or parent.
        // Full tree traversal would require finding the path.
        if target == self.current {
            return None;
        }
        // Check if target is parent
        if self.nodes[self.current].parent == Some(target) {
            self.apply_undo(buffer, &edit);
            self.current = target;
            buffer.reset_modification_count(edit.modification_count);
            return Some(edit);
        }
        // Check if target is a child
        if self.nodes[self.current].children.contains(&target) {
            self.apply_redo(buffer, &edit);
            self.current = target;
            buffer.reset_modification_count(edit.modification_count);
            return Some(edit);
        }
        None
    }

    /// Get the parent node index (for `g-`).
    pub fn parent_node(&self) -> Option<usize> {
        self.nodes[self.current].parent
    }

    /// Get the first child node index (for `g+`).
    pub fn first_child_node(&self) -> Option<usize> {
        self.nodes[self.current].children.first().copied()
    }

    /// Clear all history (used when switching buffers).
    pub fn clear(&mut self) {
        self.nodes = vec![UndoNode {
            edit: Edit {
                edit_type: EditType::Group { edits: Vec::new() },
                cursor_before: Position { line: 1, col: 0 },
                cursor_after: Position { line: 1, col: 0 },
                modification_count: 0,
            },
            parent: None,
            children: Vec::new(),
            removed: false,
        }];
        self.current = 0;
    }

    fn apply_undo(&self, buffer: &mut TextBuffer, edit: &Edit) {
        match &edit.edit_type {
            EditType::Insert { line, col, text } => {
                let start = buffer.line_to_char(line - 1) + col;
                let end = start + text.chars().count();
                buffer.remove_range(start, end);
            }
            EditType::Delete { line, col, text } => {
                buffer.insert(*line, *col, text);
            }
            EditType::InsertLine { line, text: _ } => {
                buffer.delete_line(*line);
            }
            EditType::DeleteLine { line, text } => {
                if buffer.to_string() == "\n" {
                    buffer.replace_with(text);
                } else {
                    buffer.insert(*line, 0, text);
                }
            }
            EditType::Replace {
                line,
                col,
                old_text,
                new_text,
            } => {
                let start = buffer.line_to_char(line - 1) + col;
                let end = start + new_text.chars().count();
                buffer.remove_range(start, end);
                buffer.insert(*line, *col, old_text);
            }
            EditType::Merge {
                line,
                deleted_newline_col,
            } => {
                // `line` is the 1-based line that absorbed the next line; the
                // removed newline belonged to the end of that line.
                buffer.insert(*line, *deleted_newline_col, "\n");
            }
            EditType::Split { line, col } => {
                let start = buffer.line_to_char(line - 1) + col;
                // A split at column 0 of line 1 has `start == 0`; it still
                // inserted a newline that must be removed on undo.
                if start < buffer.len_chars() {
                    buffer.remove_range(start, start + 1);
                }
            }
            EditType::Group { edits } => {
                for edit in edits.iter().rev() {
                    self.apply_undo(buffer, edit);
                }
            }
        }
    }

    fn apply_redo(&self, buffer: &mut TextBuffer, edit: &Edit) {
        match &edit.edit_type {
            EditType::Insert { line, col, text } => {
                buffer.insert(*line, *col, text);
            }
            EditType::Delete { line, col, text } => {
                let start = buffer.line_to_char(line - 1) + col;
                let end = start + text.chars().count();
                buffer.remove_range(start, end);
            }
            EditType::InsertLine { line, text } => {
                buffer.insert(*line, 0, text);
                // `insert` takes a character column; `text.len()` is bytes and
                // corrupts non-ASCII lines.
                buffer.insert(*line, text.chars().count(), "\n");
            }
            EditType::DeleteLine { line, text: _ } => {
                buffer.delete_line(*line);
            }
            EditType::Replace {
                line,
                col,
                old_text,
                new_text,
            } => {
                let start = buffer.line_to_char(line - 1) + col;
                let end = start + old_text.chars().count();
                buffer.remove_range(start, end);
                buffer.insert(*line, *col, new_text);
            }
            EditType::Merge {
                line,
                deleted_newline_col: _,
            } => {
                buffer.merge_with_prev_line(*line);
            }
            EditType::Split { line, col } => {
                buffer.insert(*line, *col, "\n");
            }
            EditType::Group { edits } => {
                for edit in edits.iter() {
                    self.apply_redo(buffer, edit);
                }
            }
        }
    }
}

impl Default for UndoManager {
    fn default() -> Self {
        Self::new()
    }
}

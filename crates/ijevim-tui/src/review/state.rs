use std::path::PathBuf;

// ── Review sub-mode ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewSubMode {
    /// Navigating file list (left panel)
    FileList,
    /// Navigating diff hunks (center/right panel)
    DiffView,
    /// Viewing full file content
    Preview,
    /// Writing a comment (mini-buffer style)
    CommentInput,
    /// Browsing all annotations for the review
    Annotations,
    /// Keybinding help
    Help,
}

/// A single row in the flattened diff view.
///
/// This is the shared navigation model used by both the keymap and the
/// renderer so that "the current line" means the same thing everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffRowKind {
    HunkHeader,
    Add,
    Delete,
    Context,
}

#[derive(Debug, Clone)]
pub struct DiffRow {
    /// Index of the owning hunk in the current file.
    pub hunk_index: usize,
    /// Index of the owning line inside the hunk (`None` for hunk headers).
    pub line_index: Option<usize>,
    pub kind: DiffRowKind,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
    pub content: String,
    pub staged: bool,
}

impl DiffRow {
    /// Line number an annotation on this row should be attached to.
    pub fn comment_line(&self) -> Option<usize> {
        self.new_lineno.or(self.old_lineno)
    }
}

/// The line an in-progress comment will be attached to.
#[derive(Debug, Clone)]
pub struct CommentTarget {
    pub file_path: String,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStyle {
    Unified,
    SideBySide,
}

// ── Diff data model ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

impl std::fmt::Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileStatus::Added => write!(f, "A"),
            FileStatus::Modified => write!(f, "M"),
            FileStatus::Deleted => write!(f, "D"),
            FileStatus::Renamed => write!(f, "R"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Add,
    Delete,
    Context,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
    /// The source diff had `\ No newline at end of file` after this line.
    pub no_newline: bool,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub header: String,
    pub lines: Vec<DiffLine>,
    /// Whether this hunk has been staged/unstaged (toggled by Enter in review mode).
    pub staged: bool,
}

#[derive(Debug, Clone)]
pub struct DiffFile {
    pub status: FileStatus,
    pub old_path: String,
    pub new_path: String,
    pub hunks: Vec<Hunk>,
}

impl DiffFile {
    /// Path a human should see (and annotations are keyed by).
    ///
    /// Deleted files have no new path, so fall back to the old path instead of
    /// showing `/dev/null`.
    pub fn display_path(&self) -> &str {
        if self.new_path == "/dev/null" || self.new_path.is_empty() {
            &self.old_path
        } else {
            &self.new_path
        }
    }
}

// ── Annotation / Comment ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Annotation {
    pub id: String,
    pub file_path: String,
    pub line: usize,
    /// Last line of a multi-line comment (`None` for single-line, v1 files).
    pub end_line: Option<usize>,
    pub text: String,
    pub resolved: bool,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct Comment {
    pub id: String,
    pub file_path: String,
    pub hunk_new_start: usize,
    pub text: String,
    pub resolved: bool,
}

// ── Scroll state ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollState {
    pub scroll_top: usize,
}

// ── ReviewArgs: :review command arguments ────────────────────────────

#[derive(Debug, Clone)]
pub enum ReviewArgs {
    /// No args: diff against HEAD (staged + unstaged changes)
    WorkingTree,
    /// `--staged` or `--cached`: staged diff
    Staged,
    /// `<rev>`: diff against a revision (HEAD, branch, SHA)
    Against(String),
    /// `<rev1>..<rev2>`: range between two revisions
    Range(String, String),
    /// `-- <path>` or existing file path as positional: specific file
    File(String),
}

impl ReviewArgs {
    /// Parse `:review` arguments string into a `ReviewArgs`.
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        if input.is_empty() || input == "." {
            return Self::WorkingTree;
        }
        if input == "--staged" || input == "--cached" {
            return Self::Staged;
        }
        if let Some(pos) = input.find("..") {
            let before = input[..pos].to_string();
            let after = input[pos + 2..].to_string();
            return Self::Range(before, after);
        }
        if let Some(path) = input.strip_prefix("-- ") {
            return Self::File(path.to_string());
        }
        // If it looks like an existing file path, treat as file
        if PathBuf::from(input).exists() && !Self::looks_like_revision(input) {
            return Self::File(input.to_string());
        }
        Self::Against(input.to_string())
    }

    fn looks_like_revision(s: &str) -> bool {
        // Revisions don't contain path separators
        !s.contains('/') && !s.contains('\\') && !s.starts_with('.')
    }
}

impl std::fmt::Display for ReviewArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewArgs::WorkingTree => write!(f, "Changes since HEAD"),
            ReviewArgs::Staged => write!(f, "Staged changes"),
            ReviewArgs::Against(rev) => write!(f, "Diff against {}", rev),
            ReviewArgs::Range(a, b) => write!(f, "Diff {}..{}", a, b),
            ReviewArgs::File(path) => write!(f, "File: {}", path),
        }
    }
}

// ── ReviewState (owned by Engine) ────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ReviewState {
    pub args: ReviewArgs,
    /// Parsed diff files
    pub files: Vec<DiffFile>,
    /// Currently selected file index
    pub selected_file: usize,
    /// Currently selected hunk index within the selected file
    pub selected_hunk: usize,
    /// Current sub-mode
    pub sub_mode: ReviewSubMode,
    /// Selected row in the flattened diff view.
    pub cursor_row: usize,
    /// Selected annotation index in the annotation browser.
    pub annotation_selected: usize,
    /// `d` was pressed once in the annotation browser; the next `d` deletes.
    pub annotation_pending_delete: bool,
    /// File path awaiting discard confirmation (`X`).
    pub pending_discard: Option<String>,
    /// Target of an in-progress comment.
    pub comment_target: Option<CommentTarget>,
    /// User annotations
    pub annotations: Vec<Annotation>,
    /// Review comments
    pub comments: Vec<Comment>,
    /// Scroll position
    pub scroll: ScrollState,
    /// Number of visible rows in the diff panel (updated by the editor).
    pub viewport_height: usize,
    /// Diff display style
    pub diff_style: DiffStyle,
    /// Width of the file list panel (in characters)
    pub file_list_width: u16,
    /// Git project root (detected)
    pub project_root: Option<PathBuf>,
    /// In-progress comment text (CommentInput sub-mode buffer)
    pub comment_buffer: String,
    /// Cursor position within the comment buffer
    pub comment_cursor: usize,
}

impl ReviewState {
    /// Total number of changes (added + deleted lines) across all files
    pub fn total_changes(&self) -> usize {
        self.files
            .iter()
            .flat_map(|f| f.hunks.iter())
            .flat_map(|h| h.lines.iter())
            .filter(|l| matches!(l.kind, DiffLineKind::Add | DiffLineKind::Delete))
            .count()
    }

    /// Number of files with changes
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Get the currently selected file
    pub fn current_file(&self) -> Option<&DiffFile> {
        self.files.get(self.selected_file)
    }

    /// Get the currently selected hunk
    pub fn current_hunk(&self) -> Option<&Hunk> {
        self.current_file()
            .and_then(|f| f.hunks.get(self.selected_hunk))
    }

    // ── Flattened row navigation ────────────────────────────────────

    /// Flattened rows for the currently selected file.
    pub fn current_rows(&self) -> Vec<DiffRow> {
        match self.current_file() {
            Some(file) => crate::review::rows::file_rows(file),
            None => Vec::new(),
        }
    }

    /// Flattened rows for a file by index.
    pub fn rows_for(&self, file_index: usize) -> Vec<DiffRow> {
        match self.files.get(file_index) {
            Some(file) => crate::review::rows::file_rows(file),
            None => Vec::new(),
        }
    }

    /// The row under the cursor.
    pub fn current_row(&self) -> Option<DiffRow> {
        self.current_rows().into_iter().nth(self.cursor_row)
    }

    /// The file line an annotation placed at the cursor would target.
    pub fn current_line(&self) -> Option<usize> {
        self.current_row().and_then(|row| row.comment_line())
    }

    /// Select a file and reset the cursor / scroll position.
    pub fn select_file(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }
        self.selected_file = index;
        self.selected_hunk = 0;
        self.cursor_row = 0;
        self.scroll.scroll_top = 0;
    }

    /// Move the cursor by `delta` rows, clamping at the ends and keeping the
    /// owning hunk in sync for staging commands.
    pub fn move_cursor(&mut self, delta: isize) {
        let total = self.current_rows().len();
        if total == 0 {
            self.cursor_row = 0;
            self.ensure_cursor_visible();
            return;
        }

        let next = if delta < 0 {
            self.cursor_row.saturating_sub(delta.unsigned_abs())
        } else {
            self.cursor_row
                .saturating_add(delta as usize)
                .min(total - 1)
        };
        self.set_cursor_row(next);
    }

    /// Set the cursor to an absolute row and keep dependent state in sync.
    pub fn set_cursor_row(&mut self, row: usize) {
        let rows = self.current_rows();
        if rows.is_empty() {
            self.cursor_row = 0;
            self.ensure_cursor_visible();
            return;
        }
        self.cursor_row = row.min(rows.len() - 1);
        if let Some(current) = rows.get(self.cursor_row) {
            self.selected_hunk = current.hunk_index;
        }
        self.ensure_cursor_visible();
    }

    /// Scroll so the cursor row is visible inside the diff panel.
    pub fn ensure_cursor_visible(&mut self) {
        let visible = self.viewport_height.max(1);
        let cursor = self.cursor_row;
        if cursor < self.scroll.scroll_top {
            self.scroll.scroll_top = cursor;
        } else if cursor >= self.scroll.scroll_top.saturating_add(visible) {
            self.scroll.scroll_top = cursor + 1 - visible;
        }
    }

    /// Jump the cursor to the next/previous hunk header.
    pub fn jump_hunk(&mut self, forward: bool) {
        let rows = self.current_rows();
        let target = if forward {
            crate::review::rows::next_hunk_row(&rows, self.cursor_row)
        } else {
            crate::review::rows::prev_hunk_row(&rows, self.cursor_row)
        };
        if let Some(row) = target {
            self.set_cursor_row(row);
        }
    }

    /// Select the given annotation in the browser, clamping the index.
    pub fn select_annotation(&mut self, index: usize) {
        if self.annotations.is_empty() {
            self.annotation_selected = 0;
        } else {
            self.annotation_selected = index.min(self.annotations.len() - 1);
        }
        self.annotation_pending_delete = false;
    }
}

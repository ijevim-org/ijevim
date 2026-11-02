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
    /// Keybinding help
    Help,
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

// ── Annotation / Comment ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Annotation {
    pub id: String,
    pub file_path: String,
    pub line: usize,
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
    /// User annotations
    pub annotations: Vec<Annotation>,
    /// Review comments
    pub comments: Vec<Comment>,
    /// Scroll position
    pub scroll: ScrollState,
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
}

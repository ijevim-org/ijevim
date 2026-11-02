/// Panel layout for Review Mode.
///
/// The terminal is divided into:
///   ┌─────────────────────────────────────────────────────┐
///   │ File List Panel  │  Diff View Panel                 │
///   │ (file_list_width)│  (remaining)                     │
///   │                  │  @@ -10,7 +10,9 @@ fn main() {   │
///   │ M src/main.rs    │   context line                   │
///   │ A src/lib.rs     │  -old code                       │
///   │ D src/old.rs     │  +new code                       │
///   ├──────────────────┴──────────────────────────────────┤
///   │  Status Bar: [REVIEW]                    1/2 files  │
///   └─────────────────────────────────────────────────────┘
pub struct ReviewLayout {
    /// File list panel region
    pub file_list: PanelRect,
    /// Diff view panel region
    pub diff_view: PanelRect,
    /// Status bar row
    pub status_row: usize,
}

/// A rectangular region of the terminal.
#[derive(Debug, Clone, Copy)]
pub struct PanelRect {
    pub row_start: usize,
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize,
}

impl PanelRect {
    pub fn width(&self) -> usize {
        self.col_end.saturating_sub(self.col_start)
    }

    pub fn height(&self) -> usize {
        self.row_end.saturating_sub(self.row_start)
    }
}

/// Compute the review mode panel layout for the given terminal dimensions.
pub fn compute_review_layout(rows: usize, cols: usize, file_list_width: u16) -> ReviewLayout {
    // Reserve last row for status bar
    let content_rows = rows.saturating_sub(1);

    let file_list_col_end = (file_list_width as usize).min(cols.saturating_sub(1));

    // If the file list is wider than half the screen, clamp it
    let file_list_col_end = file_list_col_end.min(cols / 3 * 2);

    ReviewLayout {
        file_list: PanelRect {
            row_start: 0,
            row_end: content_rows,
            col_start: 0,
            col_end: file_list_col_end,
        },
        diff_view: PanelRect {
            row_start: 0,
            row_end: content_rows,
            col_start: file_list_col_end,
            col_end: cols,
        },
        status_row: rows.saturating_sub(1),
    }
}

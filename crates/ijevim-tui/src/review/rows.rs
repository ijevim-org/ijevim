//! Flattened row model for the diff view.
//!
//! Both the keymap and the renderer consume [`DiffRow`] values produced here,
//! so "the current line" means the same thing in navigation, highlighting,
//! comments and annotation lookup.

use crate::review::state::{DiffFile, DiffLineKind, DiffRow, DiffRowKind};

/// Build the flattened rows for one file.
///
/// Every hunk contributes one [`DiffRowKind::HunkHeader`] row followed by one
/// row per diff line. Hunk headers carry `new_start` as their line number so a
/// comment placed on a header still anchors to a real file line.
pub fn file_rows(file: &DiffFile) -> Vec<DiffRow> {
    let mut rows = Vec::new();

    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        rows.push(DiffRow {
            hunk_index,
            line_index: None,
            kind: DiffRowKind::HunkHeader,
            old_lineno: None,
            new_lineno: Some(hunk.new_start),
            content: hunk.header.clone(),
            staged: hunk.staged,
        });

        for (line_index, line) in hunk.lines.iter().enumerate() {
            rows.push(DiffRow {
                hunk_index,
                line_index: Some(line_index),
                kind: match line.kind {
                    DiffLineKind::Add => DiffRowKind::Add,
                    DiffLineKind::Delete => DiffRowKind::Delete,
                    DiffLineKind::Context => DiffRowKind::Context,
                },
                old_lineno: line.old_lineno,
                new_lineno: line.new_lineno,
                content: line.content.clone(),
                staged: hunk.staged,
            });
        }
    }

    rows
}

/// Index of the first row belonging to `hunk_index`, if present.
pub fn first_row_of_hunk(rows: &[DiffRow], hunk_index: usize) -> Option<usize> {
    rows.iter().position(|row| row.hunk_index == hunk_index)
}

/// Index of the next hunk header strictly after `cursor`, if any.
pub fn next_hunk_row(rows: &[DiffRow], cursor: usize) -> Option<usize> {
    rows.iter()
        .enumerate()
        .skip(cursor + 1)
        .find(|(_, row)| row.kind == DiffRowKind::HunkHeader)
        .map(|(index, _)| index)
}

/// Index of the previous hunk header strictly before `cursor`, if any.
pub fn prev_hunk_row(rows: &[DiffRow], cursor: usize) -> Option<usize> {
    rows.iter()
        .enumerate()
        .take(cursor)
        .rev()
        .find(|(_, row)| row.kind == DiffRowKind::HunkHeader)
        .map(|(index, _)| index)
}

/// Find the row that displays `line` (new or old side).
pub fn row_for_line(rows: &[DiffRow], line: usize) -> Option<usize> {
    rows.iter()
        .position(|row| {
            row.kind != DiffRowKind::HunkHeader
                && (row.new_lineno == Some(line) || row.old_lineno == Some(line))
        })
        .or_else(|| rows.iter().position(|row| row.new_lineno == Some(line)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::state::{DiffFile, DiffLine, FileStatus, Hunk};

    fn sample_file() -> DiffFile {
        DiffFile {
            status: FileStatus::Modified,
            old_path: "src/main.rs".to_string(),
            new_path: "src/main.rs".to_string(),
            hunks: vec![
                Hunk {
                    old_start: 10,
                    old_count: 2,
                    new_start: 10,
                    new_count: 3,
                    header: "@@ -10,2 +10,3 @@".to_string(),
                    staged: false,
                    lines: vec![
                        DiffLine {
                            kind: DiffLineKind::Context,
                            content: "keep".to_string(),
                            old_lineno: Some(10),
                            new_lineno: Some(10),
                        },
                        DiffLine {
                            kind: DiffLineKind::Delete,
                            content: "old".to_string(),
                            old_lineno: Some(11),
                            new_lineno: None,
                        },
                        DiffLine {
                            kind: DiffLineKind::Add,
                            content: "new".to_string(),
                            old_lineno: None,
                            new_lineno: Some(11),
                        },
                    ],
                },
                Hunk {
                    old_start: 20,
                    old_count: 1,
                    new_start: 21,
                    new_count: 1,
                    header: "@@ -20,1 +21,1 @@".to_string(),
                    staged: true,
                    lines: vec![DiffLine {
                        kind: DiffLineKind::Add,
                        content: "later".to_string(),
                        old_lineno: None,
                        new_lineno: Some(21),
                    }],
                },
            ],
        }
    }

    #[test]
    fn flattens_hunks_with_headers() {
        let rows = file_rows(&sample_file());
        assert_eq!(rows.len(), 6);

        assert_eq!(rows[0].kind, DiffRowKind::HunkHeader);
        assert_eq!(rows[0].hunk_index, 0);
        assert_eq!(rows[0].line_index, None);
        assert_eq!(rows[0].new_lineno, Some(10));

        assert_eq!(rows[1].kind, DiffRowKind::Context);
        assert_eq!(rows[1].line_index, Some(0));
        assert_eq!(rows[2].kind, DiffRowKind::Delete);
        assert_eq!(rows[2].old_lineno, Some(11));
        assert_eq!(rows[3].kind, DiffRowKind::Add);
        assert_eq!(rows[3].new_lineno, Some(11));

        assert_eq!(rows[4].kind, DiffRowKind::HunkHeader);
        assert_eq!(rows[4].hunk_index, 1);
        assert!(rows[4].staged);
        assert_eq!(rows[5].kind, DiffRowKind::Add);
    }

    #[test]
    fn navigates_between_hunks() {
        let rows = file_rows(&sample_file());
        assert_eq!(next_hunk_row(&rows, 0), Some(4));
        assert_eq!(next_hunk_row(&rows, 4), None);
        assert_eq!(prev_hunk_row(&rows, 5), Some(4));
        assert_eq!(prev_hunk_row(&rows, 0), None);
        assert_eq!(first_row_of_hunk(&rows, 1), Some(4));
    }

    #[test]
    fn locates_line_on_either_side() {
        let rows = file_rows(&sample_file());
        assert_eq!(row_for_line(&rows, 10), Some(1));
        assert_eq!(row_for_line(&rows, 11), Some(2));
        assert_eq!(row_for_line(&rows, 21), Some(5));
        assert_eq!(row_for_line(&rows, 99), None);
    }
}

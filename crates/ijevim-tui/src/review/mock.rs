use crate::review::state::{
    DiffFile, DiffLine, DiffLineKind, FileStatus, Hunk, ReviewArgs, ReviewState, ReviewSubMode,
    ScrollState,
};

/// Generate mock diff data for Phase 1 development.
///
/// This is replaced by real `git diff` output in Phase 2.
pub fn create_mock_review_state(args: ReviewArgs) -> ReviewState {
    let mock_files = vec![
        DiffFile {
            status: FileStatus::Modified,
            old_path: "src/main.rs".to_string(),
            new_path: "src/main.rs".to_string(),
            hunks: vec![Hunk {
                old_start: 10,
                old_count: 7,
                new_start: 10,
                new_count: 9,
                header: "@@ -10,7 +10,9 @@ fn main() {".to_string(),
                staged: false,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Context,
                        content: "    let x = 1;".to_string(),
                        old_lineno: Some(10),
                        new_lineno: Some(10),
                    },
                    DiffLine {
                        kind: DiffLineKind::Context,
                        content: "    let y = 2;".to_string(),
                        old_lineno: Some(11),
                        new_lineno: Some(11),
                    },
                    DiffLine {
                        kind: DiffLineKind::Delete,
                        content: "    println!(\"old\");".to_string(),
                        old_lineno: Some(12),
                        new_lineno: None,
                    },
                    DiffLine {
                        kind: DiffLineKind::Add,
                        content: "    println!(\"new and improved\");".to_string(),
                        old_lineno: None,
                        new_lineno: Some(12),
                    },
                    DiffLine {
                        kind: DiffLineKind::Add,
                        content: "    println!(\"another line\");".to_string(),
                        old_lineno: None,
                        new_lineno: Some(13),
                    },
                    DiffLine {
                        kind: DiffLineKind::Context,
                        content: "    let z = 3;".to_string(),
                        old_lineno: Some(13),
                        new_lineno: Some(14),
                    },
                    DiffLine {
                        kind: DiffLineKind::Context,
                        content: "}".to_string(),
                        old_lineno: Some(14),
                        new_lineno: Some(15),
                    },
                ],
            }],
        },
        DiffFile {
            status: FileStatus::Added,
            old_path: "src/lib.rs".to_string(),
            new_path: "src/lib.rs".to_string(),
            hunks: vec![Hunk {
                old_start: 0,
                old_count: 0,
                new_start: 1,
                new_count: 3,
                header: "@@ -0,0 +1,3 @@".to_string(),
                staged: false,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Add,
                        content: "pub mod review;".to_string(),
                        old_lineno: None,
                        new_lineno: Some(1),
                    },
                    DiffLine {
                        kind: DiffLineKind::Add,
                        content: "pub mod types;".to_string(),
                        old_lineno: None,
                        new_lineno: Some(2),
                    },
                    DiffLine {
                        kind: DiffLineKind::Add,
                        content: "pub mod editor;".to_string(),
                        old_lineno: None,
                        new_lineno: Some(3),
                    },
                ],
            }],
        },
    ];

    ReviewState {
        args,
        files: mock_files,
        selected_file: 0,
        selected_hunk: 0,
        sub_mode: ReviewSubMode::FileList,
        annotations: Vec::new(),
        comments: Vec::new(),
        scroll: ScrollState::default(),
        diff_style: crate::review::DiffStyle::Unified,
        file_list_width: 30,
        project_root: None,
        comment_buffer: String::new(),
        comment_cursor: 0,
    }
}

use crate::review::layout::{compute_review_layout, PanelRect};
use crate::review::state::{
    DiffFile, DiffLineKind, DiffStyle, FileStatus, ReviewState, ReviewSubMode,
};
use crate::screen::{CellStyle, ScreenBuffer};
use crossterm::style::Color;
use std::collections::HashSet;
use std::path::PathBuf;

// ── Colour palette for review mode ──────────────────────────────────

/// Dark green background for added lines
const ADD_BG: Color = Color::AnsiValue(22);
/// Green foreground for `+` marker
const ADD_FG: Color = Color::AnsiValue(10);
/// Dark red background for deleted lines
const DEL_BG: Color = Color::AnsiValue(52);
/// Red foreground for `-` marker
const DEL_FG: Color = Color::AnsiValue(9);
/// Cyan-ish foreground for hunk headers
const HUNK_HEADER_FG: Color = Color::AnsiValue(33);
/// Selected file/hunk highlight
const SELECTED_BG: Color = Color::AnsiValue(236);
/// Status bar background
const STATUS_BG: Color = Color::AnsiValue(236);
/// Mode indicator background (blue)
const MODE_BG: Color = Color::AnsiValue(21);
/// Added file status
const STATUS_A: Color = Color::AnsiValue(10);
/// Modified file status
const STATUS_M: Color = Color::AnsiValue(11);
/// Deleted file status
const STATUS_D: Color = Color::AnsiValue(9);
/// Renamed file status
const STATUS_R: Color = Color::AnsiValue(13);
/// Dim foreground for secondary info
const DIM_FG: Color = Color::AnsiValue(245);
/// Separator between file list and diff
const SEPARATOR_FG: Color = Color::AnsiValue(240);
/// Yellow foreground for annotation markers
const ANNOTATION_FG: Color = Color::AnsiValue(11);

// ── Main entry point ────────────────────────────────────────────────

/// Render the review mode UI into the screen buffer.
///
/// Call this from `Renderer::render()` when review state is active.
pub fn render_review(screen: &mut ScreenBuffer, review: &ReviewState, rows: usize, cols: usize) {
    let layout = compute_review_layout(rows, cols, review.file_list_width);

    // Clear the screen
    for r in 0..layout.status_row {
        for c in 0..cols {
            screen.set_cell(r, c, ' ', CellStyle::default());
        }
    }

    match review.sub_mode {
        ReviewSubMode::FileList | ReviewSubMode::DiffView => {
            render_file_list(screen, review, layout.file_list);
            render_separator(screen, layout.file_list.col_end, layout.status_row, cols);
            render_diff_view(screen, review, layout.diff_view);
        }
        ReviewSubMode::Preview => {
            render_file_preview(
                screen,
                review,
                PanelRect {
                    row_start: 0,
                    row_end: layout.status_row,
                    col_start: 0,
                    col_end: cols,
                },
            );
        }
        ReviewSubMode::Help => {
            // Still render the background content dimmed
            render_file_list(screen, review, layout.file_list);
            render_separator(screen, layout.file_list.col_end, layout.status_row, cols);
            render_diff_view(screen, review, layout.diff_view);
            render_help_overlay(screen, rows, cols);
        }
        ReviewSubMode::CommentInput => {
            render_file_list(screen, review, layout.file_list);
            render_separator(screen, layout.file_list.col_end, layout.status_row, cols);
            render_diff_view(screen, review, layout.diff_view);
            render_comment_input(screen, review, layout.status_row, cols);
        }
    }

    render_status_bar(screen, review, layout.status_row, cols);
}

// ── File list panel ─────────────────────────────────────────────────

fn render_file_list(screen: &mut ScreenBuffer, review: &ReviewState, rect: PanelRect) {
    if rect.width() < 4 || rect.height() < 1 {
        return;
    }

    let title_style = CellStyle {
        fg: DIM_FG,
        bold: true,
        ..CellStyle::default()
    };
    let selected_style = CellStyle {
        bg: SELECTED_BG,
        ..CellStyle::default()
    };

    // Title
    let title = " Files ";
    for (i, ch) in title.chars().enumerate() {
        if i < rect.width() {
            screen.set_cell(rect.row_start, rect.col_start + i, ch, title_style);
        }
    }

    // Separator line under title
    let sep_char = '─';
    for c in rect.col_start..rect.col_end {
        screen.set_cell(
            rect.row_start + 1,
            c,
            sep_char,
            CellStyle {
                fg: SEPARATOR_FG,
                ..CellStyle::default()
            },
        );
    }

    // Empty state
    if review.files.is_empty() {
        let empty_row = rect.row_start + 2;
        let msg = " No changes to review ";
        let empty_style = CellStyle {
            fg: DIM_FG,
            ..CellStyle::default()
        };
        for (j, ch) in msg.chars().enumerate() {
            if rect.col_start + j < rect.col_end {
                screen.set_cell(empty_row, rect.col_start + j, ch, empty_style);
            }
        }
        let hint = " :review --staged to see staged changes ";
        let hint_style = CellStyle {
            fg: Color::AnsiValue(240),
            ..CellStyle::default()
        };
        for (j, ch) in hint.chars().enumerate() {
            if rect.col_start + j < rect.col_end {
                screen.set_cell(empty_row + 1, rect.col_start + j, ch, hint_style);
            }
        }
        return;
    }

    // File entries
    let max_visible = rect.height().saturating_sub(3); // title + sep + count
    for i in 0..review.files.len().min(max_visible) {
        let row = rect.row_start + 2 + i;
        let file = &review.files[i];
        let is_selected = i == review.selected_file;

        // Background for selected item
        if is_selected {
            for c in rect.col_start..rect.col_end {
                screen.set_cell(row, c, ' ', selected_style);
            }
        }

        // Status icon
        let status_fg = status_color(file.status);
        let status_str = match file.status {
            FileStatus::Added => "A ",
            FileStatus::Modified => "M ",
            FileStatus::Deleted => "D ",
            FileStatus::Renamed => "R ",
        };
        let status_style = CellStyle {
            fg: status_fg,
            bg: if is_selected {
                SELECTED_BG
            } else {
                Color::Reset
            },
            bold: true,
            ..CellStyle::default()
        };
        for (j, ch) in status_str.chars().enumerate() {
            if rect.col_start + j < rect.col_end {
                screen.set_cell(row, rect.col_start + j, ch, status_style);
            }
        }

        // Filename
        let name_start = rect.col_start + 2;
        let name_width = rect.width().saturating_sub(2);
        let display_name = short_filename(&file.new_path, name_width);
        let name_style = CellStyle {
            bg: if is_selected {
                SELECTED_BG
            } else {
                Color::Reset
            },
            ..CellStyle::default()
        };
        for (j, ch) in display_name.chars().enumerate() {
            if name_start + j < rect.col_end {
                screen.set_cell(row, name_start + j, ch, name_style);
            }
        }

        // Fill remaining with spaces if selected
        if is_selected {
            let fill_start = name_start + display_name.chars().count();
            for c in fill_start..rect.col_end {
                screen.set_cell(row, c, ' ', selected_style);
            }
        }
    }

    // File count at bottom
    let count_row = rect.row_end.saturating_sub(1);
    let count_text = format!(" {}/{} ", review.selected_file + 1, review.files.len());
    let count_style = CellStyle {
        fg: DIM_FG,
        ..CellStyle::default()
    };
    for (j, ch) in count_text.chars().enumerate() {
        if rect.col_start + j < rect.col_end {
            screen.set_cell(count_row, rect.col_start + j, ch, count_style);
        }
    }
}

// ── Separator ───────────────────────────────────────────────────────

fn render_separator(screen: &mut ScreenBuffer, col: usize, max_row: usize, _cols: usize) {
    let sep_style = CellStyle {
        fg: SEPARATOR_FG,
        ..CellStyle::default()
    };
    for r in 0..max_row {
        screen.set_cell(r, col, '│', sep_style);
    }
}

// ── Diff view panel ─────────────────────────────────────────────────

fn render_diff_view(screen: &mut ScreenBuffer, review: &ReviewState, rect: PanelRect) {
    if rect.width() < 4 || rect.height() < 1 {
        return;
    }

    let file = match review.files.get(review.selected_file) {
        Some(f) => f,
        None => return,
    };

    match review.diff_style {
        DiffStyle::Unified => render_unified_diff(screen, review, file, rect),
        DiffStyle::SideBySide => render_side_by_side_diff(screen, review, file, rect),
    }
}

// ── Unified diff rendering ──────────────────────────────────────────

fn render_unified_diff(
    screen: &mut ScreenBuffer,
    review: &ReviewState,
    file: &DiffFile,
    rect: PanelRect,
) {
    // File header
    let header = format!("--- a/{}", file.old_path);
    let header_style = CellStyle {
        fg: DIM_FG,
        ..CellStyle::default()
    };
    set_string_styled(
        screen,
        rect.row_start,
        rect.col_start,
        &header,
        header_style,
    );

    let header2 = format!("+++ b/{}", file.new_path);
    set_string_styled(
        screen,
        rect.row_start + 1,
        rect.col_start,
        &header2,
        header_style,
    );

    // Collect all lines from all hunks with their hunk headers
    let annotated_set = annotated_lines_for_file(review, &file.new_path);
    let mut display_lines: Vec<DisplayLine> = Vec::new();

    for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
        let is_selected_hunk = hunk_idx == review.selected_hunk;

        display_lines.push(DisplayLine {
            kind: DisplayLineKind::HunkHeader,
            content: hunk.header.clone(),
            is_selected: is_selected_hunk,
            staged: hunk.staged,
            has_annotation: false,
            annotation_count: 0,
        });

        for line in &hunk.lines {
            let lineno = line.new_lineno.or(line.old_lineno);
            let (has_ann, ann_count) = match lineno {
                Some(ln) => (annotated_set.contains(&ln), {
                    review
                        .annotations
                        .iter()
                        .filter(|a| a.file_path == file.new_path && a.line == ln && !a.resolved)
                        .count()
                }),
                None => (false, 0),
            };

            display_lines.push(DisplayLine {
                kind: match line.kind {
                    DiffLineKind::Add => DisplayLineKind::Added,
                    DiffLineKind::Delete => DisplayLineKind::Deleted,
                    DiffLineKind::Context => DisplayLineKind::Context,
                },
                content: line.content.clone(),
                is_selected: is_selected_hunk,
                staged: hunk.staged,
                has_annotation: has_ann,
                annotation_count: ann_count,
            });
        }
    }

    // Render visible lines
    let scroll = review.scroll.scroll_top;
    let max_visible = rect.height().saturating_sub(2);

    for i in 0..max_visible {
        let idx = scroll + i;
        if idx >= display_lines.len() {
            break;
        }
        let row = rect.row_start + 2 + i;
        let dl = &display_lines[idx];

        match dl.kind {
            DisplayLineKind::HunkHeader => {
                render_hunk_header_line(screen, row, &rect, dl);
            }
            DisplayLineKind::Added => {
                render_diff_line(screen, row, &rect, dl, '+', ADD_BG, ADD_FG);
            }
            DisplayLineKind::Deleted => {
                render_diff_line(screen, row, &rect, dl, '-', DEL_BG, DEL_FG);
            }
            DisplayLineKind::Context => {
                render_diff_line(screen, row, &rect, dl, ' ', Color::Reset, Color::Reset);
            }
        }
    }

    // No changes indicator
    if display_lines.is_empty() {
        let msg = " (no changes) ";
        let style = CellStyle {
            fg: DIM_FG,
            ..CellStyle::default()
        };
        set_string_styled(screen, rect.row_start + 2, rect.col_start, msg, style);
    }
}

// ── Side-by-side diff rendering ─────────────────────────────────────

fn render_side_by_side_diff(
    screen: &mut ScreenBuffer,
    review: &ReviewState,
    file: &DiffFile,
    rect: PanelRect,
) {
    // Side-by-side uses half the width for old (left) and half for new (right)
    let half = rect.width() / 2;
    let left_rect = PanelRect {
        col_start: rect.col_start,
        col_end: rect.col_start + half,
        ..rect
    };
    let right_rect = PanelRect {
        col_start: rect.col_start + half,
        col_end: rect.col_end,
        ..rect
    };

    // Title row
    let title_style = CellStyle {
        fg: DIM_FG,
        bold: true,
        ..CellStyle::default()
    };
    set_string_styled(
        screen,
        rect.row_start,
        left_rect.col_start,
        " OLD ",
        title_style,
    );
    set_string_styled(
        screen,
        rect.row_start,
        right_rect.col_start,
        " NEW ",
        title_style,
    );

    let sep_style = CellStyle {
        fg: SEPARATOR_FG,
        ..CellStyle::default()
    };
    let sep_col = rect.col_start + half;
    for r in rect.row_start..rect.row_end {
        screen.set_cell(r, sep_col, '│', sep_style);
    }

    // Build side-by-side lines from hunks
    let annotated_set = annotated_lines_for_file(review, &file.new_path);
    let mut left_lines: Vec<DisplayLine> = Vec::new();
    let mut right_lines: Vec<DisplayLine> = Vec::new();

    for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
        let is_selected = hunk_idx == review.selected_hunk;

        let staged_indicator = if hunk.staged { "[S] " } else { "    " };
        left_lines.push(DisplayLine {
            kind: DisplayLineKind::HunkHeader,
            content: format!("{}@@ -{}", staged_indicator, hunk.old_start),
            is_selected,
            staged: hunk.staged,
            has_annotation: false,
            annotation_count: 0,
        });
        right_lines.push(DisplayLine {
            kind: DisplayLineKind::HunkHeader,
            content: format!("{}@@ +{}", staged_indicator, hunk.new_start),
            is_selected,
            staged: hunk.staged,
            has_annotation: false,
            annotation_count: 0,
        });

        for line in &hunk.lines {
            let lineno = line.new_lineno.or(line.old_lineno);
            let (has_ann, ann_count) = match lineno {
                Some(ln) => (annotated_set.contains(&ln), {
                    review
                        .annotations
                        .iter()
                        .filter(|a| a.file_path == file.new_path && a.line == ln && !a.resolved)
                        .count()
                }),
                None => (false, 0),
            };

            match line.kind {
                DiffLineKind::Add => {
                    left_lines.push(DisplayLine {
                        kind: DisplayLineKind::Context,
                        content: String::new(),
                        is_selected,
                        staged: hunk.staged,
                        has_annotation: false,
                        annotation_count: 0,
                    });
                    right_lines.push(DisplayLine {
                        kind: DisplayLineKind::Added,
                        content: line.content.clone(),
                        is_selected,
                        staged: hunk.staged,
                        has_annotation: has_ann,
                        annotation_count: ann_count,
                    });
                }
                DiffLineKind::Delete => {
                    left_lines.push(DisplayLine {
                        kind: DisplayLineKind::Deleted,
                        content: line.content.clone(),
                        is_selected,
                        staged: hunk.staged,
                        has_annotation: has_ann,
                        annotation_count: ann_count,
                    });
                    right_lines.push(DisplayLine {
                        kind: DisplayLineKind::Context,
                        content: String::new(),
                        is_selected,
                        staged: hunk.staged,
                        has_annotation: false,
                        annotation_count: 0,
                    });
                }
                DiffLineKind::Context => {
                    left_lines.push(DisplayLine {
                        kind: DisplayLineKind::Context,
                        content: line.content.clone(),
                        is_selected,
                        staged: hunk.staged,
                        has_annotation: has_ann,
                        annotation_count: ann_count,
                    });
                    right_lines.push(DisplayLine {
                        kind: DisplayLineKind::Context,
                        content: line.content.clone(),
                        is_selected,
                        staged: hunk.staged,
                        has_annotation: false,
                        annotation_count: 0,
                    });
                }
            }
        }
    }

    let max_visible = rect.height().saturating_sub(1);
    let scroll = review.scroll.scroll_top;

    for i in 0..max_visible {
        let idx = scroll + i;
        let row = rect.row_start + 1 + i;

        if idx < left_lines.len() {
            render_side_line(screen, row, left_rect, &left_lines[idx]);
        }
        if idx < right_lines.len() {
            render_side_line(screen, row, right_rect, &right_lines[idx]);
        }
    }
}

fn render_side_line(screen: &mut ScreenBuffer, row: usize, rect: PanelRect, dl: &DisplayLine) {
    if rect.width() < 1 {
        return;
    }

    match dl.kind {
        DisplayLineKind::HunkHeader => {
            render_hunk_header_line(screen, row, &rect, dl);
        }
        DisplayLineKind::Added => {
            render_diff_line(screen, row, &rect, dl, '+', ADD_BG, ADD_FG);
        }
        DisplayLineKind::Deleted => {
            render_diff_line(screen, row, &rect, dl, '-', DEL_BG, DEL_FG);
        }
        DisplayLineKind::Context => {
            render_diff_line(screen, row, &rect, dl, ' ', Color::Reset, Color::Reset);
        }
    }
}

// ── File preview ────────────────────────────────────────────────────

fn render_file_preview(screen: &mut ScreenBuffer, review: &ReviewState, rect: PanelRect) {
    let file = match review.files.get(review.selected_file) {
        Some(f) => f,
        None => return,
    };

    let header = format!(" Preview: {} ", file.new_path);
    let header_style = CellStyle {
        fg: DIM_FG,
        bold: true,
        ..CellStyle::default()
    };
    set_string_styled(
        screen,
        rect.row_start,
        rect.col_start,
        &header,
        header_style,
    );

    let scroll = review.scroll.scroll_top;
    let max_visible = rect.height().saturating_sub(1);

    // Try to read the actual file from disk
    let lines = read_file_lines(review, &file.new_path);

    if !lines.is_empty() {
        // Render actual file content
        for i in 0..max_visible {
            let idx = scroll + i;
            if idx >= lines.len() {
                break;
            }
            let row = rect.row_start + 1 + i;
            let truncated = truncate(&lines[idx], rect.width());
            let style = CellStyle {
                fg: Color::AnsiValue(255),
                ..CellStyle::default()
            };
            set_string_styled(screen, row, rect.col_start, &truncated, style);
        }

        // Show line count
        let count_text = format!(" {} lines ", lines.len());
        let count_style = CellStyle {
            fg: DIM_FG,
            ..CellStyle::default()
        };
        let count_row = rect.row_end.saturating_sub(1);
        set_string_styled(screen, count_row, rect.col_start, &count_text, count_style);
    } else {
        // Fallback: show diff lines as pseudo-file
        let mut all_lines: Vec<String> = Vec::new();
        for hunk in &file.hunks {
            for line in &hunk.lines {
                let clean = line.content.trim_start_matches('+').trim_start_matches('-');
                all_lines.push(clean.to_string());
            }
        }
        for i in 0..max_visible {
            let idx = scroll + i;
            if idx >= all_lines.len() {
                break;
            }
            let row = rect.row_start + 1 + i;
            let truncated = truncate(&all_lines[idx], rect.width());
            set_string_styled(
                screen,
                row,
                rect.col_start,
                &truncated,
                CellStyle::default(),
            );
        }
    }
}

// ── Help overlay ────────────────────────────────────────────────────

fn render_help_overlay(screen: &mut ScreenBuffer, rows: usize, cols: usize) {
    let help_lines = vec![
        " ╔══════════════════════════════════════════╗ ",
        " ║         Review Mode Help                 ║ ",
        " ╠══════════════════════════════════════════╣ ",
        " ║  q   Exit review mode                    ║ ",
        " ║  ?   Toggle this help                    ║ ",
        " ║  Esc Back to previous panel / exit       ║ ",
        " ║                                          ║ ",
        " ║  File List:                              ║ ",
        " ║    j/k  Navigate files                   ║ ",
        " ║    Enter Open diff view                  ║ ",
        " ║                                          ║ ",
        " ║  Diff View:                              ║ ",
        " ║    j/k  Scroll lines                     ║ ",
        " ║    [/]  Previous/next hunk               ║ ",
        " ║    Enter Toggle hunk stage (git apply)   ║ ",
        " ║    c    Add comment on current line      ║ ",
        " ║    o/p   Open file preview               ║ ",
        " ║    d    Toggle diff style (unified/sbs)  ║ ",
        " ║                                          ║ ",
        " ║  Diff Style:                             ║ ",
        " ║    Unified / Side-by-Side (press d)      ║ ",
        " ╚══════════════════════════════════════════╝ ",
    ];

    let box_width = help_lines.iter().map(|l| l.len()).max().unwrap_or(44);
    let start_row = (rows.saturating_sub(help_lines.len())) / 2;
    let start_col = (cols.saturating_sub(box_width)) / 2;

    // Dim background behind help
    let dim_style = CellStyle {
        bg: Color::AnsiValue(234),
        ..CellStyle::default()
    };
    for r in start_row..start_row + help_lines.len() {
        for c in start_col..start_col + box_width {
            screen.set_cell(r, c, ' ', dim_style);
        }
    }

    for (i, line) in help_lines.iter().enumerate() {
        let row = start_row + i;
        for (j, ch) in line.chars().enumerate() {
            let col = start_col + j;
            if col < cols {
                let fg = if ch == '║'
                    || ch == '╔'
                    || ch == '╗'
                    || ch == '╚'
                    || ch == '╝'
                    || ch == '╠'
                    || ch == '╣'
                {
                    Color::AnsiValue(33)
                } else if ch == 'q'
                    || ch == '?'
                    || ch == 'j'
                    || ch == 'k'
                    || ch == '['
                    || ch == ']'
                    || ch == 'd'
                {
                    Color::AnsiValue(11)
                } else {
                    Color::AnsiValue(255)
                };
                let style = CellStyle {
                    fg,
                    bg: Color::AnsiValue(234),
                    ..CellStyle::default()
                };
                screen.set_cell(row, col, ch, style);
            }
        }
    }
}

// ── Comment Input mini-buffer ───────────────────────────────────────

fn render_comment_input(
    screen: &mut ScreenBuffer,
    review: &ReviewState,
    status_row: usize,
    cols: usize,
) {
    // Render a mini-buffer line just above the status bar
    let input_row = status_row.saturating_sub(1);

    // Clear the line
    for c in 0..cols {
        screen.set_cell(
            input_row,
            c,
            ' ',
            CellStyle {
                bg: Color::AnsiValue(17),
                ..CellStyle::default()
            },
        );
    }

    // Label
    let label = " Comment: ";
    let label_style = CellStyle {
        fg: Color::AnsiValue(11),
        bg: Color::AnsiValue(17),
        bold: true,
        ..CellStyle::default()
    };
    let mut col = 0usize;
    for ch in label.chars() {
        if col < cols {
            screen.set_cell(input_row, col, ch, label_style);
            col += 1;
        }
    }

    // Buffer content with cursor
    let buffer_style = CellStyle {
        fg: Color::White,
        bg: Color::AnsiValue(17),
        ..CellStyle::default()
    };
    let cursor_style = CellStyle {
        fg: Color::Black,
        bg: Color::AnsiValue(11),
        ..CellStyle::default()
    };

    let max_input_cols = cols.saturating_sub(col + 2);
    let display_text: String = review.comment_buffer.chars().take(max_input_cols).collect();
    let cursor_display_pos = {
        let buf_before_cursor = &review.comment_buffer[..review.comment_cursor];
        let chars_before = buf_before_cursor.chars().count();
        chars_before.min(max_input_cols)
    };

    for (i, ch) in display_text.chars().enumerate() {
        if col + i >= cols {
            break;
        }
        if i == cursor_display_pos {
            screen.set_cell(input_row, col + i, ch, cursor_style);
        } else {
            screen.set_cell(input_row, col + i, ch, buffer_style);
        }
    }

    // Cursor at end if past buffer
    if cursor_display_pos >= display_text.chars().count() && col + cursor_display_pos < cols {
        screen.set_cell(input_row, col + cursor_display_pos, ' ', cursor_style);
    }

    // Hint text on the right
    let hint = " Esc:cancel Enter:save ";
    let hint_style = CellStyle {
        fg: Color::AnsiValue(245),
        bg: Color::AnsiValue(17),
        italic: true,
        ..CellStyle::default()
    };
    let hint_col = cols.saturating_sub(hint.len());
    for (i, ch) in hint.chars().enumerate() {
        let c = hint_col + i;
        if c < cols {
            screen.set_cell(input_row, c, ch, hint_style);
        }
    }
}

// ── Status bar ──────────────────────────────────────────────────────

fn render_status_bar(screen: &mut ScreenBuffer, review: &ReviewState, row: usize, cols: usize) {
    // Background for entire status row
    for c in 0..cols {
        screen.set_cell(
            row,
            c,
            ' ',
            CellStyle {
                bg: STATUS_BG,
                ..CellStyle::default()
            },
        );
    }

    // Mode indicator
    let mode_text = " REVIEW ";
    let mode_style = CellStyle {
        fg: Color::White,
        bg: MODE_BG,
        bold: true,
        ..CellStyle::default()
    };
    for (i, ch) in mode_text.chars().enumerate() {
        if i < cols {
            screen.set_cell(row, i, ch, mode_style);
        }
    }

    // Sub-mode indicator
    let sub_mode_text = match review.sub_mode {
        ReviewSubMode::FileList => " [files] ",
        ReviewSubMode::DiffView => " [diff] ",
        ReviewSubMode::Preview => " [preview] ",
        ReviewSubMode::CommentInput => " [comment] ",
        ReviewSubMode::Help => " [help] ",
    };
    let sub_mode_style = CellStyle {
        fg: DIM_FG,
        bg: STATUS_BG,
        ..CellStyle::default()
    };
    let sub_mode_start = mode_text.len() + 1;
    for (i, ch) in sub_mode_text.chars().enumerate() {
        let col = sub_mode_start + i;
        if col < cols {
            screen.set_cell(row, col, ch, sub_mode_style);
        }
    }

    // Info text
    let file = review.files.get(review.selected_file);
    let file_name = file.map(|f| f.new_path.as_str()).unwrap_or("");
    let changes = review.total_changes();
    let ann_count = review.annotations.len();
    let ann_text = if ann_count > 0 {
        format!(" {} annots ", ann_count)
    } else {
        String::new()
    };
    let info_text = format!(
        "{}{}  |  {} changes  |  {}",
        file_name, ann_text, changes, review.args,
    );
    let info_style = CellStyle {
        fg: Color::White,
        bg: STATUS_BG,
        ..CellStyle::default()
    };
    let info_start = sub_mode_start + sub_mode_text.len();
    for (i, ch) in info_text.chars().enumerate() {
        let col = info_start + i;
        if col < cols {
            screen.set_cell(row, col, ch, info_style);
        }
    }

    // Diff style indicator on the right
    let style_text = match review.diff_style {
        DiffStyle::Unified => " unified ",
        DiffStyle::SideBySide => " side-by-side ",
    };
    let style_start = cols.saturating_sub(style_text.len());
    let style_style = CellStyle {
        fg: DIM_FG,
        bg: STATUS_BG,
        ..CellStyle::default()
    };
    for (i, ch) in style_text.chars().enumerate() {
        let col = style_start + i;
        if col < cols {
            screen.set_cell(row, col, ch, style_style);
        }
    }
}

// ── Helper types and functions ──────────────────────────────────────

#[derive(Debug)]
enum DisplayLineKind {
    HunkHeader,
    Added,
    Deleted,
    Context,
}

#[derive(Debug)]
struct DisplayLine {
    kind: DisplayLineKind,
    content: String,
    is_selected: bool,
    /// Whether the hunk this line belongs to is staged
    staged: bool,
    /// Whether this line has unresolved annotations
    has_annotation: bool,
    /// Number of unresolved annotations on this line
    annotation_count: usize,
}

fn status_color(status: FileStatus) -> Color {
    match status {
        FileStatus::Added => STATUS_A,
        FileStatus::Modified => STATUS_M,
        FileStatus::Deleted => STATUS_D,
        FileStatus::Renamed => STATUS_R,
    }
}

fn short_filename(path: &str, max_width: usize) -> String {
    if path.len() <= max_width {
        return path.to_string();
    }
    if max_width < 5 {
        return path.chars().take(max_width).collect();
    }
    let take = max_width.saturating_sub(3);
    let mut result: String = path
        .chars()
        .rev()
        .take(take)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    result = format!("...{}", result);
    result
}

/// Display width of a string (using char count as approximation for now)
fn display_width(s: &str) -> usize {
    s.chars().count()
}

/// Truncate a string to `max_width` display characters.
fn truncate(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_width {
        s.to_string()
    } else {
        chars.iter().take(max_width).collect()
    }
}

/// Write a plain string at a given position.
fn set_string_styled(
    screen: &mut ScreenBuffer,
    row: usize,
    col: usize,
    text: &str,
    style: CellStyle,
) {
    for (i, ch) in text.chars().enumerate() {
        screen.set_cell(row, col + i, ch, style);
    }
}

/// Fill from `start_col` to `end_col` with spaces using the given style.
fn fill_to_end(
    screen: &mut ScreenBuffer,
    row: usize,
    start_col: usize,
    end_col: usize,
    style: CellStyle,
) {
    for c in start_col..end_col {
        screen.set_cell(row, c, ' ', style);
    }
}

/// Read a file from disk, using `project_root` to resolve the path.
/// Returns an empty vec on error (file not found, not in git repo, etc.).
fn read_file_lines(review: &ReviewState, file_path: &str) -> Vec<String> {
    let base = match &review.project_root {
        Some(root) => root.clone(),
        None => return Vec::new(),
    };
    let full_path: PathBuf = [base, PathBuf::from(file_path)].iter().collect();
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content.lines().map(|l| l.to_string()).collect()
}

// ── Shared diff rendering helpers ───────────────────────────────────
//
// These eliminate the 300+ line duplication between unified and
// side-by-side diff rendering. Each function handles one aspect of
// the 4 DisplayLineKind variants.

/// Build a set of line numbers with unresolved annotations for a file.
fn annotated_lines_for_file(review: &ReviewState, file_path: &str) -> HashSet<usize> {
    review
        .annotations
        .iter()
        .filter(|a| a.file_path == file_path && !a.resolved)
        .map(|a| a.line)
        .collect()
}

/// Render a hunk header line (with staged indicator + annotation suffix).
fn render_hunk_header_line(
    screen: &mut ScreenBuffer,
    row: usize,
    rect: &PanelRect,
    dl: &DisplayLine,
) {
    let staged = if dl.staged { "[S] " } else { "[ ] " };
    let ann_suffix = if dl.has_annotation {
        format!(" [{}A]", dl.annotation_count)
    } else {
        String::new()
    };
    let full = format!("{}{}{}", staged, dl.content, ann_suffix);
    let fg = if dl.has_annotation {
        ANNOTATION_FG
    } else {
        HUNK_HEADER_FG
    };
    let bg = if dl.is_selected {
        SELECTED_BG
    } else {
        Color::Reset
    };
    let style = CellStyle {
        fg,
        bg,
        bold: true,
        ..CellStyle::default()
    };
    let truncated = truncate(&full, rect.width());
    set_string_styled(screen, row, rect.col_start, &truncated, style);
    fill_to_end(
        screen,
        row,
        rect.col_start + display_width(&truncated),
        rect.col_end,
        style,
    );
}

/// Render a diff content line (Added/Deleted/Context).
///
/// `prefix_char` is the diff marker ('+', '-', or ' ') displayed before
/// the annotation marker.  `bg` is the normal (non-selected) background.
/// `prefix_fg` is the colour for the prefix character.
fn render_diff_line(
    screen: &mut ScreenBuffer,
    row: usize,
    rect: &PanelRect,
    dl: &DisplayLine,
    prefix_char: char,
    bg: Color,
    prefix_fg: Color,
) {
    let actual_bg = if dl.is_selected { SELECTED_BG } else { bg };

    let mut col = rect.col_start;

    // Prefix character ('+', '-', ' ')
    screen.set_cell(
        row,
        col,
        prefix_char,
        CellStyle {
            fg: prefix_fg,
            bg: actual_bg,
            bold: true,
            ..CellStyle::default()
        },
    );
    col += 1;

    // Annotation marker
    if dl.has_annotation {
        let marker = format!("[{}A] ", dl.annotation_count);
        let marker_w = display_width(&marker);
        let marker_end = col + marker_w;
        if marker_end <= rect.col_end {
            set_string_styled(
                screen,
                row,
                col,
                &marker,
                CellStyle {
                    fg: ANNOTATION_FG,
                    bg: actual_bg,
                    bold: true,
                    ..CellStyle::default()
                },
            );
            col = marker_end;
        }
    }

    // Content
    if !dl.content.is_empty() {
        let remaining = rect.width().saturating_sub(col - rect.col_start);
        let truncated = truncate(&dl.content, remaining);
        let content_end = col + display_width(&truncated);
        set_string_styled(
            screen,
            row,
            col,
            &truncated,
            CellStyle {
                bg: actual_bg,
                ..CellStyle::default()
            },
        );
        fill_to_end(
            screen,
            row,
            content_end,
            rect.col_end,
            CellStyle {
                bg: actual_bg,
                ..CellStyle::default()
            },
        );
    } else {
        fill_to_end(
            screen,
            row,
            col,
            rect.col_end,
            CellStyle {
                bg: actual_bg,
                ..CellStyle::default()
            },
        );
    }
}

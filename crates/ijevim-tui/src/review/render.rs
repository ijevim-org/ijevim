use crate::review::layout::{compute_review_layout, PanelRect};
use crate::review::state::{
    DiffFile, DiffRow, DiffRowKind, DiffStyle, FileStatus, ReviewState, ReviewSubMode,
};
use crate::screen::{CellStyle, ScreenBuffer};
use crossterm::style::Color;
use std::collections::HashMap;
use std::path::PathBuf;

// ── Colour palette for review mode ──────────────────────────────────

/// Dark green background for added lines
const ADD_BG: Color = Color::AnsiValue(22);
/// Green background for the current added line
const ADD_SEL_BG: Color = Color::AnsiValue(28);
/// Green foreground for `+` marker
const ADD_FG: Color = Color::AnsiValue(10);
/// Dark red background for deleted lines
const DEL_BG: Color = Color::AnsiValue(52);
/// Red background for the current deleted line
const DEL_SEL_BG: Color = Color::AnsiValue(88);
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
/// Separator between panels
const SEPARATOR_FG: Color = Color::AnsiValue(240);
/// Yellow foreground for annotation markers
const ANNOTATION_FG: Color = Color::AnsiValue(11);
/// Line-number gutter foreground
const LINENO_FG: Color = Color::AnsiValue(243);
/// Overlay background
const OVERLAY_BG: Color = Color::AnsiValue(234);
/// Comment input background
const COMMENT_BG: Color = Color::AnsiValue(17);

// ── Main entry point ────────────────────────────────────────────────

/// Render the review mode UI into the screen buffer.
pub fn render_review(screen: &mut ScreenBuffer, review: &ReviewState, rows: usize, cols: usize) {
    let layout = compute_review_layout(rows, cols, review.file_list_width);

    for r in 0..layout.status_row {
        for c in 0..cols {
            screen.set_cell(r, c, ' ', CellStyle::default());
        }
    }

    match review.sub_mode {
        ReviewSubMode::FileList | ReviewSubMode::DiffView => {
            render_file_list(screen, review, layout.file_list);
            render_separator(screen, layout.file_list.col_end, layout.status_row);
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
            render_background(screen, review, &layout);
            render_help_overlay(screen, rows, cols);
        }
        ReviewSubMode::CommentInput => {
            render_background(screen, review, &layout);
            render_comment_input(screen, review, layout.status_row, cols);
        }
        ReviewSubMode::Annotations => {
            render_background(screen, review, &layout);
            render_annotations_overlay(screen, review, rows, cols);
        }
    }

    render_status_bar(screen, review, layout.status_row, cols);
}

fn render_background(
    screen: &mut ScreenBuffer,
    review: &ReviewState,
    layout: &crate::review::layout::ReviewLayout,
) {
    render_file_list(screen, review, layout.file_list);
    render_separator(screen, layout.file_list.col_end, layout.status_row);
    render_diff_view(screen, review, layout.diff_view);
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

    for (i, ch) in " Files ".chars().enumerate() {
        if i < rect.width() {
            screen.set_cell(rect.row_start, rect.col_start + i, ch, title_style);
        }
    }

    for c in rect.col_start..rect.col_end {
        screen.set_cell(
            rect.row_start + 1,
            c,
            '─',
            CellStyle {
                fg: SEPARATOR_FG,
                ..CellStyle::default()
            },
        );
    }

    if review.files.is_empty() {
        let empty_row = rect.row_start + 2;
        let empty_style = CellStyle {
            fg: DIM_FG,
            ..CellStyle::default()
        };
        set_string_styled(
            screen,
            empty_row,
            rect.col_start,
            " No changes to review ",
            empty_style,
        );
        set_string_styled(
            screen,
            empty_row + 1,
            rect.col_start,
            " :review --staged for staged changes ",
            CellStyle {
                fg: Color::AnsiValue(240),
                ..CellStyle::default()
            },
        );
        return;
    }

    let max_visible = rect.height().saturating_sub(3);
    for i in 0..review.files.len().min(max_visible) {
        let row = rect.row_start + 2 + i;
        let file = &review.files[i];
        let is_selected = i == review.selected_file;

        if is_selected {
            for c in rect.col_start..rect.col_end {
                screen.set_cell(row, c, ' ', selected_style);
            }
        }

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

        let name_start = rect.col_start + 2;
        let name_width = rect.width().saturating_sub(2);
        let display_name = short_filename(file.display_path(), name_width);
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

        if is_selected {
            let fill_start = name_start + display_name.chars().count();
            for c in fill_start..rect.col_end {
                screen.set_cell(row, c, ' ', selected_style);
            }
        }
    }

    let count_row = rect.row_end.saturating_sub(1);
    let count_text = format!(" {}/{} ", review.selected_file + 1, review.files.len());
    set_string_styled(
        screen,
        count_row,
        rect.col_start,
        &count_text,
        CellStyle {
            fg: DIM_FG,
            ..CellStyle::default()
        },
    );
}

// ── Separator ───────────────────────────────────────────────────────

fn render_separator(screen: &mut ScreenBuffer, col: usize, max_row: usize) {
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
    let header_style = CellStyle {
        fg: DIM_FG,
        ..CellStyle::default()
    };
    set_string_styled(
        screen,
        rect.row_start,
        rect.col_start,
        &format!("--- a/{}", file.old_path),
        header_style,
    );
    set_string_styled(
        screen,
        rect.row_start + 1,
        rect.col_start,
        &format!("+++ b/{}", file.new_path),
        header_style,
    );

    let rows = review.rows_for(review.selected_file);
    if rows.is_empty() {
        set_string_styled(
            screen,
            rect.row_start + 2,
            rect.col_start,
            " (no changes) ",
            header_style,
        );
        return;
    }

    let counts = annotation_counts(review, file.display_path());
    let lineno_width = line_number_width(&rows);
    let max_visible = rect.height().saturating_sub(2);
    let scroll = effective_scroll(review, rows.len(), max_visible);

    for i in 0..max_visible {
        let idx = scroll + i;
        let Some(dl) = rows.get(idx) else {
            break;
        };
        let row = rect.row_start + 2 + i;
        render_unified_row(
            screen,
            row,
            &rect,
            dl,
            idx == review.cursor_row,
            lineno_width,
            counts
                .get(&dl.comment_line().unwrap_or(0))
                .copied()
                .unwrap_or(0),
        );
    }
}

fn render_unified_row(
    screen: &mut ScreenBuffer,
    row: usize,
    rect: &PanelRect,
    dl: &DiffRow,
    is_cursor: bool,
    lineno_width: usize,
    annotation_count: usize,
) {
    match dl.kind {
        DiffRowKind::HunkHeader => {
            let mut full = format!("{}{}", if dl.staged { "[S] " } else { "[ ] " }, dl.content);
            if annotation_count > 0 {
                full.push_str(&format!(" [{}A]", annotation_count));
            }
            let style = CellStyle {
                fg: if annotation_count > 0 {
                    ANNOTATION_FG
                } else {
                    HUNK_HEADER_FG
                },
                bg: if is_cursor { SELECTED_BG } else { Color::Reset },
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
        DiffRowKind::Add | DiffRowKind::Delete | DiffRowKind::Context => {
            let (prefix, normal_bg, sel_bg, prefix_fg) = match dl.kind {
                DiffRowKind::Add => ('+', ADD_BG, ADD_SEL_BG, ADD_FG),
                DiffRowKind::Delete => ('-', DEL_BG, DEL_SEL_BG, DEL_FG),
                _ => (' ', Color::Reset, SELECTED_BG, Color::Reset),
            };
            let bg = if is_cursor { sel_bg } else { normal_bg };

            let old = format_lineno(dl.old_lineno, lineno_width);
            let new = format_lineno(dl.new_lineno, lineno_width);
            let gutter = format!("{} {} │ ", old, new);
            set_string_styled(
                screen,
                row,
                rect.col_start,
                &gutter,
                CellStyle {
                    fg: LINENO_FG,
                    bg,
                    ..CellStyle::default()
                },
            );

            let mut col = rect.col_start + display_width(&gutter);
            screen.set_cell(
                row,
                col,
                prefix,
                CellStyle {
                    fg: prefix_fg,
                    bg,
                    bold: true,
                    ..CellStyle::default()
                },
            );
            col += 1;

            if annotation_count > 0 {
                let marker = format!("[{}A] ", annotation_count);
                let marker_end = col + display_width(&marker);
                if marker_end <= rect.col_end {
                    set_string_styled(
                        screen,
                        row,
                        col,
                        &marker,
                        CellStyle {
                            fg: ANNOTATION_FG,
                            bg,
                            bold: true,
                            ..CellStyle::default()
                        },
                    );
                    col = marker_end;
                }
            }

            let remaining = rect.col_end.saturating_sub(col);
            let truncated = truncate(&dl.content, remaining);
            let content_end = col + display_width(&truncated);
            set_string_styled(
                screen,
                row,
                col,
                &truncated,
                CellStyle {
                    bg,
                    bold: is_cursor,
                    ..CellStyle::default()
                },
            );
            fill_to_end(
                screen,
                row,
                content_end,
                rect.col_end,
                CellStyle {
                    bg,
                    ..CellStyle::default()
                },
            );
        }
    }
}

// ── Side-by-side diff rendering ─────────────────────────────────────

fn render_side_by_side_diff(
    screen: &mut ScreenBuffer,
    review: &ReviewState,
    file: &DiffFile,
    rect: PanelRect,
) {
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
    for r in rect.row_start..rect.row_end {
        screen.set_cell(r, left_rect.col_end, '│', sep_style);
    }

    let rows = review.rows_for(review.selected_file);
    let counts = annotation_counts(review, file.display_path());
    let lineno_width = (line_number_width(&rows) / 2).max(3);
    let max_visible = rect.height().saturating_sub(1);
    let scroll = effective_scroll(review, rows.len(), max_visible);

    for i in 0..max_visible {
        let idx = scroll + i;
        let Some(dl) = rows.get(idx) else {
            break;
        };
        let row = rect.row_start + 1 + i;
        let is_cursor = idx == review.cursor_row;
        let count = counts
            .get(&dl.comment_line().unwrap_or(0))
            .copied()
            .unwrap_or(0);

        match dl.kind {
            DiffRowKind::HunkHeader => {
                let hunk = file.hunks.get(dl.hunk_index);
                let staged = if dl.staged { "[S] " } else { "[ ] " };
                let left = match hunk {
                    Some(h) => format!("{}@@ -{},{} @@", staged, h.old_start, h.old_count),
                    None => dl.content.clone(),
                };
                let right = match hunk {
                    Some(h) => format!("{}@@ +{},{} @@", staged, h.new_start, h.new_count),
                    None => dl.content.clone(),
                };
                render_side_header(screen, row, &left_rect, &left, is_cursor, count);
                render_side_header(screen, row, &right_rect, &right, is_cursor, count);
            }
            DiffRowKind::Add => {
                render_side_blank(screen, row, &left_rect, Color::Reset, is_cursor, false);
                render_side_content(
                    screen,
                    row,
                    &right_rect,
                    dl,
                    '+',
                    ADD_BG,
                    ADD_SEL_BG,
                    ADD_FG,
                    is_cursor,
                    lineno_width,
                    count,
                    true,
                );
            }
            DiffRowKind::Delete => {
                render_side_content(
                    screen,
                    row,
                    &left_rect,
                    dl,
                    '-',
                    DEL_BG,
                    DEL_SEL_BG,
                    DEL_FG,
                    is_cursor,
                    lineno_width,
                    count,
                    false,
                );
                render_side_blank(screen, row, &right_rect, Color::Reset, is_cursor, true);
            }
            DiffRowKind::Context => {
                render_side_content(
                    screen,
                    row,
                    &left_rect,
                    dl,
                    ' ',
                    Color::Reset,
                    SELECTED_BG,
                    Color::Reset,
                    is_cursor,
                    lineno_width,
                    count,
                    false,
                );
                render_side_content(
                    screen,
                    row,
                    &right_rect,
                    dl,
                    ' ',
                    Color::Reset,
                    SELECTED_BG,
                    Color::Reset,
                    is_cursor,
                    lineno_width,
                    count,
                    true,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_side_content(
    screen: &mut ScreenBuffer,
    row: usize,
    rect: &PanelRect,
    dl: &DiffRow,
    prefix: char,
    normal_bg: Color,
    sel_bg: Color,
    prefix_fg: Color,
    is_cursor: bool,
    lineno_width: usize,
    annotation_count: usize,
    new_side: bool,
) {
    let bg = if is_cursor { sel_bg } else { normal_bg };
    let lineno = if new_side {
        dl.new_lineno
    } else {
        dl.old_lineno
    };
    let gutter = format!("{} │ ", format_lineno(lineno, lineno_width));
    set_string_styled(
        screen,
        row,
        rect.col_start,
        &gutter,
        CellStyle {
            fg: LINENO_FG,
            bg,
            ..CellStyle::default()
        },
    );

    let mut col = rect.col_start + display_width(&gutter);
    screen.set_cell(
        row,
        col,
        prefix,
        CellStyle {
            fg: prefix_fg,
            bg,
            bold: true,
            ..CellStyle::default()
        },
    );
    col += 1;

    if annotation_count > 0 {
        let marker = format!("[{}A] ", annotation_count);
        if col + display_width(&marker) <= rect.col_end {
            set_string_styled(
                screen,
                row,
                col,
                &marker,
                CellStyle {
                    fg: ANNOTATION_FG,
                    bg,
                    bold: true,
                    ..CellStyle::default()
                },
            );
            col += display_width(&marker);
        }
    }

    let remaining = rect.col_end.saturating_sub(col);
    let truncated = truncate(&dl.content, remaining);
    let end = col + display_width(&truncated);
    set_string_styled(
        screen,
        row,
        col,
        &truncated,
        CellStyle {
            bg,
            bold: is_cursor,
            ..CellStyle::default()
        },
    );
    fill_to_end(
        screen,
        row,
        end,
        rect.col_end,
        CellStyle {
            bg,
            ..CellStyle::default()
        },
    );
}

fn render_side_header(
    screen: &mut ScreenBuffer,
    row: usize,
    rect: &PanelRect,
    text: &str,
    is_cursor: bool,
    annotation_count: usize,
) {
    let style = CellStyle {
        fg: if annotation_count > 0 {
            ANNOTATION_FG
        } else {
            HUNK_HEADER_FG
        },
        bg: if is_cursor { SELECTED_BG } else { Color::Reset },
        bold: true,
        ..CellStyle::default()
    };
    let truncated = truncate(text, rect.width());
    set_string_styled(screen, row, rect.col_start, &truncated, style);
    fill_to_end(
        screen,
        row,
        rect.col_start + display_width(&truncated),
        rect.col_end,
        style,
    );
}

fn render_side_blank(
    screen: &mut ScreenBuffer,
    row: usize,
    rect: &PanelRect,
    bg: Color,
    is_cursor: bool,
    show_gutter: bool,
) {
    let bg = if is_cursor { SELECTED_BG } else { bg };
    let style = CellStyle {
        bg,
        ..CellStyle::default()
    };
    if show_gutter {
        let gutter = format!("{} │ ", " ".repeat(3));
        set_string_styled(screen, row, rect.col_start, &gutter, style);
    }
    fill_to_end(screen, row, rect.col_start, rect.col_end, style);
}

// ── File preview ────────────────────────────────────────────────────

fn render_file_preview(screen: &mut ScreenBuffer, review: &ReviewState, rect: PanelRect) {
    let file = match review.files.get(review.selected_file) {
        Some(f) => f,
        None => return,
    };

    let header = format!(" Preview: {} ", file.display_path());
    set_string_styled(
        screen,
        rect.row_start,
        rect.col_start,
        &header,
        CellStyle {
            fg: DIM_FG,
            bold: true,
            ..CellStyle::default()
        },
    );

    let scroll = review.scroll.scroll_top;
    let max_visible = rect.height().saturating_sub(1);
    let lines = read_file_lines(review, file.display_path());

    if !lines.is_empty() {
        for i in 0..max_visible {
            let idx = scroll + i;
            let Some(line) = lines.get(idx) else {
                break;
            };
            let row = rect.row_start + 1 + i;
            let truncated = truncate(line, rect.width());
            set_string_styled(
                screen,
                row,
                rect.col_start,
                &truncated,
                CellStyle {
                    fg: Color::AnsiValue(255),
                    ..CellStyle::default()
                },
            );
        }

        set_string_styled(
            screen,
            rect.row_end.saturating_sub(1),
            rect.col_start,
            &format!(" {} lines ", lines.len()),
            CellStyle {
                fg: DIM_FG,
                ..CellStyle::default()
            },
        );
    } else {
        let mut all_lines: Vec<String> = Vec::new();
        for hunk in &file.hunks {
            for line in &hunk.lines {
                all_lines.push(line.content.clone());
            }
        }
        for i in 0..max_visible {
            let idx = scroll + i;
            let Some(line) = all_lines.get(idx) else {
                break;
            };
            let row = rect.row_start + 1 + i;
            let truncated = truncate(line, rect.width());
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
        " ╔══════════════════════════════════════════════╗ ",
        " ║            Review Mode Help                  ║ ",
        " ╠══════════════════════════════════════════════╣ ",
        " ║  q     Exit review mode                      ║ ",
        " ║  ?     Toggle this help                      ║ ",
        " ║  Esc   Back to previous panel                ║ ",
        " ║                                              ║ ",
        " ║  File List:                                  ║ ",
        " ║    j/k    Navigate files                     ║ ",
        " ║    Enter  Open diff view                     ║ ",
        " ║    a      Browse annotations                 ║ ",
        " ║                                              ║ ",
        " ║  Diff View:                                  ║ ",
        " ║    j/k    Move cursor                        ║ ",
        " ║    C-d/u  Half page down/up                  ║ ",
        " ║    g/G    First/last row                     ║ ",
        " ║    [/]    Previous/next hunk                 ║ ",
        " ║    n/p    Next/previous annotation           ║ ",
        " ║    Enter  Toggle hunk stage (git apply)      ║ ",
        " ║    c      Comment on the current line        ║ ",
        " ║    a      Browse annotations                 ║ ",
        " ║    o      Open file preview                  ║ ",
        " ║    d      Toggle diff style                  ║ ",
        " ║                                              ║ ",
        " ║  Comment input:                              ║ ",
        " ║    Enter  New line                           ║ ",
        " ║    C-s    Save annotation                    ║ ",
        " ║    Esc    Cancel                             ║ ",
        " ╚══════════════════════════════════════════════╝ ",
    ];

    let box_width = help_lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(48);
    let start_row = (rows.saturating_sub(help_lines.len())) / 2;
    let start_col = (cols.saturating_sub(box_width)) / 2;

    for r in start_row..(start_row + help_lines.len()).min(rows) {
        for c in start_col..(start_col + box_width).min(cols) {
            screen.set_cell(
                r,
                c,
                ' ',
                CellStyle {
                    bg: OVERLAY_BG,
                    ..CellStyle::default()
                },
            );
        }
    }

    for (i, line) in help_lines.iter().enumerate() {
        let row = start_row + i;
        if row >= rows {
            break;
        }
        for (j, ch) in line.chars().enumerate() {
            let col = start_col + j;
            if col >= cols {
                break;
            }
            let fg = if '╔' == ch
                || '╗' == ch
                || '╚' == ch
                || '╝' == ch
                || '║' == ch
                || '╠' == ch
                || '╣' == ch
            {
                Color::AnsiValue(33)
            } else if "q?jk[]dacnpgG".contains(ch) {
                Color::AnsiValue(11)
            } else {
                Color::AnsiValue(255)
            };
            screen.set_cell(
                row,
                col,
                ch,
                CellStyle {
                    fg,
                    bg: OVERLAY_BG,
                    ..CellStyle::default()
                },
            );
        }
    }
}

// ── Comment input overlay ───────────────────────────────────────────

fn render_comment_input(
    screen: &mut ScreenBuffer,
    review: &ReviewState,
    status_row: usize,
    cols: usize,
) {
    let box_width = cols.saturating_sub(4).clamp(20, 76);
    let start_col = (cols.saturating_sub(box_width)) / 2;

    let target = review
        .comment_target
        .as_ref()
        .map(|t| format!(" {}:{} ", t.file_path, t.line))
        .unwrap_or_else(|| " comment ".to_string());

    let lines: Vec<&str> = review.comment_buffer.split('\n').collect();
    let content_height = lines.len().clamp(2, 6);
    let box_height = content_height + 4; // title + borders + hint
    let start_row = status_row.saturating_sub(box_height + 1);

    // Box background
    for r in start_row..(start_row + box_height).min(status_row) {
        for c in start_col..(start_col + box_width).min(cols) {
            screen.set_cell(
                r,
                c,
                ' ',
                CellStyle {
                    bg: COMMENT_BG,
                    ..CellStyle::default()
                },
            );
        }
    }

    // Title bar
    let title = format!(" Comment{}", target);
    set_string_styled(
        screen,
        start_row,
        start_col,
        &truncate(&title, box_width),
        CellStyle {
            fg: Color::AnsiValue(11),
            bg: COMMENT_BG,
            bold: true,
            ..CellStyle::default()
        },
    );

    // Compute cursor line/column before the cursor.
    let before = &review.comment_buffer[..review.comment_cursor.min(review.comment_buffer.len())];
    let cursor_line = before.matches('\n').count();
    let cursor_col = before
        .rsplit('\n')
        .next()
        .map(|s| s.chars().count())
        .unwrap_or(0);
    let first_line = cursor_line.saturating_sub(content_height.saturating_sub(1));

    for i in 0..content_height {
        let line_idx = first_line + i;
        let row = start_row + 1 + i;
        if row >= status_row {
            break;
        }
        let text = lines.get(line_idx).copied().unwrap_or("");
        let truncated = truncate(text, box_width.saturating_sub(2));
        set_string_styled(
            screen,
            row,
            start_col + 1,
            &truncated,
            CellStyle {
                fg: Color::White,
                bg: COMMENT_BG,
                ..CellStyle::default()
            },
        );

        if line_idx == cursor_line {
            let col = start_col + 1 + cursor_col.min(box_width.saturating_sub(2));
            if col < cols {
                let ch = truncated.chars().nth(cursor_col).unwrap_or(' ');
                screen.set_cell(
                    row,
                    col,
                    ch,
                    CellStyle {
                        fg: Color::Black,
                        bg: Color::AnsiValue(11),
                        ..CellStyle::default()
                    },
                );
            }
        }
    }

    let hint_row = start_row + box_height - 1;
    let hint = " Enter:newline  C-s:save  Esc:cancel ";
    set_string_styled(
        screen,
        hint_row,
        start_col + 1,
        &truncate(hint, box_width.saturating_sub(2)),
        CellStyle {
            fg: DIM_FG,
            bg: COMMENT_BG,
            italic: true,
            ..CellStyle::default()
        },
    );
}

// ── Annotation browser overlay ──────────────────────────────────────

fn render_annotations_overlay(
    screen: &mut ScreenBuffer,
    review: &ReviewState,
    rows: usize,
    cols: usize,
) {
    let box_width = cols.saturating_sub(2).clamp(24, 100);
    let box_height = (review.annotations.len() + 4).clamp(5, rows.saturating_sub(2).max(5));
    let start_row = (rows.saturating_sub(box_height)) / 2;
    let start_col = (cols.saturating_sub(box_width)) / 2;

    for r in start_row..(start_row + box_height).min(rows) {
        for c in start_col..(start_col + box_width).min(cols) {
            screen.set_cell(
                r,
                c,
                ' ',
                CellStyle {
                    bg: OVERLAY_BG,
                    ..CellStyle::default()
                },
            );
        }
    }

    let title = format!(
        " Annotations ({}) — {}/{} ",
        review.annotations.len(),
        if review.annotations.is_empty() {
            0
        } else {
            review.annotation_selected + 1
        },
        review.annotations.len()
    );
    set_string_styled(
        screen,
        start_row,
        start_col,
        &truncate(&title, box_width),
        CellStyle {
            fg: Color::AnsiValue(11),
            bg: OVERLAY_BG,
            bold: true,
            ..CellStyle::default()
        },
    );

    let list_height = box_height.saturating_sub(3);
    let first = review
        .annotation_selected
        .saturating_sub(list_height.saturating_sub(1));

    if review.annotations.is_empty() {
        set_string_styled(
            screen,
            start_row + 1,
            start_col + 1,
            " No annotations ",
            CellStyle {
                fg: DIM_FG,
                bg: OVERLAY_BG,
                ..CellStyle::default()
            },
        );
    }

    for i in 0..list_height {
        let idx = first + i;
        let Some(ann) = review.annotations.get(idx) else {
            break;
        };
        let row = start_row + 1 + i;
        let is_selected = idx == review.annotation_selected;

        if is_selected {
            for c in start_col..(start_col + box_width).min(cols) {
                screen.set_cell(
                    row,
                    c,
                    ' ',
                    CellStyle {
                        bg: SELECTED_BG,
                        ..CellStyle::default()
                    },
                );
            }
        }

        let mark = if ann.resolved { "[x] " } else { "[ ] " };
        let text = ann.text.replace('\n', " ");
        let line = format!("{}{}:{}  {}", mark, ann.file_path, ann.line, text);
        let fg = if ann.resolved { DIM_FG } else { Color::White };
        set_string_styled(
            screen,
            row,
            start_col + 1,
            &truncate(&line, box_width.saturating_sub(2)),
            CellStyle {
                fg,
                bg: if is_selected { SELECTED_BG } else { OVERLAY_BG },
                ..CellStyle::default()
            },
        );
    }

    let hint = if review.annotation_pending_delete {
        " d again:delete  Esc:cancel "
    } else {
        " Enter:jump  x:resolve  dd:delete  Esc:close "
    };
    set_string_styled(
        screen,
        start_row + box_height - 1,
        start_col + 1,
        &truncate(hint, box_width.saturating_sub(2)),
        CellStyle {
            fg: DIM_FG,
            bg: OVERLAY_BG,
            italic: true,
            ..CellStyle::default()
        },
    );
}

// ── Status bar ──────────────────────────────────────────────────────

fn render_status_bar(screen: &mut ScreenBuffer, review: &ReviewState, row: usize, cols: usize) {
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

    let sub_mode_text = match review.sub_mode {
        ReviewSubMode::FileList => " [files] ",
        ReviewSubMode::DiffView => " [diff] ",
        ReviewSubMode::Preview => " [preview] ",
        ReviewSubMode::CommentInput => " [comment] ",
        ReviewSubMode::Annotations => " [annots] ",
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

    let file = review.files.get(review.selected_file);
    let file_name = file.map(|f| f.display_path()).unwrap_or("");
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
    let info_start = sub_mode_start + sub_mode_text.len();
    for (i, ch) in info_text.chars().enumerate() {
        let col = info_start + i;
        if col >= cols {
            break;
        }
        screen.set_cell(
            row,
            col,
            ch,
            CellStyle {
                fg: Color::White,
                bg: STATUS_BG,
                ..CellStyle::default()
            },
        );
    }

    let style_text = match review.diff_style {
        DiffStyle::Unified => " unified ",
        DiffStyle::SideBySide => " side-by-side ",
    };
    let style_start = cols.saturating_sub(style_text.len());
    for (i, ch) in style_text.chars().enumerate() {
        let col = style_start + i;
        if col < cols {
            screen.set_cell(
                row,
                col,
                ch,
                CellStyle {
                    fg: DIM_FG,
                    bg: STATUS_BG,
                    ..CellStyle::default()
                },
            );
        }
    }
}

// ── Helper functions ────────────────────────────────────────────────

fn status_color(status: FileStatus) -> Color {
    match status {
        FileStatus::Added => STATUS_A,
        FileStatus::Modified => STATUS_M,
        FileStatus::Deleted => STATUS_D,
        FileStatus::Renamed => STATUS_R,
    }
}

fn short_filename(path: &str, max_width: usize) -> String {
    if path.chars().count() <= max_width {
        return path.to_string();
    }
    if max_width < 5 {
        return path.chars().take(max_width).collect();
    }
    let take = max_width.saturating_sub(3);
    let tail: String = path
        .chars()
        .rev()
        .take(take)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{}", tail)
}

/// Display width of a string (char count is a good enough approximation here).
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
        chars.into_iter().take(max_width).collect()
    }
}

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

/// Width of the shared line-number gutter, based on the largest line number.
fn line_number_width(rows: &[DiffRow]) -> usize {
    let max = rows
        .iter()
        .flat_map(|row| [row.old_lineno, row.new_lineno])
        .flatten()
        .max()
        .unwrap_or(1);
    max.to_string().len().max(3)
}

fn format_lineno(lineno: Option<usize>, width: usize) -> String {
    match lineno {
        Some(n) => format!("{:>width$}", n, width = width),
        None => " ".repeat(width),
    }
}

/// Number of unresolved annotations per line for a file.
fn annotation_counts(review: &ReviewState, file_path: &str) -> HashMap<usize, usize> {
    let mut counts = HashMap::new();
    for annotation in &review.annotations {
        if annotation.file_path == file_path && !annotation.resolved {
            *counts.entry(annotation.line).or_insert(0) += 1;
        }
    }
    counts
}

/// Scroll position that keeps the cursor visible even if the stored value is
/// stale after a resize or a jump.
fn effective_scroll(review: &ReviewState, total: usize, visible: usize) -> usize {
    if total == 0 || visible == 0 {
        return 0;
    }
    let mut scroll = review.scroll.scroll_top.min(total.saturating_sub(visible));
    if review.cursor_row < scroll {
        scroll = review.cursor_row;
    } else if review.cursor_row >= scroll + visible {
        scroll = review.cursor_row + 1 - visible;
    }
    scroll
}

/// Read a file from disk, using `project_root` to resolve the path.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::state::{DiffLine, DiffLineKind, Hunk};

    fn sample_review() -> ReviewState {
        ReviewState {
            args: crate::review::ReviewArgs::WorkingTree,
            files: vec![DiffFile {
                status: FileStatus::Modified,
                old_path: "src/main.rs".to_string(),
                new_path: "src/main.rs".to_string(),
                hunks: vec![Hunk {
                    old_start: 1,
                    old_count: 1,
                    new_start: 1,
                    new_count: 2,
                    header: "@@ -1,1 +1,2 @@".to_string(),
                    staged: false,
                    lines: vec![
                        DiffLine {
                            kind: DiffLineKind::Context,
                            content: "keep".to_string(),
                            old_lineno: Some(1),
                            new_lineno: Some(1),
                        },
                        DiffLine {
                            kind: DiffLineKind::Add,
                            content: "added".to_string(),
                            old_lineno: None,
                            new_lineno: Some(2),
                        },
                    ],
                }],
            }],
            selected_file: 0,
            selected_hunk: 0,
            sub_mode: ReviewSubMode::DiffView,
            cursor_row: 2,
            annotation_selected: 0,
            annotation_pending_delete: false,
            comment_target: None,
            annotations: Vec::new(),
            comments: Vec::new(),
            scroll: crate::review::ScrollState::default(),
            viewport_height: 10,
            diff_style: DiffStyle::Unified,
            file_list_width: 30,
            project_root: None,
            comment_buffer: String::new(),
            comment_cursor: 0,
        }
    }

    #[test]
    fn effective_scroll_keeps_cursor_visible() {
        let mut review = sample_review();
        review.scroll.scroll_top = 0;
        review.cursor_row = 2;
        assert_eq!(effective_scroll(&review, 3, 2), 1);
        review.scroll.scroll_top = 99;
        assert_eq!(effective_scroll(&review, 3, 2), 1);
    }

    #[test]
    fn unified_render_writes_line_numbers_and_cursor() {
        let review = sample_review();
        let mut screen = ScreenBuffer::new(12, 80);
        render_review(&mut screen, &review, 12, 80);

        let text = screen.render();
        assert!(text.contains("added"), "diff content missing: {text}");
        assert!(text.contains('│'), "line-number gutter missing: {text}");
    }
}

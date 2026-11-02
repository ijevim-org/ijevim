use crossterm::style::Color;
use ijevim_highlight::Style;
use std::fmt::Write;

/// Per-cell styling — lightweight, Copy-friendly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellStyle {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: Color::Reset,
            bg: Color::Reset,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

impl CellStyle {
    /// Append ANSI escape sequences to switch to this style.
    /// Always starts with a full reset (`\x1b[0m`) for correctness.
    pub fn write_ansi(&self, output: &mut String) {
        output.push_str("\x1b[0m");

        if self.bold {
            output.push_str("\x1b[1m");
        }
        if self.italic {
            output.push_str("\x1b[3m");
        }
        if self.underline {
            output.push_str("\x1b[4m");
        }

        match self.fg {
            Color::Reset => output.push_str("\x1b[39m"),
            Color::Black => output.push_str("\x1b[30m"),
            Color::DarkGrey | Color::Grey => output.push_str("\x1b[90m"),
            Color::Red | Color::DarkRed => output.push_str("\x1b[31m"),
            Color::Green | Color::DarkGreen => output.push_str("\x1b[32m"),
            Color::Yellow | Color::DarkYellow => output.push_str("\x1b[33m"),
            Color::Blue | Color::DarkBlue => output.push_str("\x1b[34m"),
            Color::Magenta | Color::DarkMagenta => output.push_str("\x1b[35m"),
            Color::Cyan | Color::DarkCyan => output.push_str("\x1b[36m"),
            Color::White => output.push_str("\x1b[37m"),
            Color::AnsiValue(n) => {
                let _ = write!(output, "\x1b[38;5;{}m", n);
            }
            Color::Rgb { r, g, b } => {
                let _ = write!(output, "\x1b[38;2;{};{};{}m", r, g, b);
            }
        }

        match self.bg {
            Color::Reset => output.push_str("\x1b[49m"),
            Color::Black => output.push_str("\x1b[40m"),
            Color::DarkGrey | Color::Grey => output.push_str("\x1b[100m"),
            Color::Red | Color::DarkRed => output.push_str("\x1b[41m"),
            Color::Green | Color::DarkGreen => output.push_str("\x1b[42m"),
            Color::Yellow | Color::DarkYellow => output.push_str("\x1b[43m"),
            Color::Blue | Color::DarkBlue => output.push_str("\x1b[44m"),
            Color::Magenta | Color::DarkMagenta => output.push_str("\x1b[45m"),
            Color::Cyan | Color::DarkCyan => output.push_str("\x1b[46m"),
            Color::White => output.push_str("\x1b[47m"),
            Color::AnsiValue(n) => {
                let _ = write!(output, "\x1b[48;5;{}m", n);
            }
            Color::Rgb { r, g, b } => {
                let _ = write!(output, "\x1b[48;2;{};{};{}m", r, g, b);
            }
        }
    }
}

/// A single terminal cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub style: CellStyle,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: CellStyle::default(),
        }
    }
}

/// Off-screen framebuffer that tracks dirty cells for incremental rendering.
///
/// On the first frame every cell is emitted as a full redraw.
/// On subsequent frames only cells that changed since the last frame are emitted.
/// This eliminates flicker and dramatically reduces terminal I/O for small edits.
pub struct ScreenBuffer {
    /// Current frame content.
    cells: Vec<Cell>,
    /// Previous frame content (for diff).
    prev: Vec<Cell>,
    /// Number of rows.
    pub rows: usize,
    /// Number of columns.
    pub cols: usize,
    /// 0 = first frame (emit everything), >0 = subsequent frames (diff only).
    generation: usize,
}

impl ScreenBuffer {
    pub fn new(rows: usize, cols: usize) -> Self {
        let total = rows * cols;
        Self {
            cells: vec![Cell::default(); total],
            prev: vec![Cell::default(); total],
            rows,
            cols,
            generation: 0,
        }
    }

    /// Resize internal buffer, resetting all state.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        if rows == self.rows && cols == self.cols {
            return;
        }
        let total = rows * cols;
        self.cells = vec![Cell::default(); total];
        self.prev = vec![Cell::default(); total];
        self.rows = rows;
        self.cols = cols;
        self.generation = 0;
    }

    /// Mark entire buffer for full redraw on next render.
    pub fn invalidate_all(&mut self) {
        self.generation = 0;
    }

    /// Reset every cell to default (space, no style).
    pub fn clear_all(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = Cell::default();
        }
    }

    /// Set a single cell at (row, col).
    pub fn set_cell(&mut self, row: usize, col: usize, ch: char, style: CellStyle) {
        if row < self.rows && col < self.cols {
            let idx = row * self.cols + col;
            self.cells[idx] = Cell { ch, style };
        }
    }

    /// Write a plain string (default style) starting at (row, col).
    pub fn set_string(&mut self, row: usize, col: usize, text: &str) {
        let default_style = CellStyle::default();
        for (i, ch) in text.chars().enumerate() {
            self.set_cell(row, col + i, ch, default_style);
        }
    }

    /// Write a string with an inline‑ANSI aware style.
    ///
    /// `segments` is a list of `(style, text)` pairs — each segment is written
    /// starting at `col` and advancing column-by-column.
    pub fn set_styled_string(
        &mut self,
        row: usize,
        mut col: usize,
        segments: &[(CellStyle, &str)],
    ) {
        for (style, text) in segments {
            for ch in text.chars() {
                self.set_cell(row, col, ch, *style);
                col += 1;
                if col >= self.cols {
                    return;
                }
            }
        }
    }

    /// Compare current frame against previous and produce a minimal ANSI string.
    ///
    /// After this call the current frame becomes the new previous frame.
    pub fn render(&mut self) -> String {
        let mut output = String::with_capacity(4096);

        if self.generation == 0 {
            // ── First frame: emit every row as a single line ──
            for row in 0..self.rows {
                let row_off = row * self.cols;
                let _ = write!(output, "\x1b[{};{}H", row + 1, 1);
                let mut prev_style = CellStyle::default();
                for col in 0..self.cols {
                    let cell = self.cells[row_off + col];
                    if cell.style != prev_style {
                        cell.style.write_ansi(&mut output);
                        prev_style = cell.style;
                    }
                    output.push(cell.ch);
                }
                // Clear to end-of-line (in case the row was shorter before)
                output.push_str("\x1b[K");
            }
            self.generation = 1;
        } else {
            // ── Subsequent frames: only changed cells ──
            for row in 0..self.rows {
                let row_off = row * self.cols;
                let mut col: usize = 0;
                while col < self.cols {
                    let idx = row_off + col;
                    if self.cells[idx] != self.prev[idx] {
                        let _ = write!(output, "\x1b[{};{}H", row + 1, col + 1);
                        let mut last_style = CellStyle::default();
                        let mut first = true;
                        while col < self.cols {
                            let idx2 = row_off + col;
                            if self.cells[idx2] != self.prev[idx2] {
                                let cell = self.cells[idx2];
                                if first || cell.style != last_style {
                                    cell.style.write_ansi(&mut output);
                                    last_style = cell.style;
                                    first = false;
                                }
                                output.push(cell.ch);
                                col += 1;
                            } else {
                                break;
                            }
                        }
                    } else {
                        col += 1;
                    }
                }
            }
        }

        // Rotate buffers: current → prev
        self.prev.copy_from_slice(&self.cells);

        output
    }
}

// ── Helpers for building styled character arrays ──

/// Convert a syntect `Style` into our `CellStyle`.
///
/// The syntect-specific extraction lives in `ijevim-highlight`, so this
/// crate never has to depend on `syntect` itself.
#[cfg(feature = "syntax")]
pub fn from_syntect_style(s: &Style) -> CellStyle {
    let c = ijevim_highlight::StyleColors::from_style(s);
    CellStyle {
        fg: c
            .fg
            .map_or(Color::Reset, |(r, g, b)| Color::Rgb { r, g, b }),
        bg: c
            .bg
            .map_or(Color::Reset, |(r, g, b)| Color::Rgb { r, g, b }),
        bold: c.bold,
        italic: c.italic,
        underline: c.underline,
    }
}

/// Build a per‑character styled array for one buffer line.
///
/// `line` is the raw text of the line.
/// `highlights` is the highlight segments (style + text) for this line. When
/// the `syntax` feature is off, callers pass `None` and the function returns
/// characters with [`CellStyle::default`].
///
/// Returns `Vec<(char, CellStyle)>` where each entry corresponds to one character
/// in the line with its associated style.
pub fn build_styled_chars(
    line: &str,
    highlights: Option<&[(Style, String)]>,
) -> Vec<(char, CellStyle)> {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    if len == 0 {
        return Vec::new();
    }

    // Build a style-per-character lookup from highlight segments.
    let mut char_styles: Vec<CellStyle> = if let Some(hls) = highlights {
        let mut styles = Vec::with_capacity(len);
        let mut pos = 0;
        for (style, text) in hls {
            let text_chars: Vec<char> = text.chars().collect();
            for _ in 0..text_chars.len() {
                if pos < len {
                    #[cfg(feature = "syntax")]
                    {
                        styles.push(from_syntect_style(style));
                    }
                    #[cfg(not(feature = "syntax"))]
                    {
                        let _ = style;
                        styles.push(CellStyle::default());
                    }
                    pos += 1;
                }
            }
        }
        // Pad with default style for any remaining characters.
        while pos < len {
            styles.push(CellStyle::default());
            pos += 1;
        }
        styles
    } else {
        vec![CellStyle::default(); len]
    };

    // Apply defaults for unhighlighted characters (e.g. trailing whitespace).
    // syntect sometimes returns fewer segments than characters.
    if char_styles.len() < len {
        char_styles.resize(len, CellStyle::default());
    }

    chars.into_iter().zip(char_styles).collect()
}

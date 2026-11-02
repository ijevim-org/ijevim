use crossterm::{
    cursor::{MoveTo, Show},
    execute,
    terminal::{size, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::collections::HashMap;
use std::io::{self, stdout, BufWriter, IsTerminal, Stdout, Write};

pub struct Terminal {
    rows: u16,
    cols: u16,
    stdout: BufWriter<Stdout>,
    line_cache: HashMap<String, String>,
}

impl Terminal {
    pub fn new() -> io::Result<Self> {
        match size() {
            Ok((cols, rows)) => Ok(Self {
                rows,
                cols,
                stdout: BufWriter::new(stdout()),
                line_cache: HashMap::new(),
            }),
            Err(e) => {
                eprintln!(
                    "Warning: Could not get terminal size: {}. Using defaults.",
                    e
                );
                Ok(Self {
                    rows: 24,
                    cols: 80,
                    stdout: BufWriter::new(stdout()),
                    line_cache: HashMap::new(),
                })
            }
        }
    }

    pub fn enable_raw_mode(&mut self) -> io::Result<()> {
        if !std::io::stdin().is_terminal() {
            return Err(io::Error::other("Not a terminal"));
        }
        crossterm::terminal::enable_raw_mode()?;
        execute!(self.stdout, EnterAlternateScreen, Show)?;
        self.update_size();
        Ok(())
    }

    pub fn disable_raw_mode(&mut self) -> io::Result<()> {
        execute!(self.stdout, LeaveAlternateScreen)?;
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }

    pub fn update_size(&mut self) {
        if let Ok((cols, rows)) = size() {
            self.cols = cols;
            self.rows = rows;
            self.line_cache.clear();
        }
    }

    pub fn clear_cache(&mut self) {
        self.line_cache.clear();
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn move_cursor(&mut self, row: u16, col: u16) -> io::Result<()> {
        execute!(
            self.stdout,
            MoveTo(col.saturating_sub(1), row.saturating_sub(1))
        )
    }

    pub fn clear_screen(&mut self) -> io::Result<()> {
        execute!(self.stdout, Clear(ClearType::All))
    }

    pub fn write_line(&mut self, row: u16, content: &str) -> io::Result<()> {
        let cols = self.cols as usize;

        let stripped = if let Some(cached) = self.line_cache.get(content) {
            cached.clone()
        } else {
            let s = strip_ansi(content);
            if self.line_cache.len() < 500 {
                self.line_cache.insert(content.to_string(), s.clone());
            }
            s
        };

        let visible_width = stripped.chars().count();
        let padded = if visible_width < cols {
            format!("{}{}", stripped, " ".repeat(cols - visible_width))
        } else {
            stripped.chars().take(cols).collect()
        };

        execute!(self.stdout, MoveTo(0, row.saturating_sub(1)))?;
        self.stdout.write_all(b"\x1b[2K")?;
        self.stdout.write_all(padded.as_bytes())?;
        Ok(())
    }

    pub fn write_status(&mut self, content: &str) -> io::Result<()> {
        let row = self.rows.saturating_sub(1);
        self.write_line(row, content)
    }

    /// Write raw bytes to the terminal output buffer.
    /// Used by ScreenBuffer-based rendering.
    /// Flushes after every write to ensure the rendered frame is actually sent
    /// to the terminal — without this the `BufWriter` would buffer indefinitely
    /// since frames are typically well below its 8 KB auto-flush threshold.
    pub fn write_raw(&mut self, data: &[u8]) -> io::Result<()> {
        self.stdout.write_all(data)?;
        self.stdout.flush()
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.disable_raw_mode();
    }
}

fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ANSI escape sequence: ESC [ ... letter
            if chars.next() == Some('[') {
                for c2 in &mut chars {
                    if c2.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

use crate::editor::Editor;
use crate::types::Position;
use crate::undo::{Edit, EditType};

/// A number token found in a line, with the style needed to write it back.
struct NumberToken {
    /// Column of the first character (the `-` sign when present).
    start: usize,
    /// Column just past the last digit.
    end: usize,
    value: i64,
    radix: u32,
    /// Explicit prefix such as `0x`, `0b`, `0o`; empty for decimal/octal.
    prefix: &'static str,
    /// Width of the digit run, used to preserve zero padding.
    digits: usize,
    uppercase: bool,
}

impl Editor {
    /// Vim's `Ctrl-a` / `Ctrl-x`: add `delta * count` to the number under or
    /// after the cursor. Keeps zero padding, radix prefixes and sign.
    pub(crate) fn increment_number(&mut self, delta: i64, count: usize) {
        let line = self.engine.state.cursor.line;
        let chars = self.engine.buffer.line_chars(line);
        let Some(token) = find_number(&chars, self.engine.state.cursor.col) else {
            return;
        };
        let old: String = chars[token.start..token.end].iter().collect();
        let shifted = token
            .value
            .saturating_add(delta.saturating_mul(count as i64));
        let new = format_number(shifted, &token);
        let cursor_after = Position {
            line,
            col: token.start,
        };
        self.engine.state.cursor.col = token.start;
        if old == new {
            self.needs_render = true;
            return;
        }

        let cursor_before = self.engine.state.cursor;
        let mod_count = self.engine.buffer.modification_count();
        let char_start = self.engine.buffer.line_to_char(line - 1) + token.start;
        self.engine
            .buffer
            .remove_range(char_start, char_start + old.chars().count());
        self.engine.buffer.insert(line, token.start, &new);
        self.engine.undo_manager.push(Edit {
            edit_type: EditType::Replace {
                line,
                col: token.start,
                old_text: old,
                new_text: new,
            },
            cursor_before,
            cursor_after,
            modification_count: mod_count,
        });
        self.on_buffer_modified();
    }
}

/// Find a number at or after `col`, wrapping to the start of the line.
fn find_number(chars: &[char], col: usize) -> Option<NumberToken> {
    let len = chars.len();
    let col = col.min(len);
    if let Some(token) = scan(chars, col, len) {
        return Some(token);
    }
    scan(chars, 0, col)
}

fn scan(chars: &[char], from: usize, to: usize) -> Option<NumberToken> {
    let mut i = from;
    while i < to {
        if chars[i].is_ascii_digit()
            || (chars[i] == '-' && i + 1 < to && chars[i + 1].is_ascii_digit())
        {
            if let Some(token) = parse_token(chars, i, to) {
                return Some(token);
            }
        }
        i += 1;
    }
    None
}

fn parse_token(chars: &[char], start: usize, to: usize) -> Option<NumberToken> {
    let mut i = start;
    let negative = chars[i] == '-';
    if negative {
        i += 1;
    }

    let mut radix = 10u32;
    let mut prefix = "";
    let mut digits_start = i;
    if chars[i] == '0' && i + 1 < to {
        match chars[i + 1] {
            'x' | 'X' => {
                radix = 16;
                prefix = if chars[i + 1] == 'X' { "0X" } else { "0x" };
                digits_start = i + 2;
            }
            'b' | 'B' => {
                radix = 2;
                prefix = if chars[i + 1] == 'B' { "0B" } else { "0b" };
                digits_start = i + 2;
            }
            'o' | 'O' => {
                radix = 8;
                prefix = if chars[i + 1] == 'O' { "0O" } else { "0o" };
                digits_start = i + 2;
            }
            '0'..='9' => radix = 8,
            _ => {}
        }
    }

    let mut end = digits_start;
    while end < to && chars[end].is_digit(radix) {
        end += 1;
    }
    if end == digits_start {
        return None;
    }

    let digits: String = chars[digits_start..end].iter().collect();
    let value = i64::from_str_radix(&digits, radix).ok()?;
    Some(NumberToken {
        start,
        end,
        value: if negative { -value } else { value },
        radix,
        prefix,
        digits: digits.len(),
        uppercase: digits.chars().any(|c| c.is_ascii_uppercase()),
    })
}

/// Render `value` back in the token's radix, padding and prefix style.
fn format_number(value: i64, token: &NumberToken) -> String {
    let magnitude = value.unsigned_abs();
    let mut digits = match token.radix {
        16 if token.uppercase => format!("{magnitude:X}"),
        16 => format!("{magnitude:x}"),
        8 if !token.prefix.is_empty() => format!("{magnitude:o}"),
        8 => format!("{magnitude:o}"),
        2 => format!("{magnitude:b}"),
        _ => magnitude.to_string(),
    };
    while digits.len() < token.digits {
        digits.insert(0, '0');
    }

    let mut out = String::new();
    if value < 0 {
        out.push('-');
    }
    out.push_str(token.prefix);
    out.push_str(&digits);
    out
}

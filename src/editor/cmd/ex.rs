use crate::editor::Editor;
use crate::types::MessageLevel;

/// Commands that accept a leading Ex range in this editor.
fn is_range_command(rest: &str) -> bool {
    matches!(rest, "d" | "y" | ">" | "<") || rest.starts_with("s/")
}

impl Editor {
    /// Split a leading Ex range off `cmd`.
    ///
    /// Supports `'<,'>`, `%`, `N`, `N,M`, `.`, `$` and combinations. Returns
    /// the resolved inclusive line range and the command that follows it.
    pub(super) fn split_ex_range<'a>(&self, cmd: &'a str) -> Option<((usize, usize), &'a str)> {
        let line_count = self.engine.buffer.content_line_count().max(1);
        if let Some(rest) = cmd.strip_prefix("'<,'>") {
            let start = self
                .engine
                .state
                .last_visual_start
                .map(|p| p.line)
                .unwrap_or(self.engine.state.cursor.line)
                .clamp(1, line_count);
            let end = self
                .engine
                .state
                .last_visual_end
                .or(self.engine.state.last_visual_start)
                .map(|p| p.line)
                .unwrap_or(self.engine.state.cursor.line)
                .clamp(1, line_count);
            let rest = rest.trim_start();
            return is_range_command(rest).then_some(((start.min(end), start.max(end)), rest));
        }

        // The command starts at the first letter or `>`/`<`; everything
        // before it must look like a range.
        let idx = cmd
            .char_indices()
            .find(|(_, c)| c.is_ascii_alphabetic() || matches!(c, '>' | '<'))?
            .0;
        if idx == 0 {
            return None;
        }
        let spec = cmd[..idx].trim();
        if spec.is_empty() {
            return None;
        }
        let rest = cmd[idx..].trim_start();
        if !is_range_command(rest) {
            return None;
        }
        let range = parse_line_range(spec, self.engine.state.cursor.line, line_count)?;
        Some((range, rest))
    }

    /// Run a range command (`d`, `y`, `>`, `<`, `s/...`).
    pub(super) fn handle_ex_range(&mut self, range: (usize, usize), rest: &str) {
        let (start, end) = range;
        match rest {
            "d" => {
                if start <= end {
                    self.delete_lines_range('"', start, end);
                }
            }
            "y" => {
                if start <= end {
                    self.yank_lines_range('"', start, end);
                }
            }
            ">" => self.indent_lines(start, end, true),
            "<" => self.indent_lines(start, end, false),
            _ if rest.starts_with("s/") => {
                self.handle_substitute(&format!("{start},{end}{rest}"));
            }
            _ => {
                self.engine
                    .state
                    .push_message(MessageLevel::Warning, format!("Unknown command: {}", rest));
            }
        }
        self.needs_render = true;
    }
}

/// Parse a `:s` range: `%`, ``, `3`, `1,5`, `.,$`, `$`.
///
/// Returns `(start, end)` clamped to the buffer, or `None` when the text is
/// not a range (so the caller can report it instead of guessing).
pub(super) fn parse_line_range(
    spec: &str,
    current: usize,
    line_count: usize,
) -> Option<(usize, usize)> {
    if spec == "%" {
        return Some((1, line_count.max(1)));
    }
    match spec.split_once(',') {
        Some((a, b)) => {
            let start = parse_line_bound(a, current, line_count)?;
            let end = parse_line_bound(b, current, line_count)?;
            Some((start.min(end), start.max(end)))
        }
        None => {
            let n = parse_line_bound(spec, current, line_count)?;
            Some((n, n))
        }
    }
}

fn parse_line_bound(token: &str, current: usize, line_count: usize) -> Option<usize> {
    let token = token.trim();
    match token {
        "" | "." => Some(current),
        "$" => Some(line_count.max(1)),
        _ => token
            .parse::<usize>()
            .ok()
            .map(|n| n.clamp(1, line_count.max(1))),
    }
}

/// Translate Vim replacement syntax into the `regex` crate's.
///
/// Vim: `&` = whole match, `\1`..`\9` = capture groups, `\&` = literal `&`.
/// regex: `$0` = whole match, `$1`..`$9` = capture groups, `$$` = literal `$`.
pub(super) fn translate_replacement(replacement: &str) -> String {
    let mut out = String::new();
    let mut chars = replacement.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(d @ '1'..='9') => {
                    out.push('$');
                    out.push(d);
                }
                Some('&') => out.push('&'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            '&' => out.push_str("$0"),
            '$' => out.push_str("$$"),
            _ => out.push(c),
        }
    }
    out
}

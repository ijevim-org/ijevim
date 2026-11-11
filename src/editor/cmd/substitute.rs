use crate::editor::Editor;
use crate::types::MessageLevel;

use super::ex::{parse_line_range, translate_replacement};

impl Editor {
    /// True for `:s/...`, `:%s/...`, `:3s/...`, `:1,5s/...` and `:$s/...`.
    pub(super) fn is_substitute_command(cmd: &str) -> bool {
        let Some(idx) = cmd.find("s/") else {
            return false;
        };
        let spec = &cmd[..idx];
        match spec.chars().next() {
            // No range at all: `:s/...`.
            None => true,
            // A range-looking prefix is routed here so a malformed one is
            // reported as an invalid range rather than an unknown command.
            Some(c) => c.is_ascii_digit() || c == '%' || c == '.' || c == '$',
        }
    }

    /// Handle `:[range]s/pattern/replacement/[g]`.
    ///
    /// The replacement follows Vim's syntax (`&`, `\1`..`\9`) and is
    /// translated to the `regex` crate's (`$0`, `$1`..`$9`).
    pub(super) fn handle_substitute(&mut self, cmd: &str) {
        let Some(idx) = cmd.find("s/") else {
            self.engine.state.push_message(
                MessageLevel::Warning,
                "Invalid substitute command".to_string(),
            );
            return;
        };
        let range_spec = &cmd[..idx];
        let rest = &cmd[idx + 2..];

        // Split into pattern / replacement / flags.
        let mut parts = rest.splitn(3, '/');
        let pattern_str = parts.next().unwrap_or("");
        let Some(replacement) = parts.next() else {
            self.engine.state.push_message(
                MessageLevel::Warning,
                "Invalid substitute syntax".to_string(),
            );
            return;
        };
        let flags = parts.next().unwrap_or("");

        if pattern_str.is_empty() {
            self.engine.state.push_message(
                MessageLevel::Warning,
                "Empty substitute pattern".to_string(),
            );
            return;
        }

        let re = match regex::Regex::new(pattern_str) {
            Ok(re) => re,
            Err(e) => {
                self.engine
                    .state
                    .push_message(MessageLevel::Warning, format!("Regex error: {}", e));
                return;
            }
        };
        let replacement = translate_replacement(replacement);

        let global = flags.contains('g');
        let line_count = self.engine.buffer.line_count();

        let Some((start_line, end_line)) =
            parse_line_range(range_spec, self.engine.state.cursor.line, line_count)
        else {
            self.engine.state.push_message(
                MessageLevel::Warning,
                format!("Invalid range: {}", range_spec),
            );
            return;
        };

        let mut replacements = 0;
        let mut edits: Vec<crate::undo::Edit> = Vec::new();

        // Process lines bottom-up so earlier line indices stay valid, and treat
        // the trailing newline as part of the line. Otherwise substituting N
        // lines joins them all into one.
        for line_idx in (start_line..=end_line).rev() {
            let old_text = self.engine.buffer.get_line(line_idx);
            let new_text = if global {
                re.replace_all(&old_text, replacement.as_str()).to_string()
            } else {
                re.replace(&old_text, replacement.as_str()).to_string()
            };
            if new_text == old_text {
                continue;
            }
            replacements += 1;
            let mod_count = self.engine.buffer.modification_count();

            let char_start = self.engine.buffer.line_to_char(line_idx - 1);
            let char_end = char_start + old_text.chars().count();
            self.engine.buffer.remove_range(char_start, char_end);
            self.engine.buffer.insert(line_idx, 0, &new_text);

            edits.push(crate::undo::Edit {
                edit_type: crate::undo::EditType::Replace {
                    line: line_idx,
                    col: 0,
                    old_text,
                    new_text,
                },
                cursor_before: self.engine.state.cursor,
                cursor_after: self.engine.state.cursor,
                modification_count: mod_count,
            });
        }

        // One `u` reverts the whole `:s`, like Vim. The group records the
        // modification count from before the first substitution so undoing it
        // restores the clean state.
        if let Some(first) = edits.first() {
            let modification_count = first.modification_count;
            let cursor_before = first.cursor_before;
            self.engine.undo_manager.push(crate::undo::Edit {
                edit_type: crate::undo::EditType::Group { edits },
                cursor_before,
                cursor_after: self.engine.state.cursor,
                modification_count,
            });
        }

        self.engine.state.dirty = self.engine.buffer.is_dirty();
        self.engine.state.push_message(
            MessageLevel::Info,
            format!(
                "{} substitutions on {} line{}",
                replacements,
                if end_line > start_line {
                    format!("{}", end_line - start_line + 1)
                } else {
                    "1".to_string()
                },
                if replacements == 1 { "" } else { "s" }
            ),
        );
        self.needs_render = true;
    }
}

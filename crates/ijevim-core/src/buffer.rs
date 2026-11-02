use crate::types::{DirEntry, IjevimError, Result};
use ropey::Rope;
use std::path::{Path, PathBuf};

pub struct TextBuffer {
    doc: Rope,
    file_path: Option<PathBuf>,
    /// Mirror of `modification_count != saved_modification_count`.
    /// Kept in sync internally; mutate via mutation methods only.
    dirty: bool,
    modification_count: usize,
    pub(crate) saved_modification_count: usize,
    /// If set, this buffer shows a directory listing (file browser).
    dir_path: Option<PathBuf>,
    /// Cached directory entries for navigation. Index i corresponds to line i+1.
    dir_entries: Vec<DirEntry>,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            doc: Rope::new(),
            file_path: None,
            dirty: false,
            modification_count: 0,
            saved_modification_count: 0,
            dir_path: None,
            dir_entries: Vec::new(),
        }
    }

    pub fn with_text(text: &str) -> Self {
        Self {
            doc: Rope::from_str(text),
            file_path: None,
            dirty: false,
            modification_count: 0,
            saved_modification_count: 0,
            dir_path: None,
            dir_entries: Vec::new(),
        }
    }

    pub fn line_count(&self) -> usize {
        self.doc.len_lines()
    }

    pub fn get_line(&self, line_number: usize) -> String {
        if line_number < 1 || line_number > self.line_count() {
            return String::new();
        }
        self.doc.line(line_number - 1).to_string()
    }

    /// Return the character count (not byte count) of a line.
    /// Use this instead of `get_line(n).len()` for cursor bounds.
    pub fn line_char_len(&self, line_number: usize) -> usize {
        if line_number < 1 || line_number > self.line_count() {
            return 0;
        }
        self.doc.line(line_number - 1).chars().count()
    }

    /// Return the characters of a line as a `Vec<char>`.
    /// Use this instead of `get_line(n).chars().collect()` to avoid an
    /// intermediate `String` allocation — `RopeSlice` yields chars directly.
    pub fn line_chars(&self, line_number: usize) -> Vec<char> {
        if line_number < 1 || line_number > self.line_count() {
            return Vec::new();
        }
        self.doc.line(line_number - 1).chars().collect()
    }

    pub fn insert(&mut self, line: usize, col: usize, text: &str) {
        let line_idx = line.saturating_sub(1);
        if line_idx >= self.doc.len_lines() {
            // Caller is targeting a line past the current end. Materialise the
            // new line by appending a newline, then drop the text on it.
            // Skip when the gap is more than one line (caller error).
            if line_idx == self.doc.len_lines() {
                self.doc.insert(self.doc.len_chars(), "\n");
                self.mark_modified();
            } else if self.doc.len_chars() == 0 {
                // Empty rope still reports `len_lines() == 1`. Treat an insert
                // beyond line 1 on an empty rope as a single-line append.
                if line_idx > 1 {
                    return;
                }
                self.doc.insert(0, "\n");
                self.mark_modified();
            } else {
                return;
            }

            if text != "\n" {
                let insert_pos = self.doc.len_chars();
                self.doc.insert(insert_pos, text);
                self.mark_modified();
            }
            return;
        }

        let line_start = self.doc.line_to_char(line_idx);
        let char_idx = line_start + col.min(self.doc.line(line_idx).len_chars());

        self.doc.insert(char_idx, text);
        self.mark_modified();
    }

    pub fn delete(&mut self, line: usize, col: usize) {
        let line_idx = line.saturating_sub(1);
        if line_idx >= self.doc.len_lines() {
            return;
        }

        let line_start = self.doc.line_to_char(line_idx);
        let char_idx = line_start + col;

        if char_idx >= self.doc.len_chars() {
            return;
        }

        self.doc.remove(char_idx..char_idx + 1);
        self.mark_modified();
    }

    pub fn insert_char(&mut self, line: usize, col: usize, ch: char) {
        self.insert(line, col, &ch.to_string());
    }

    pub fn insert_newline(&mut self, line: usize, col: usize) {
        self.insert(line, col, "\n");
    }

    pub fn merge_with_prev_line(&mut self, line: usize) -> usize {
        if line <= 1 || line > self.line_count() {
            return 0;
        }

        let prev_line_idx = line - 2;
        let cur_line_idx = line - 1;

        if prev_line_idx >= self.doc.len_lines() || cur_line_idx >= self.doc.len_lines() {
            return 0;
        }

        let prev_line_start = self.doc.line_to_char(prev_line_idx);
        let prev_line_len = self.doc.line(prev_line_idx).len_chars();
        let newline_pos = prev_line_start + prev_line_len - 1;

        if newline_pos >= self.doc.len_chars() {
            return 0;
        }

        self.doc.remove(newline_pos..newline_pos + 1);
        self.mark_modified();

        prev_line_len - 1
    }

    pub async fn load_file(path: &str) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        let mut buffer = Self::with_text(&content);
        buffer.file_path = Some(PathBuf::from(path));
        Ok(buffer)
    }

    pub async fn save_file(&mut self) -> Result<()> {
        if let Some(path) = self.file_path.clone() {
            self.save_to_path(&path).await
        } else {
            Err(IjevimError::NoFilePath)
        }
    }

    pub async fn save_to_path(&mut self, path: &Path) -> Result<()> {
        let content = self.doc.to_string();
        tokio::fs::write(path, content).await?;
        self.file_path = Some(path.to_path_buf());
        self.mark_clean();
        Ok(())
    }

    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.doc.to_string()
    }

    /// Monotonically increasing counter incremented on every buffer mutation.
    /// The undo manager uses this to detect post-edit divergence from a
    /// captured snapshot.
    pub fn modification_count(&self) -> usize {
        self.modification_count
    }

    /// `true` if the buffer has not been modified since the last save/load.
    pub fn is_clean(&self) -> bool {
        self.modification_count == self.saved_modification_count
    }

    /// `true` if [`Self::is_clean`] returns `false`.
    pub fn is_dirty(&self) -> bool {
        !self.is_clean()
    }

    /// Reset the dirty flag to match the current contents. Call after a
    /// successful save or after loading/replacing buffer text.
    pub fn mark_clean(&mut self) {
        self.saved_modification_count = self.modification_count;
        self.dirty = false;
    }

    /// Back-compat alias for [`Self::mark_clean`].
    pub fn reset_clean_state(&mut self) {
        self.mark_clean();
    }

    pub(crate) fn reset_modification_count(&mut self, count: usize) {
        self.modification_count = count;
    }

    pub(crate) fn replace_with(&mut self, text: &str) {
        let len = self.doc.len_chars();
        if len > 0 {
            self.doc.remove(0..len);
        }
        self.doc.insert(0, text);
        self.mark_modified();
    }

    pub fn line_to_char(&self, line_idx: usize) -> usize {
        self.doc.line_to_char(line_idx)
    }

    pub fn len_chars(&self) -> usize {
        self.doc.len_chars()
    }

    /// Returns the path the buffer was loaded from, or `None` if it has never
    /// been associated with a file on disk.
    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Replace the associated file path. Use this when rebinding a buffer to a
    /// different file (e.g., after `:w newpath`).
    pub fn set_file_path(&mut self, path: Option<PathBuf>) {
        self.file_path = path;
    }

    /// Returns `true` if this buffer contains a directory listing (file browser).
    pub fn is_directory_listing(&self) -> bool {
        self.dir_path.is_some()
    }

    /// Returns the directory path if this is a directory listing buffer.
    pub fn dir_path(&self) -> Option<&Path> {
        self.dir_path.as_deref()
    }

    /// Returns the cached directory entries for navigation.
    /// Entry at index `i` corresponds to buffer line `i + 1`.
    pub fn dir_entries(&self) -> &[DirEntry] {
        &self.dir_entries
    }

    /// Return the directory entry at the given buffer line (1-indexed).
    pub fn selected_entry(&self, line: usize) -> Option<&DirEntry> {
        if line < 1 || line > self.dir_entries.len() {
            return None;
        }
        self.dir_entries.get(line - 1)
    }

    /// Mark this buffer as a directory listing with the given entries and path.
    pub fn set_dir_listing(&mut self, entries: Vec<DirEntry>, path: PathBuf) {
        self.dir_entries = entries;
        self.dir_path = Some(path);
    }

    /// Single point of truth for marking the buffer as modified. All mutation
    /// methods funnel through this helper to keep `dirty` and
    /// `modification_count` in lock-step.
    fn mark_modified(&mut self) {
        self.modification_count += 1;
        self.dirty = true;
    }

    /// Test-only: force the dirty flag without going through a real mutation.
    /// Crate-private so unit tests in `editor/` can set up quit-confirmation
    /// scenarios without having to type a character first.
    #[cfg(test)]
    pub fn force_dirty(&mut self, dirty: bool) {
        if dirty {
            // Increment past `saved_modification_count` to make `is_dirty()` true.
            self.modification_count = self.saved_modification_count + 1;
            self.dirty = true;
        } else {
            self.mark_clean();
        }
    }

    pub fn get_word_at(&self, line: usize, col: usize) -> (String, usize, usize) {
        if line < 1 || line > self.line_count() {
            return (String::new(), 0, 0);
        }
        let chars: Vec<char> = self.line_chars(line);

        if col >= chars.len() {
            return (String::new(), col, col);
        }

        let mut start = col;
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }

        let mut end = col;
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }

        let word: String = chars[start..end].iter().collect();
        (word, start, end)
    }

    pub fn get_word_range(&self, line: usize, col: usize) -> (String, usize, usize) {
        self.get_word_at(line, col)
    }

    /// Get the WORD range at the given position.
    /// A WORD is a sequence of non-whitespace characters (Vim's `W`/`B`).
    /// Returns (word_text, start_col, end_col).
    pub fn get_word_range_upper(&self, line: usize, col: usize) -> (String, usize, usize) {
        self.get_word_at(line, col)
    }

    pub fn get_line_range(&self, start_line: usize, end_line: usize) -> String {
        if start_line > end_line || start_line < 1 {
            return String::new();
        }
        let start = start_line.saturating_sub(1);
        let end = end_line.min(self.doc.len_lines());
        if start >= end {
            return String::new();
        }
        let mut result = String::new();
        for i in start..end {
            result.push_str(&self.doc.line(i).to_string());
            if i < end - 1 {
                result.push('\n');
            }
        }
        result
    }

    pub fn get_char_range(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        if start_line > end_line {
            return String::new();
        }
        if start_line == end_line {
            if start_col >= end_col {
                return String::new();
            }
            let line_str = self.get_line(start_line);
            return line_str
                .chars()
                .skip(start_col)
                .take(end_col - start_col)
                .collect();
        }

        let mut result = String::new();
        result.push_str(
            &self
                .get_line(start_line)
                .chars()
                .skip(start_col)
                .collect::<String>(),
        );
        result.push('\n');
        for line_num in (start_line + 1)..end_line {
            result.push_str(&self.get_line(line_num));
            result.push('\n');
        }
        result.push_str(
            &self
                .get_line(end_line)
                .chars()
                .take(end_col)
                .collect::<String>(),
        );
        result
    }

    pub fn delete_line(&mut self, line: usize) -> String {
        if line < 1 || line > self.line_count() {
            return String::new();
        }
        let line_idx = line.saturating_sub(1);
        let content = self.doc.line(line_idx).to_string();

        let char_start = self.doc.line_to_char(line_idx);
        let char_end = if line_idx + 1 < self.doc.len_lines() {
            self.doc.line_to_char(line_idx + 1)
        } else {
            self.doc.len_chars()
        };

        if char_end > char_start {
            self.doc.remove(char_start..char_end);
            // Ensure buffer always has at least one newline
            if self.doc.len_chars() == 0 {
                self.doc.insert(0, "\n");
            }
            self.mark_modified();
        }

        content
    }

    pub fn delete_range(
        &mut self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        if start_line > end_line {
            return String::new();
        }

        let content = self.get_char_range(start_line, start_col, end_line, end_col);

        let start_line_idx = start_line.saturating_sub(1);
        let end_line_idx = end_line.saturating_sub(1);

        let char_start = self.doc.line_to_char(start_line_idx)
            + start_col.min(self.doc.line(start_line_idx).len_chars());
        let char_end = self.doc.line_to_char(end_line_idx)
            + end_col.min(self.doc.line(end_line_idx).len_chars());

        if char_start < char_end {
            self.doc.remove(char_start..char_end);
            self.mark_modified();
        }

        content
    }

    pub fn remove_range(&mut self, start: usize, end: usize) {
        if start < end {
            self.doc.remove(start..end);
            self.mark_modified();
        }
    }

    pub fn search(&self, query: &str) -> Vec<crate::types::SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }

        // Use regex for efficient searching. Escape the query to treat it
        // as a literal string (not a regex pattern).
        let escaped = regex::escape(query);
        let re = match regex::Regex::new(&escaped) {
            Ok(re) => re,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();

        for line_idx in 0..self.doc.len_lines() {
            let line_str = self.doc.line(line_idx).to_string();
            for mat in re.find_iter(&line_str) {
                // Convert byte offset to char offset for the column.
                let start_col = line_str[..mat.start()].chars().count();
                results.push(crate::types::SearchResult {
                    line: line_idx + 1,
                    start_col,
                });
            }
        }

        results
    }

    /// Get the text content of a rectangular (visual block) region.
    /// The rectangle spans lines [start_line, end_line] and columns [start_col, end_col].
    /// Returns each line segment joined by newline.
    pub fn get_block_range(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        let (s_line, e_line) = if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        };
        let (s_col, e_col) = if start_col <= end_col {
            (start_col, end_col)
        } else {
            (end_col, start_col)
        };

        let mut result = String::new();
        for line_num in s_line..=e_line {
            let chars = self.line_chars(line_num);
            let end = e_col.min(chars.len());
            if s_col < end {
                result.push_str(&chars[s_col..end].iter().collect::<String>());
            }
            if line_num < e_line {
                result.push('\n');
            }
        }
        result
    }

    /// Delete a rectangular (visual block) region.
    /// Each line has the range [col_start, col_end) removed.
    /// Returns the deleted content (same format as `get_block_range`).
    pub fn delete_block_range(
        &mut self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        let (s_line, e_line) = if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        };
        let (s_col, e_col) = if start_col <= end_col {
            (start_col, end_col)
        } else {
            (end_col, start_col)
        };

        let mut result = String::new();
        // Delete from bottom line to top to avoid index shifting
        // But since we're deleting within lines (not full lines),
        // we can process top-to-bottom safely because each line's
        // deletion is at the same col range within its own line.
        for line_num in s_line..=e_line {
            let chars = self.line_chars(line_num);
            let end = e_col.min(chars.len());
            if s_col < end {
                let line_content: String = chars[s_col..end].iter().collect();
                result.push_str(&line_content);
                if line_num < e_line {
                    result.push('\n');
                }
                // Delete the range for this line
                let line_idx = line_num.saturating_sub(1);
                let char_start = self.doc.line_to_char(line_idx) + s_col;
                let char_end = self.doc.line_to_char(line_idx) + end;
                if char_start < char_end && char_end <= self.doc.len_chars() {
                    self.doc.remove(char_start..char_end);
                    self.mark_modified();
                }
            }
        }
        result
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_char() {
        let mut buf = TextBuffer::new();
        buf.insert_char(1, 0, 'a');
        assert_eq!(buf.get_line(1), "a");
        assert!(buf.is_dirty());
        assert_eq!(buf.modification_count(), 1);
    }

    #[test]
    fn test_insert_at_end_of_buffer() {
        let mut buf = TextBuffer::new();
        buf.insert(1, 0, "hello");
        assert_eq!(buf.get_line(1), "hello");
        assert!(buf.is_dirty());
    }

    #[test]
    fn test_delete_char() {
        let mut buf = TextBuffer::with_text("ab\n");
        buf.delete(1, 0);
        assert_eq!(buf.get_line(1), "b\n");
        assert!(buf.is_dirty());
    }

    #[test]
    fn test_merge_with_prev_line() {
        let mut buf = TextBuffer::with_text("hello\nworld\n");
        buf.merge_with_prev_line(2);
        assert_eq!(buf.to_string(), "helloworld\n");
        assert!(buf.is_dirty());
    }

    #[test]
    fn test_save_resets_dirty() {
        let mut buf = TextBuffer::with_text("test\n");
        buf.set_file_path(Some(std::env::temp_dir().join("test_ijevim.txt")));
        // Use a current-thread runtime; this matches the editor's runtime
        // flavour and keeps tokio's rt-multi-thread feature unrequired.
        // No `enable_all()` — we don't need the time driver for `tokio::fs`.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            buf.save_file().await.expect("Save failed");
        });
        assert!(!buf.is_dirty());
    }

    #[test]
    fn test_is_dirty_mirrors_modification_count() {
        let mut buf = TextBuffer::new();
        assert!(!buf.is_dirty());
        assert!(buf.is_clean());
        buf.insert_char(1, 0, 'x');
        assert!(buf.is_dirty());
        assert!(!buf.is_clean());
        buf.mark_clean();
        assert!(!buf.is_dirty());
        assert!(buf.is_clean());
    }
}

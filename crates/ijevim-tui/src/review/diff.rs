//! Git diff integration for Review Mode.
//!
//! This module runs `git diff`, parses unified diff output into structured
//! [`DiffFile`] / [`Hunk`] / [`DiffLine`] data, and stages/unstages patches.
//!
//! All git invocations take an explicit repository root (`git -C <root>`), so
//! review mode never depends on the process working directory.

use crate::review::annotations;
use crate::review::state::{
    DiffFile, DiffLine, DiffLineKind, FileStatus, Hunk, ReviewArgs, ReviewState,
};
use std::path::Path;
use std::process::{Command, Stdio};

/// Options applied to every diff invocation so parsing is prefix-stable.
const DIFF_OPTIONS: [&str; 4] = [
    "--no-color",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

// ── Public API ──────────────────────────────────────────────────────

/// Build review state from the real `git diff`.
pub fn try_get_review_state(args: &ReviewArgs) -> Result<ReviewState, String> {
    let project_root =
        ijevim_core::git::find_root().ok_or_else(|| "not inside a git repository".to_string())?;
    let files = load_diff(args, &project_root)?;
    let annotations = annotations::load_annotations(&project_root);

    Ok(ReviewState {
        args: args.clone(),
        files,
        selected_file: 0,
        selected_hunk: 0,
        sub_mode: crate::review::ReviewSubMode::FileList,
        cursor_row: 0,
        annotation_selected: 0,
        annotation_pending_delete: false,
        pending_discard: None,
        comment_target: None,
        comment_anchor: None,
        annotations,
        comments: Vec::new(),
        scroll: crate::review::ScrollState::default(),
        viewport_height: 20,
        diff_style: crate::review::DiffStyle::Unified,
        file_list_width: 30,
        project_root: Some(project_root),
        comment_buffer: String::new(),
        comment_cursor: 0,
    })
}

/// Reload the diff from git, preserving file / line selection as far as
/// possible.
pub fn reload(state: &mut ReviewState) -> Result<(), String> {
    let Some(root) = state.project_root.clone() else {
        return Err("not inside a git repository".to_string());
    };

    let old_path = state.current_file().map(|f| f.display_path().to_string());
    let old_hunk = state
        .current_row()
        .map(|row| row.hunk_index)
        .unwrap_or(state.selected_hunk);
    let old_line = state.current_line();
    let old_cursor = state.cursor_row;

    let files = load_diff(&state.args, &root)?;
    state.files = files;

    if state.files.is_empty() {
        state.selected_file = 0;
        state.selected_hunk = 0;
        state.cursor_row = 0;
        state.scroll.scroll_top = 0;
        return Ok(());
    }

    let file_index = old_path
        .and_then(|path| {
            state
                .files
                .iter()
                .position(|file| file.display_path() == path)
        })
        .unwrap_or_else(|| state.selected_file.min(state.files.len() - 1));

    state.select_file(file_index);
    let rows = state.rows_for(file_index);
    let target = old_line
        .and_then(|line| crate::review::rows::row_for_line(&rows, line))
        .or_else(|| crate::review::rows::first_row_of_hunk(&rows, old_hunk))
        .unwrap_or_else(|| old_cursor.min(rows.len().saturating_sub(1)));
    state.set_cursor_row(target);
    Ok(())
}

// ── Loading diffs ───────────────────────────────────────────────────

fn load_diff(args: &ReviewArgs, root: &Path) -> Result<Vec<DiffFile>, String> {
    let diff_output = run_git_diff(args, root)?;
    let mut files = parse_unified_diff(&diff_output);
    apply_staged_status(&mut files, args, root);
    Ok(files)
}

fn run_git_diff(args: &ReviewArgs, root: &Path) -> Result<String, String> {
    match args {
        ReviewArgs::WorkingTree => run_working_tree_diff(root),
        ReviewArgs::Staged => run_diff(root, &["diff", "--cached", "--unified=3"], &[]),
        ReviewArgs::Against(rev) => run_diff(root, &["diff", rev.as_str(), "--unified=3"], &[]),
        ReviewArgs::Range(a, b) => {
            let range = format!("{}..{}", a, b);
            run_diff(root, &["diff", range.as_str(), "--unified=3"], &[])
        }
        ReviewArgs::File(path) => run_diff(root, &["diff", "--unified=3"], &[path.as_str()]),
    }
}

/// Run `:review` (default): all changes since HEAD.
fn run_working_tree_diff(root: &Path) -> Result<String, String> {
    match run_diff(root, &["diff", "HEAD", "--unified=3"], &[]) {
        Ok(output) => Ok(output),
        Err(_) => {
            // HEAD does not exist yet (empty repository). Combine staged and
            // unstaged diffs.
            let staged = run_diff(root, &["diff", "--cached", "--unified=3"], &[])?;
            let unstaged = run_diff(root, &["diff", "--unified=3"], &[])?;
            let mut combined = staged;
            combined.push_str(&unstaged);
            Ok(combined)
        }
    }
}

fn run_diff(root: &Path, base: &[&str], paths: &[&str]) -> Result<String, String> {
    let mut args: Vec<&str> = base.to_vec();
    args.extend_from_slice(&DIFF_OPTIONS);
    if !paths.is_empty() {
        args.push("--");
        args.extend_from_slice(paths);
    }
    run_git(root, &args)
}

/// Run a git command inside `root`, returning stdout on success.
fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("git {} failed", args.join(" "))
        } else {
            stderr
        })
    }
}

/// Mark hunks that are already present in the index.
fn apply_staged_status(files: &mut [DiffFile], args: &ReviewArgs, root: &Path) {
    match args {
        ReviewArgs::Staged => {
            for file in files.iter_mut() {
                for hunk in file.hunks.iter_mut() {
                    hunk.staged = true;
                }
            }
        }
        ReviewArgs::WorkingTree => {
            let cached = run_diff(root, &["diff", "--cached", "--unified=3"], &[])
                .map(|output| parse_unified_diff(&output))
                .unwrap_or_default();

            for file in files.iter_mut() {
                let Some(cached_file) = cached
                    .iter()
                    .find(|candidate| candidate.display_path() == file.display_path())
                else {
                    continue;
                };
                for hunk in file.hunks.iter_mut() {
                    hunk.staged = cached_file
                        .hunks
                        .iter()
                        .any(|cached_hunk| hunks_equivalent(hunk, cached_hunk));
                }
            }
        }
        _ => {}
    }
}

/// Compare hunks by their HEAD-side position and content, ignoring the new
/// line offset (which shifts when unrelated unstaged changes exist).
fn hunks_equivalent(a: &Hunk, b: &Hunk) -> bool {
    a.old_start == b.old_start
        && a.old_count == b.old_count
        && a.lines.len() == b.lines.len()
        && a.lines.iter().zip(b.lines.iter()).all(|(left, right)| {
            left.kind == right.kind
                && left.content == right.content
                && left.no_newline == right.no_newline
        })
}

// ── Unified diff parser ─────────────────────────────────────────────

/// Parse a unified diff string into structured `DiffFile` entries.
pub fn parse_unified_diff(input: &str) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current_file: Option<DiffFileBuilder> = None;

    for line in input.lines() {
        if line.starts_with("diff --git ") {
            if let Some(builder) = current_file.take() {
                if let Some(file) = builder.build() {
                    files.push(file);
                }
            }

            let paths = line.strip_prefix("diff --git ").unwrap_or(line);
            let (old, new) = parse_diff_paths(paths);
            current_file = Some(DiffFileBuilder {
                old_path: old.unwrap_or_else(|| "unknown".to_string()),
                new_path: new.unwrap_or_else(|| "unknown".to_string()),
                status: FileStatus::Modified,
                hunks: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("rename from ") {
            if let Some(file) = current_file.as_mut() {
                file.old_path = normalize_diff_path(rest);
                file.status = FileStatus::Renamed;
            }
        } else if let Some(rest) = line.strip_prefix("rename to ") {
            if let Some(file) = current_file.as_mut() {
                file.new_path = normalize_diff_path(rest);
                file.status = FileStatus::Renamed;
            }
        } else if line.starts_with("--- ")
            && current_file
                .as_ref()
                .is_some_and(|file| file.hunks.is_empty())
        {
            // File headers only occur before the first hunk. Once inside a
            // hunk, a line starting with `---` is deleted content.
            if let Some(file) = current_file.as_mut() {
                file.old_path = normalize_diff_path(&line[4..]);
            }
        } else if line.starts_with("+++ ")
            && current_file
                .as_ref()
                .is_some_and(|file| file.hunks.is_empty())
        {
            if let Some(file) = current_file.as_mut() {
                file.new_path = normalize_diff_path(&line[4..]);
            }
        } else if line.starts_with("@@") {
            if let Some(file) = current_file.as_mut() {
                if let Some(hunk) = parse_hunk_header(line) {
                    file.hunks.push(hunk);
                }
            }
        } else if line.starts_with("\\ No newline at end of file") {
            if let Some(file) = current_file.as_mut() {
                if let Some(hunk) = file.hunks.last_mut() {
                    if let Some(last) = hunk.lines.last_mut() {
                        last.no_newline = true;
                    }
                }
            }
        } else if let Some(content) = line.strip_prefix('+') {
            if let Some(file) = current_file.as_mut() {
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Add,
                        content: content.to_string(),
                        old_lineno: None,
                        new_lineno: Some(hunk.new_start + hunk.add_count),
                        no_newline: false,
                    });
                    hunk.add_count += 1;
                }
            }
        } else if let Some(content) = line.strip_prefix('-') {
            if let Some(file) = current_file.as_mut() {
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Delete,
                        content: content.to_string(),
                        old_lineno: Some(hunk.old_start + hunk.del_count),
                        new_lineno: None,
                        no_newline: false,
                    });
                    hunk.del_count += 1;
                }
            }
        } else if let Some(content) = line.strip_prefix(' ') {
            if let Some(file) = current_file.as_mut() {
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Context,
                        content: content.to_string(),
                        old_lineno: Some(hunk.old_start + hunk.del_count),
                        new_lineno: Some(hunk.new_start + hunk.add_count),
                        no_newline: false,
                    });
                    hunk.del_count += 1;
                    hunk.add_count += 1;
                }
            }
        }
    }

    if let Some(builder) = current_file.take() {
        if let Some(file) = builder.build() {
            files.push(file);
        }
    }

    for file in &mut files {
        if file.old_path == "/dev/null" {
            file.status = FileStatus::Added;
        } else if file.new_path == "/dev/null" {
            file.status = FileStatus::Deleted;
        } else if file.old_path != file.new_path {
            file.status = FileStatus::Renamed;
        } else {
            file.status = FileStatus::Modified;
        }
    }

    files
}

// ── Hunk / file staging via git apply ───────────────────────────────

/// Reconstruct the unified-diff patch text for a single hunk.
pub fn hunk_to_patch(hunk: &Hunk, file: &DiffFile) -> String {
    let mut patch = String::new();

    if file.old_path != file.new_path
        && file.old_path != "/dev/null"
        && file.new_path != "/dev/null"
    {
        patch.push_str(&format!(
            "diff --git {} {}\n",
            patch_path("a/", &file.old_path),
            patch_path("b/", &file.new_path)
        ));
        patch.push_str(&format!("rename from {}\n", file.old_path));
        patch.push_str(&format!("rename to {}\n", file.new_path));
    }

    let old = if file.old_path == "/dev/null" {
        "/dev/null".to_string()
    } else {
        patch_path("a/", &file.old_path)
    };
    let new = if file.new_path == "/dev/null" {
        "/dev/null".to_string()
    } else {
        patch_path("b/", &file.new_path)
    };

    patch.push_str(&format!("--- {}\n+++ {}\n", old, new));
    patch.push_str(&hunk.header);
    patch.push('\n');

    for line in &hunk.lines {
        match line.kind {
            DiffLineKind::Add => patch.push('+'),
            DiffLineKind::Delete => patch.push('-'),
            DiffLineKind::Context => patch.push(' '),
        }
        patch.push_str(&line.content);
        patch.push('\n');
        if line.no_newline {
            patch.push_str("\\ No newline at end of file\n");
        }
    }

    patch
}

/// Stage a single hunk by piping its patch to `git apply --cached`.
pub fn stage_hunk(hunk: &Hunk, file: &DiffFile, root: &Path) -> Result<(), String> {
    apply_patch(root, &hunk_to_patch(hunk, file), false)
}

/// Unstage a single hunk by piping its reverse patch to `git apply --cached`.
pub fn unstage_hunk(hunk: &Hunk, file: &DiffFile, root: &Path) -> Result<(), String> {
    apply_patch(root, &hunk_to_patch(hunk, file), true)
}

fn apply_patch(root: &Path, patch: &str, reverse: bool) -> Result<(), String> {
    let mut args = vec!["apply", "--cached"];
    if reverse {
        args.push("--reverse");
    }

    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git apply: {}", e))?;

    use std::io::Write;
    if let Some(ref mut stdin) = child.stdin {
        stdin
            .write_all(patch.as_bytes())
            .map_err(|e| format!("Failed to write patch to git apply: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for git apply: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "git apply --cached failed".to_string()
        } else {
            format!("git apply --cached failed: {}", stderr)
        })
    }
}

/// Stage all changes in a file.
pub fn stage_file(file: &DiffFile, root: &Path) -> Result<(), String> {
    run_git(root, &["add", "-A", "--", file.display_path()]).map(|_| ())
}

/// Unstage all changes in a file.
pub fn unstage_file(file: &DiffFile, root: &Path) -> Result<(), String> {
    let path = file.display_path();
    if run_git(root, &["restore", "--staged", "--", path]).is_ok() {
        return Ok(());
    }
    if run_git(root, &["reset", "-q", "HEAD", "--", path]).is_ok() {
        return Ok(());
    }
    run_git(root, &["rm", "--cached", "--", path]).map(|_| ())
}

/// Discard all index and worktree changes for a file.
pub fn discard_file(file: &DiffFile, root: &Path) -> Result<(), String> {
    let path = file.display_path();

    if run_git(root, &["restore", "--staged", "--worktree", "--", path]).is_ok() {
        return Ok(());
    }

    if file.status == FileStatus::Added {
        return run_git(root, &["rm", "-f", "--", path]).map(|_| ());
    }

    if run_git(root, &["checkout", "HEAD", "--", path]).is_ok() {
        return Ok(());
    }
    run_git(root, &["checkout", "--", path]).map(|_| ())
}

// ── Path parsing helpers ────────────────────────────────────────────

/// Parse `diff --git a/path b/path` into (old_path, new_path).
fn parse_diff_paths(s: &str) -> (Option<String>, Option<String>) {
    let s = s.trim();

    // Quoted paths: "a/foo bar" "b/foo bar".
    if let Some((first, next)) = parse_quoted_token(s, 0) {
        let rest = s[next..].trim_start();
        if let Some((second, _)) = parse_quoted_token(rest, 0) {
            return (
                Some(normalize_diff_path(&first)),
                Some(normalize_diff_path(&second)),
            );
        }
        let second = rest.split('\t').next().unwrap_or(rest);
        return (
            Some(normalize_diff_path(&first)),
            Some(normalize_diff_path(second)),
        );
    }

    // Unquoted paths with spaces: split on the last ` b/` / ` w/` boundary.
    for marker in [" b/", " w/", " i/"] {
        if let Some(index) = s.rfind(marker) {
            let old = &s[..index];
            let new = &s[index + 1..];
            return (
                Some(normalize_diff_path(old)),
                Some(normalize_diff_path(new)),
            );
        }
    }

    // Fallback: whitespace-separated tokens.
    let mut parts = s.split_whitespace();
    let old = parts.next().map(normalize_diff_path);
    let new = parts.next().map(normalize_diff_path);
    (old, new)
}

/// Parse one C-style quoted git path starting at `start`.
fn parse_quoted_token(s: &str, start: usize) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if start >= bytes.len() || bytes[start] != b'"' {
        return None;
    }

    let mut out: Vec<u8> = Vec::new();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                return Some((String::from_utf8_lossy(&out).to_string(), i + 1));
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                match bytes[i] {
                    b'a' => out.push(0x07),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0c),
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'v' => out.push(0x0b),
                    b'\\' => out.push(b'\\'),
                    b'"' => out.push(b'"'),
                    b'0'..=b'7' => {
                        let mut value = 0u8;
                        let mut digits = 0;
                        while digits < 3 && i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                            value = value * 8 + (bytes[i] - b'0');
                            i += 1;
                            digits += 1;
                        }
                        out.push(value);
                        continue;
                    }
                    other => out.push(other),
                }
            }
            other => out.push(other),
        }
        i += 1;
    }
    None
}

/// Normalise a path from a diff header.
fn normalize_diff_path(raw: &str) -> String {
    let raw = raw.trim_end();
    let raw = raw.split('\t').next().unwrap_or(raw).trim();

    let raw = if raw.starts_with('"') {
        parse_quoted_token(raw, 0)
            .map(|(token, _)| token)
            .unwrap_or_else(|| raw.to_string())
    } else {
        raw.to_string()
    };

    strip_diff_prefix(&raw)
}

fn strip_diff_prefix(path: &str) -> String {
    if path == "/dev/null" {
        return path.to_string();
    }
    for prefix in ["a/", "b/", "i/", "w/", "c/", "o/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    path.to_string()
}

/// Quote a path for a patch header if it contains characters that would break
/// line parsing. Spaces are accepted unquoted by `git apply`.
fn patch_path(prefix: &str, path: &str) -> String {
    let needs_quote = path
        .chars()
        .any(|ch| ch == '"' || ch == '\\' || ch == '\n' || ch == '\r' || ch == '\t');
    if needs_quote {
        format!("\"{}{}\"", prefix, escape_git_path(path))
    } else {
        format!("{}{}", prefix, path)
    }
}

fn escape_git_path(path: &str) -> String {
    let mut out = String::new();
    for byte in path.bytes() {
        match byte {
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            byte if !(0x20..0x7f).contains(&byte) => out.push_str(&format!("\\{:03o}", byte)),
            byte => out.push(byte as char),
        }
    }
    out
}

// ── Internal builder types ──────────────────────────────────────────

struct DiffFileBuilder {
    old_path: String,
    new_path: String,
    status: FileStatus,
    hunks: Vec<HunkBuilder>,
}

impl DiffFileBuilder {
    fn build(self) -> Option<DiffFile> {
        if self.hunks.is_empty() {
            return None;
        }

        let hunks: Vec<Hunk> = self.hunks.into_iter().map(HunkBuilder::build).collect();

        let mut status = self.status;
        if self.old_path == "/dev/null" {
            status = FileStatus::Added;
        } else if self.new_path == "/dev/null" {
            status = FileStatus::Deleted;
        } else if self.old_path != self.new_path {
            status = FileStatus::Renamed;
        }

        Some(DiffFile {
            status,
            old_path: self.old_path,
            new_path: self.new_path,
            hunks,
        })
    }
}

struct HunkBuilder {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    header: String,
    lines: Vec<DiffLine>,
    add_count: usize,
    del_count: usize,
}

impl HunkBuilder {
    fn build(self) -> Hunk {
        Hunk {
            old_start: self.old_start,
            old_count: self.old_count,
            new_start: self.new_start,
            new_count: self.new_count,
            header: self.header,
            lines: self.lines,
            staged: false,
        }
    }
}

/// Parse `@@ -start,count +start,count @@ header` keeping the original header
/// text (including the function context after the closing `@@`).
fn parse_hunk_header(line: &str) -> Option<HunkBuilder> {
    let after = line.strip_prefix("@@")?;
    let (range_part, _section) = after.split_once("@@")?;
    let range_part = range_part.trim();
    let (old_part, new_part) = range_part.split_once(' ')?;

    let (old_start, old_count) = parse_range(old_part)?;
    let (new_start, new_count) = parse_range(new_part)?;

    Some(HunkBuilder {
        old_start,
        old_count,
        new_start,
        new_count,
        header: line.to_string(),
        lines: Vec::new(),
        add_count: 0,
        del_count: 0,
    })
}

/// Parse `-start,count` or `+start,count` into (start, count).
fn parse_range(s: &str) -> Option<(usize, usize)> {
    let s = s.strip_prefix('-').or_else(|| s.strip_prefix('+'))?;
    if let Some((start, count)) = s.split_once(',') {
        Some((start.parse().ok()?, count.parse().ok()?))
    } else {
        Some((s.parse().ok()?, 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    // ── Parser ──────────────────────────────────────────────────────

    #[test]
    fn parses_hunks_and_line_numbers() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index 1111111..2222222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@ fn main() {
 context
-old
+new
+extra
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.status, FileStatus::Modified);
        assert_eq!(file.display_path(), "src/main.rs");
        let hunk = &file.hunks[0];
        assert_eq!(hunk.header, "@@ -1,3 +1,4 @@ fn main() {");
        assert_eq!(hunk.lines.len(), 4);
        assert_eq!(hunk.lines[0].old_lineno, Some(1));
        assert_eq!(hunk.lines[0].new_lineno, Some(1));
        assert_eq!(hunk.lines[1].old_lineno, Some(2));
        assert_eq!(hunk.lines[2].new_lineno, Some(2));
        assert_eq!(hunk.lines[3].new_lineno, Some(3));
    }

    #[test]
    fn parses_mnemonic_prefixes_and_header_tabs() {
        let diff = "\
diff --git i/a b.txt w/a b.txt
index 1111111..2222222 100644
--- i/a b.txt	
+++ w/a b.txt	
@@ -1 +1,2 @@
 one
+two
";
        let files = parse_unified_diff(diff);
        assert_eq!(files[0].old_path, "a b.txt");
        assert_eq!(files[0].new_path, "a b.txt");
        assert_eq!(files[0].hunks[0].lines[1].content, "two");
    }

    #[test]
    fn parses_quoted_paths() {
        let diff = "\
diff --git \"a/foo bar.txt\" \"b/foo bar.txt\"
index 1111111..2222222 100644
--- \"a/foo bar.txt\"
+++ \"b/foo bar.txt\"
@@ -1 +1,2 @@
 one
+two
";
        let files = parse_unified_diff(diff);
        assert_eq!(files[0].old_path, "foo bar.txt");
        assert_eq!(files[0].new_path, "foo bar.txt");
    }

    #[test]
    fn content_starting_with_header_markers_stays_content() {
        let diff = "\
diff --git a/text b/text
index 1111111..2222222 100644
--- a/text
+++ b/text
@@ -1 +1,2 @@
 keep
++++ not a file header
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hunks[0].lines[1].kind, DiffLineKind::Add);
        assert_eq!(files[0].hunks[0].lines[1].content, "+++ not a file header");
    }

    #[test]
    fn records_no_newline_markers() {
        let diff = "\
diff --git a/text b/text
index 1111111..2222222 100644
--- a/text
+++ b/text
@@ -1,2 +1,2 @@
 one
-two
\\ No newline at end of file
+two
";
        let files = parse_unified_diff(diff);
        let lines = &files[0].hunks[0].lines;
        assert!(lines[1].no_newline);
        assert!(!lines[2].no_newline);
    }

    #[test]
    fn added_and_deleted_files_are_classified() {
        let added = "\
diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+hello
";
        let deleted = "\
diff --git a/old.txt b/old.txt
deleted file mode 100644
--- a/old.txt
+++ /dev/null
@@ -1 +0,0 @@
-goodbye
";
        assert_eq!(parse_unified_diff(added)[0].status, FileStatus::Added);
        assert_eq!(parse_unified_diff(deleted)[0].status, FileStatus::Deleted);
    }

    #[test]
    fn hunk_patch_targets_dev_null_for_new_and_deleted_files() {
        let added = parse_unified_diff(
            "diff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hello\n",
        );
        let patch = hunk_to_patch(&added[0].hunks[0], &added[0]);
        assert!(patch.contains("--- /dev/null\n+++ b/new.txt\n"));

        let deleted = parse_unified_diff(
            "diff --git a/old.txt b/old.txt\ndeleted file mode 100644\n--- a/old.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-goodbye\n",
        );
        let patch = hunk_to_patch(&deleted[0].hunks[0], &deleted[0]);
        assert!(patch.contains("--- a/old.txt\n+++ /dev/null\n"));
    }

    #[test]
    fn hunk_patch_round_trips_no_newline_marker() {
        let files = parse_unified_diff(
            "diff --git a/text b/text\n--- a/text\n+++ b/text\n@@ -1,2 +1,2 @@\n one\n-two\n\\ No newline at end of file\n+two\n",
        );
        let patch = hunk_to_patch(&files[0].hunks[0], &files[0]);
        assert!(patch.contains("-two\n\\ No newline at end of file\n"));
    }

    // ── Integration with a temporary repository ────────────────────

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    struct TempRepo {
        path: PathBuf,
    }

    impl TempRepo {
        fn new() -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!(
                "ijevim-review-test-{}-{}",
                std::process::id(),
                id
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            let repo = Self { path };
            repo.git(&["init", "-q"]);
            repo.git(&["config", "user.email", "test@example.com"]);
            repo.git(&["config", "user.name", "Test"]);
            repo
        }

        fn git(&self, args: &[&str]) {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.path)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn write(&self, name: &str, content: &str) {
            fs::write(self.path.join(name), content).unwrap();
        }

        fn commit_all(&self, message: &str) {
            self.git(&["add", "-A"]);
            self.git(&["commit", "-qm", message]);
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn working_diff(root: &Path) -> Vec<DiffFile> {
        let output = run_diff(root, &["diff", "HEAD", "--unified=3"], &[]).unwrap();
        parse_unified_diff(&output)
    }

    fn cached_diff(root: &Path) -> String {
        run_diff(root, &["diff", "--cached", "--unified=3"], &[]).unwrap()
    }

    #[test]
    fn stages_hunk_in_file_with_spaces() {
        let repo = TempRepo::new();
        repo.write("a b.txt", "one\ntwo\n");
        repo.commit_all("init");
        repo.write("a b.txt", "one\ntwo\nthree\n");

        let files = working_diff(&repo.path);
        assert_eq!(files[0].display_path(), "a b.txt");
        stage_hunk(&files[0].hunks[0], &files[0], &repo.path).unwrap();
        assert!(cached_diff(&repo.path).contains("+three"));
    }

    #[test]
    fn stages_hunk_without_trailing_newline() {
        let repo = TempRepo::new();
        repo.write("text.txt", "one\ntwo");
        repo.commit_all("init");
        repo.write("text.txt", "one\ntwo\nthree\n");

        let files = working_diff(&repo.path);
        assert!(files[0].hunks[0].lines.iter().any(|line| line.no_newline));
        stage_hunk(&files[0].hunks[0], &files[0], &repo.path).unwrap();
        assert!(cached_diff(&repo.path).contains("+three"));
    }

    #[test]
    fn stages_added_and_deleted_files() {
        let repo = TempRepo::new();
        repo.write("old.txt", "goodbye\n");
        repo.commit_all("init");

        fs::remove_file(repo.path.join("old.txt")).unwrap();
        repo.write("new file.txt", "hello\n");
        repo.git(&["add", "-N", "new file.txt"]);

        let files = working_diff(&repo.path);
        let added = files
            .iter()
            .find(|file| file.status == FileStatus::Added)
            .unwrap();
        stage_hunk(&added.hunks[0], added, &repo.path).unwrap();
        assert!(cached_diff(&repo.path).contains("+hello"));

        let files = working_diff(&repo.path);
        let deleted = files
            .iter()
            .find(|file| file.status == FileStatus::Deleted)
            .unwrap();
        stage_hunk(&deleted.hunks[0], deleted, &repo.path).unwrap();
        assert!(cached_diff(&repo.path).contains("-goodbye"));
    }

    #[test]
    fn staged_status_is_derived_from_the_index() {
        let repo = TempRepo::new();
        repo.write("text.txt", "one\n");
        repo.commit_all("init");
        repo.write("text.txt", "one\ntwo\n");

        let files = working_diff(&repo.path);
        stage_hunk(&files[0].hunks[0], &files[0], &repo.path).unwrap();

        let reloaded = load_diff(&ReviewArgs::WorkingTree, &repo.path).unwrap();
        assert!(reloaded[0].hunks[0].staged);

        unstage_hunk(&reloaded[0].hunks[0], &reloaded[0], &repo.path).unwrap();
        let reloaded = load_diff(&ReviewArgs::WorkingTree, &repo.path).unwrap();
        assert!(!reloaded[0].hunks[0].staged);
    }

    #[test]
    fn file_stage_and_unstage_round_trip() {
        let repo = TempRepo::new();
        repo.write("text.txt", "one\n");
        repo.commit_all("init");
        repo.write("text.txt", "one\ntwo\n");

        let files = working_diff(&repo.path);
        stage_file(&files[0], &repo.path).unwrap();
        assert!(cached_diff(&repo.path).contains("+two"));

        let files = working_diff(&repo.path);
        unstage_file(&files[0], &repo.path).unwrap();
        assert!(!cached_diff(&repo.path).contains("+two"));
    }

    #[test]
    fn discards_worktree_changes() {
        let repo = TempRepo::new();
        repo.write("text.txt", "one\n");
        repo.commit_all("init");
        repo.write("text.txt", "one\ntwo\n");

        let files = working_diff(&repo.path);
        discard_file(&files[0], &repo.path).unwrap();
        assert_eq!(
            fs::read_to_string(repo.path.join("text.txt")).unwrap(),
            "one\n"
        );
        assert!(working_diff(&repo.path).is_empty());
    }
}

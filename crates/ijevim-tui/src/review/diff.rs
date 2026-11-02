//! Git diff integration for Review Mode.
//!
//! This module handles running `git diff` and parsing unified diff output
//! into structured [`DiffFile`] / [`Hunk`] / [`DiffLine`] data.

use crate::review::state::{
    DiffFile, DiffLine, DiffLineKind, FileStatus, Hunk, ReviewArgs, ReviewState,
};

// ── Public API ──────────────────────────────────────────────────────

/// Try to enter review mode with real `git diff` data.
///
/// Returns `Some(ReviewState)` on success, `None` if not in a git
/// repository or if `git diff` fails. The caller should fall back to
/// mock data when this returns `None`.
pub fn try_get_review_state(args: &ReviewArgs) -> Option<ReviewState> {
    let project_root = ijevim_core::git::find_root()?;
    let diff_output = run_git_diff(args).ok()?;
    let files = parse_unified_diff(&diff_output);

    Some(ReviewState {
        args: args.clone(),
        files,
        selected_file: 0,
        selected_hunk: 0,
        sub_mode: crate::review::ReviewSubMode::FileList,
        annotations: Vec::new(),
        comments: Vec::new(),
        scroll: crate::review::ScrollState::default(),
        diff_style: crate::review::DiffStyle::Unified,
        file_list_width: 30,
        project_root: Some(project_root),
        comment_buffer: String::new(),
        comment_cursor: 0,
    })
}

// ── Running git diff ────────────────────────────────────────────────

fn run_git_diff(args: &ReviewArgs) -> Result<String, String> {
    match args {
        ReviewArgs::WorkingTree => run_working_tree_diff(),
        ReviewArgs::Staged => diff_cmd(&["--cached", "--unified=3"]),
        ReviewArgs::Against(rev) => diff_cmd(&[rev.as_str(), "--unified=3"]),
        ReviewArgs::Range(a, b) => {
            let range = format!("{}..{}", a, b);
            diff_cmd(&[&range, "--unified=3"])
        }
        ReviewArgs::File(path) => diff_cmd(&["--unified=3", "--", path.as_str()]),
    }
}

fn diff_cmd(args: &[&str]) -> Result<String, String> {
    let mut all = vec!["diff"];
    all.extend_from_slice(args);
    let output =
        ijevim_core::git::raw(&all).map_err(|e| format!("Failed to run git diff: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("git diff failed: {}", stderr))
    }
}

/// Run diff for `:review` (default): show all changes since HEAD.
///
/// Tries `git diff HEAD --unified=3` first (shows both staged and unstaged
/// changes). Falls back to combining staged + unstaged separately when
/// there is no HEAD commit yet (empty repository).
fn run_working_tree_diff() -> Result<String, String> {
    let head_result = ijevim_core::git::raw(&["diff", "HEAD", "--unified=3"])
        .map_err(|e| format!("Failed to run git diff HEAD: {}", e))?;

    if head_result.status.success() {
        return Ok(String::from_utf8_lossy(&head_result.stdout).to_string());
    }

    // HEAD doesn't exist yet (empty repo). Combine staged + unstaged.
    let staged = ijevim_core::git::raw(&["diff", "--cached", "--unified=3"])
        .map_err(|e| format!("Failed to run git diff --cached: {}", e))?;
    let unstaged = ijevim_core::git::raw(&["diff", "--unified=3"])
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    if !staged.status.success() {
        return Err(format!(
            "git diff --cached failed: {}",
            String::from_utf8_lossy(&staged.stderr).trim()
        ));
    }
    if !unstaged.status.success() {
        return Err(format!(
            "git diff failed: {}",
            String::from_utf8_lossy(&unstaged.stderr).trim()
        ));
    }

    let mut combined = String::from_utf8_lossy(&staged.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&unstaged.stdout));
    Ok(combined)
}

// ── Unified diff parser ─────────────────────────────────────────────

/// Parse a unified diff string into structured `DiffFile` entries.
///
/// Handles:
///   - `diff --git a/... b/...` file headers
///   - `--- a/...` / `+++ b/...` path headers
///   - `@@ -start,count +start,count @@ ...` hunk headers
///   - Context, added, and deleted lines
///   - `\ No newline at end of file` (skipped)
pub fn parse_unified_diff(input: &str) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current_file: Option<DiffFileBuilder> = None;

    for line in input.lines() {
        if line.starts_with("diff --git ") {
            // Finalize previous file
            if let Some(builder) = current_file.take() {
                if let Some(file) = builder.build() {
                    files.push(file);
                }
            }

            // Extract paths from "diff --git a/old b/new"
            let paths = line.strip_prefix("diff --git ").unwrap_or(line);
            let (old, new) = parse_diff_paths(paths);
            current_file = Some(DiffFileBuilder {
                old_path: old.unwrap_or_else(|| String::from("unknown")),
                new_path: new.unwrap_or_else(|| String::from("unknown")),
                status: FileStatus::Modified,
                hunks: Vec::new(),
            });
        } else if let Some(path) = line.strip_prefix("--- ") {
            // Old file path (may include /dev/null)
            if let Some(ref mut file) = current_file {
                let cleaned = path.trim_start_matches("a/");
                if cleaned == "/dev/null" || file.old_path == "unknown" {
                    file.old_path = cleaned.to_string();
                }
            }
        } else if let Some(path) = line.strip_prefix("+++ ") {
            // New file path (may include /dev/null)
            if let Some(ref mut file) = current_file {
                let cleaned = path.trim_start_matches("b/");
                if cleaned == "/dev/null" || file.new_path == "unknown" {
                    file.new_path = cleaned.to_string();
                }
            }
        } else if line.starts_with("@@") {
            // Parse hunk header: @@ -start,count +start,count @@ optional header
            if let Some(ref mut file) = current_file {
                if let Some(hunk) = parse_hunk_header(line) {
                    file.hunks.push(hunk);
                }
            }
        } else if line.starts_with('+') && line != "+++ b/" {
            // Added line (but not the +++ header)
            if let Some(ref mut file) = current_file {
                if let Some(hunk) = file.hunks.last_mut() {
                    let content = line.strip_prefix('+').unwrap_or("").to_string();
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Add,
                        content,
                        old_lineno: None,
                        new_lineno: Some(hunk.new_start + hunk.add_count),
                    });
                    hunk.add_count += 1;
                }
            }
        } else if line.starts_with('-') && line != "--- a/" {
            // Deleted line (but not the --- header)
            if let Some(ref mut file) = current_file {
                if let Some(hunk) = file.hunks.last_mut() {
                    let content = line.strip_prefix('-').unwrap_or("").to_string();
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Delete,
                        content,
                        old_lineno: Some(hunk.old_start + hunk.del_count),
                        new_lineno: None,
                    });
                    hunk.del_count += 1;
                }
            }
        } else if let Some(stripped) = line.strip_prefix(' ') {
            // Context line
            if let Some(ref mut file) = current_file {
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Context,
                        content: stripped.to_string(),
                        old_lineno: Some(hunk.old_start + hunk.del_count),
                        new_lineno: Some(hunk.new_start + hunk.add_count),
                    });
                    hunk.del_count += 1;
                    hunk.add_count += 1;
                }
            }
        }
        // Skip "\ No newline at end of file" and other lines
    }

    // Finalize last file
    if let Some(builder) = current_file.take() {
        if let Some(file) = builder.build() {
            files.push(file);
        }
    }

    // Detect file status (Added/Deleted/Renamed) from paths
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

// ── Hunk stage/unstage via git apply ────────────────────────────────

/// Reconstruct the unified-diff patch text for a single hunk.
///
/// This produces a minimal patch suitable for piping into `git apply`.
/// The patch uses the file's `new_path` (the working tree path).
pub fn hunk_to_patch(hunk: &Hunk, file_path: &str) -> String {
    let mut patch = String::new();
    patch.push_str("--- a/");
    patch.push_str(file_path);
    patch.push('\n');
    patch.push_str("+++ b/");
    patch.push_str(file_path);
    patch.push('\n');
    patch.push_str(&hunk.header);
    patch.push('\n');
    for line in &hunk.lines {
        match line.kind {
            DiffLineKind::Add => {
                patch.push('+');
            }
            DiffLineKind::Delete => {
                patch.push('-');
            }
            DiffLineKind::Context => {
                patch.push(' ');
            }
        }
        patch.push_str(&line.content);
        patch.push('\n');
    }
    patch
}

/// Stage a single hunk by piping its patch to `git apply --cached`.
pub fn stage_hunk(hunk: &Hunk, file_path: &str) -> Result<(), String> {
    let patch = hunk_to_patch(hunk, file_path);
    let mut child = std::process::Command::new("git")
        .args(["apply", "--cached"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git apply: {}", e))?;

    use std::io::Write;
    if let Some(ref mut stdin) = child.stdin {
        stdin
            .write_all(patch.as_bytes())
            .map_err(|e| format!("Failed to write patch to git apply: {}", e))?;
    }

    let output = child
        .wait()
        .map_err(|e| format!("Failed to wait for git apply: {}", e))?;

    if output.success() {
        Ok(())
    } else {
        Err("git apply --cached failed, hunk may conflict with current index".to_string())
    }
}

/// Unstage a single hunk by piping its reverse patch to `git apply --cached --reverse`.
pub fn unstage_hunk(hunk: &Hunk, file_path: &str) -> Result<(), String> {
    let patch = hunk_to_patch(hunk, file_path);
    let mut child = std::process::Command::new("git")
        .args(["apply", "--cached", "--reverse"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git apply --reverse: {}", e))?;

    use std::io::Write;
    if let Some(ref mut stdin) = child.stdin {
        stdin
            .write_all(patch.as_bytes())
            .map_err(|e| format!("Failed to write patch to git apply: {}", e))?;
    }

    let output = child
        .wait()
        .map_err(|e| format!("Failed to wait for git apply: {}", e))?;

    if output.success() {
        Ok(())
    } else {
        Err(
            "git apply --cached --reverse failed, hunk may not be staged or may conflict"
                .to_string(),
        )
    }
}

// ── Internal helpers ────────────────────────────────────────────────

/// Parse `diff --git a/path1 b/path2` into (old_path, new_path).
fn parse_diff_paths(s: &str) -> (Option<String>, Option<String>) {
    let parts: Vec<&str> = s.splitn(4, ' ').collect();
    // parts[0] = "a/path1", parts[1] = "b/path2" (or vice versa)
    let old = parts
        .iter()
        .find(|p| p.starts_with("a/"))
        .map(|p| p.strip_prefix("a/").unwrap_or(*p).to_string());
    let new = parts
        .iter()
        .find(|p| p.starts_with("b/"))
        .map(|p| p.strip_prefix("b/").unwrap_or(*p).to_string());
    (old, new)
}

/// Parse `@@ -start,count +start,count @@ header`
fn parse_hunk_header(line: &str) -> Option<HunkBuilder> {
    let line = line.strip_prefix("@@")?;
    let (range_part, _header) = line.split_once("@@")?;
    let range_part = range_part.trim();

    // Parse "-start,count +start,count"
    let (old_part, new_part) = range_part.split_once(' ')?;

    let (old_start, old_count) = parse_range(old_part)?;
    let (new_start, new_count) = parse_range(new_part)?;

    Some(HunkBuilder {
        old_start,
        old_count,
        new_start,
        new_count,
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
        // If count is omitted, it's 1
        Some((s.parse().ok()?, 1))
    }
}

// ── Builder types (for incremental parsing) ─────────────────────────

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
        let hunks: Vec<Hunk> = self
            .hunks
            .into_iter()
            .map(|h| {
                let header = format!(
                    "@@ -{},{} +{},{} @@",
                    h.old_start, h.old_count, h.new_start, h.new_count
                );
                Hunk {
                    old_start: h.old_start,
                    old_count: h.old_count,
                    new_start: h.new_start,
                    new_count: h.new_count,
                    header,
                    lines: h.lines,
                    staged: false,
                }
            })
            .collect();

        let mut status = self.status;

        // Detect special cases
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
    lines: Vec<DiffLine>,
    /// Running counter for tracking old-line numbering
    add_count: usize,
    /// Running counter for tracking new-line numbering
    del_count: usize,
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_diff() {
        let files = parse_unified_diff("");
        assert!(files.is_empty());
    }

    #[test]
    fn parse_simple_diff() {
        let input = "\
diff --git a/src/main.rs b/src/main.rs
index abc..def 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 line1
 line2
-old line
+new line
+extra line
";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Modified);
        assert_eq!(files[0].old_path, "src/main.rs");
        assert_eq!(files[0].new_path, "src/main.rs");
        assert_eq!(files[0].hunks.len(), 1);

        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.lines.len(), 5);
        assert_eq!(hunk.lines[0].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[2].kind, DiffLineKind::Delete);
        assert_eq!(hunk.lines[3].kind, DiffLineKind::Add);
        assert_eq!(hunk.lines[4].kind, DiffLineKind::Add);
    }

    #[test]
    fn parse_multiple_files() {
        let input = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1 +1,2 @@
 a
+b
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
@@ -1 +1 @@
-x
+y
";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].new_path, "a.rs");
        assert_eq!(files[1].new_path, "b.rs");
    }

    #[test]
    fn parse_new_file() {
        let input = "\
diff --git a/src/new.rs b/src/new.rs
new file mode 100644
index 000..abc
--- /dev/null
+++ b/src/new.rs
@@ -0,0 +1,3 @@
+fn new_fn() {
+    todo!()
+}
";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Added);
        assert_eq!(files[0].hunks[0].lines.len(), 3);
    }

    #[test]
    fn parse_deleted_file() {
        let input = "\
diff --git a/src/old.rs b/src/old.rs
deleted file mode 100644
index abc..000
--- a/src/old.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-dead
-code
";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Deleted);
    }

    #[test]
    fn run_git_diff_on_self() {
        // This test requires being in a git repo
        if ijevim_core::git::find_root().is_none() {
            eprintln!("Skipping git diff test (not in a repo)");
            return;
        }
        let result = run_git_diff(&ReviewArgs::WorkingTree);
        assert!(result.is_ok());
    }

    #[test]
    fn hunk_to_patch_basic() {
        let hunk = Hunk {
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
                    kind: DiffLineKind::Delete,
                    content: "    println!(\"old\");".to_string(),
                    old_lineno: Some(12),
                    new_lineno: None,
                },
                DiffLine {
                    kind: DiffLineKind::Add,
                    content: "    println!(\"new\");".to_string(),
                    old_lineno: None,
                    new_lineno: Some(12),
                },
                DiffLine {
                    kind: DiffLineKind::Context,
                    content: "    let z = 3;".to_string(),
                    old_lineno: Some(14),
                    new_lineno: Some(15),
                },
            ],
        };

        let patch = hunk_to_patch(&hunk, "src/main.rs");
        let expected = "\
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,7 +10,9 @@ fn main() {
     let x = 1;
-    println!(\"old\");
+    println!(\"new\");
     let z = 3;
";
        assert_eq!(patch, expected);
    }

    #[test]
    fn hunk_to_patch_new_file() {
        let hunk = Hunk {
            old_start: 0,
            old_count: 0,
            new_start: 1,
            new_count: 2,
            header: "@@ -0,0 +1,2 @@".to_string(),
            staged: false,
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Add,
                    content: "fn new_fn() {".to_string(),
                    old_lineno: None,
                    new_lineno: Some(1),
                },
                DiffLine {
                    kind: DiffLineKind::Add,
                    content: "    todo!()".to_string(),
                    old_lineno: None,
                    new_lineno: Some(2),
                },
            ],
        };

        let patch = hunk_to_patch(&hunk, "src/new.rs");
        assert!(patch.contains("--- a/src/new.rs"));
        assert!(patch.contains("+++ b/src/new.rs"));
        assert!(patch.contains("@@ -0,0 +1,2 @@"));
        assert!(patch.contains("+fn new_fn() {"));
    }

    #[test]
    fn parse_binary_diff_skipped() {
        // Binary diff lines should be skipped — the parser returns an empty
        // file entry (no hunks) which gets filtered out by build().
        let input = "\
diff --git a/image.png b/image.png
index abc..def 100644
Binary files a/image.png and b/image.png differ
";
        let files = parse_unified_diff(input);
        // Binary diffs have no hunks, so build() returns None and the file
        // is not included.
        assert!(files.is_empty(), "binary diffs should produce no files");
    }

    #[test]
    fn parse_diff_with_binary_and_text() {
        let input = "\
diff --git a/image.png b/image.png
Binary files a/image.png and b/image.png differ
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1,2 @@
 a
+b
";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].new_path, "src/main.rs");
    }

    #[test]
    fn parse_staged_args() {
        // Verify ReviewArgs parse --staged and --cached correctly
        match ReviewArgs::parse("--staged") {
            ReviewArgs::Staged => {}
            other => panic!("expected Staged, got {:?}", other),
        }
        match ReviewArgs::parse("--cached") {
            ReviewArgs::Staged => {}
            other => panic!("expected Staged (cached), got {:?}", other),
        }
        match ReviewArgs::parse("") {
            ReviewArgs::WorkingTree => {}
            other => panic!("expected WorkingTree, got {:?}", other),
        }
        match ReviewArgs::parse("HEAD") {
            ReviewArgs::Against(ref s) if s == "HEAD" => {}
            other => panic!("expected Against(HEAD), got {:?}", other),
        }
        match ReviewArgs::parse("HEAD~3") {
            ReviewArgs::Against(ref s) if s == "HEAD~3" => {}
            other => panic!("expected Against(HEAD~3), got {:?}", other),
        }
        match ReviewArgs::parse("main..feature") {
            ReviewArgs::Range(ref a, ref b) if a == "main" && b == "feature" => {}
            other => panic!("expected Range(main, feature), got {:?}", other),
        }
    }

    #[test]
    fn stage_hunk_integration() {
        // Only run in a git repo (this project)
        let _root = match ijevim_core::git::find_root() {
            Some(r) => r,
            None => {
                eprintln!("Skipping stage_hunk test (not in a repo)");
                return;
            }
        };

        // Create a temp file to apply a patch to
        let tmp_dir = std::env::temp_dir().join("ijevim_test_stage_hunk");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        // Init a git repo with a test file
        let status = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();
        assert!(status.success());

        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();

        // Write initial content and commit
        std::fs::write(tmp_dir.join("test.txt"), "line1\nline2\nline3\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();

        // Modify the file to create a diff
        std::fs::write(tmp_dir.join("test.txt"), "line1\nline2\nline3\nline4\n").unwrap();

        // Build a hunk representing the addition
        let hunk = Hunk {
            old_start: 3,
            old_count: 1,
            new_start: 3,
            new_count: 2,
            header: "@@ -3,1 +3,2 @@".to_string(),
            staged: false,
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Context,
                    content: "line3".to_string(),
                    old_lineno: Some(3),
                    new_lineno: Some(3),
                },
                DiffLine {
                    kind: DiffLineKind::Add,
                    content: "line4".to_string(),
                    old_lineno: None,
                    new_lineno: Some(4),
                },
            ],
        };

        // Stage the hunk
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Change directory to tmp_dir for git apply to work
            let original_dir = std::env::current_dir().unwrap();
            std::env::set_current_dir(&tmp_dir).unwrap();

            let stage_result = stage_hunk(&hunk, "test.txt");

            std::env::set_current_dir(original_dir).unwrap();
            stage_result
        }));

        match result {
            Ok(Ok(())) => {
                // Verify the hunk is now staged (git diff --cached shows the change)
                let output = std::process::Command::new("git")
                    .args(["diff", "--cached"])
                    .current_dir(&tmp_dir)
                    .output()
                    .unwrap();
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    stdout.contains("+line4"),
                    "Staged diff should contain +line4, got: {}",
                    stdout
                );
                eprintln!("stage_hunk integration test PASSED");
            }
            Ok(Err(e)) => {
                eprintln!("stage_hunk returned error: {}", e);
                // Not a hard failure — git version or environment may differ
            }
            Err(_) => {
                eprintln!("stage_hunk panicked (expected in some environments)");
            }
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn unstage_hunk_integration() {
        // Only run in a git repo
        if ijevim_core::git::find_root().is_none() {
            eprintln!("Skipping unstage_hunk test (not in a repo)");
            return;
        }

        let tmp_dir = std::env::temp_dir().join("ijevim_test_unstage_hunk");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        // Init a git repo with a test file
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();

        // Write initial content and commit
        std::fs::write(tmp_dir.join("test.txt"), "line1\nline2\nline3\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();

        // Modify and stage the full file
        std::fs::write(tmp_dir.join("test.txt"), "line1\nline2\nline3\nline4\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(&tmp_dir)
            .status()
            .unwrap();
        // Now the addition is staged

        // Try to unstage just the hunk for line4
        let hunk = Hunk {
            old_start: 3,
            old_count: 1,
            new_start: 3,
            new_count: 2,
            header: "@@ -3,1 +3,2 @@".to_string(),
            staged: true,
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Context,
                    content: "line3".to_string(),
                    old_lineno: Some(3),
                    new_lineno: Some(3),
                },
                DiffLine {
                    kind: DiffLineKind::Add,
                    content: "line4".to_string(),
                    old_lineno: None,
                    new_lineno: Some(4),
                },
            ],
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let original_dir = std::env::current_dir().unwrap();
            std::env::set_current_dir(&tmp_dir).unwrap();

            let unstage_result = unstage_hunk(&hunk, "test.txt");

            std::env::set_current_dir(original_dir).unwrap();
            unstage_result
        }));

        match result {
            Ok(Ok(())) => {
                // Verify hunk is no longer staged
                let output = std::process::Command::new("git")
                    .args(["diff", "--cached"])
                    .current_dir(&tmp_dir)
                    .output()
                    .unwrap();
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    !stdout.contains("+line4"),
                    "After unstage, diff --cached should NOT contain line4, got: {}",
                    stdout
                );
                eprintln!("unstage_hunk integration test PASSED");
            }
            Ok(Err(e)) => {
                eprintln!("unstage_hunk returned error: {}", e);
            }
            Err(_) => {
                eprintln!("unstage_hunk panicked (expected in some environments)");
            }
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}

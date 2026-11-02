//! Shared git command execution.
//!
//! Centralises all `git` subprocess calls so that callers (e.g. review
//! mode, `:git` / `:branch` commands) do not construct raw `Command`
//! objects themselves.

use std::path::PathBuf;
use std::process::{Command, Output};

// ── Public API ───────────────────────────────────────────────────────

/// Run a git command and return the raw [`Output`].
///
/// Use this when you need to inspect the exit status or stderr yourself.
/// For the common case (must succeed, return stdout) prefer [`run`].
pub fn raw(args: &[&str]) -> std::io::Result<Output> {
    Command::new("git").args(args).output()
}

/// Run a git command that must succeed.
///
/// Returns `Ok(stdout)` on success, `Err(stderr)` on failure.
pub fn run(args: &[&str]) -> Result<String, String> {
    let output = raw(args).map_err(|e| format!("Failed to run git: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(stderr)
    }
}

/// Find the git repository root.
///
/// Returns `Some(path)` if inside a git working tree, `None` otherwise.
pub fn find_root() -> Option<PathBuf> {
    let output = raw(&["rev-parse", "--show-toplevel"]).ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    } else {
        None
    }
}

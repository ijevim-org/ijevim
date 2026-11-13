//! Annotation storage for Review Mode.
//!
//! Annotations are comments attached to specific lines in a diff.
//! They are persisted as JSON under the repository's git directory
//! (`.git/ijevim/review/annotations.json`) so review notes never dirty the
//! working tree or get committed by accident.
//!
//! The legacy location `.review/annotations.json` is still read when no
//! current file exists, so existing notes survive the move.
//!
//! Format (version 2, version 1 files are accepted):
//! ```json
//! {
//!   "version": 2,
//!   "annotations": [
//!     {
//!       "id": "ann-1717776000000000000",
//!       "file_path": "src/main.rs",
//!       "line": 42,
//!       "end_line": 42,
//!       "text": "this might be buggy",
//!       "resolved": false,
//!       "created_at": "2026-06-08T12:00:00Z"
//!     }
//!   ]
//! }
//! ```

use crate::review::state::Annotation;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The filename used for annotation storage.
const ANNOTATIONS_FILE: &str = "annotations.json";
/// Legacy worktree-local review directory.
const LEGACY_REVIEW_DIR: &str = ".review";
/// Current format version written to disk.
const FORMAT_VERSION: u32 = 2;

/// Load annotations for a given project root.
///
/// Reads the current location first, then the legacy `.review/` location.
/// Returns an empty vec if neither file exists or can't be read.
pub fn load_annotations(project_root: &Path) -> Vec<Annotation> {
    for path in [
        annotations_path(project_root),
        legacy_annotations_path(project_root),
    ] {
        if let Ok(content) = std::fs::read_to_string(&path) {
            return parse_annotations_json(&content);
        }
    }
    Vec::new()
}

/// Save a single annotation and return the updated list.
///
/// Reads existing annotations, appends the new one, writes back.
pub fn add_annotation(
    project_root: &Path,
    annotation: Annotation,
) -> Result<Vec<Annotation>, String> {
    let mut annotations = load_annotations(project_root);
    annotations.push(annotation);
    save_annotations(project_root, &annotations)?;
    Ok(annotations)
}

/// Remove an annotation by its id.
pub fn remove_annotation(project_root: &Path, id: &str) -> Result<Vec<Annotation>, String> {
    let annotations = load_annotations(project_root);
    let filtered: Vec<Annotation> = annotations.into_iter().filter(|a| a.id != id).collect();
    save_annotations(project_root, &filtered)?;
    Ok(filtered)
}

/// Toggle resolved state for an annotation by id.
pub fn toggle_resolved(project_root: &Path, id: &str) -> Result<Vec<Annotation>, String> {
    let mut annotations = load_annotations(project_root);
    for a in &mut annotations {
        if a.id == id {
            a.resolved = !a.resolved;
            break;
        }
    }
    save_annotations(project_root, &annotations)?;
    Ok(annotations)
}

/// Get annotations for a specific file.
pub fn annotations_for_file<'a>(
    annotations: &'a [Annotation],
    file_path: &str,
) -> Vec<&'a Annotation> {
    annotations
        .iter()
        .filter(|a| a.file_path == file_path)
        .collect()
}

/// Generate a unique annotation id based on current timestamp.
pub fn generate_annotation_id() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("ann-{}", since_epoch.as_nanos())
}

/// Current UTC time as an ISO 8601 / RFC 3339 timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
pub fn now_iso8601() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format_iso8601(since_epoch)
}

/// Format a Unix timestamp (seconds) as `YYYY-MM-DDTHH:MM:SSZ`.
fn format_iso8601(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let seconds_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

/// Convert days since 1970-01-01 into a proleptic Gregorian (year, month, day).
///
/// Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ── JSON helpers (no serde dependency) ────────────────────────────────

/// Path to the annotations file for a given project root.
fn annotations_path(project_root: &Path) -> PathBuf {
    review_dir(project_root).join(ANNOTATIONS_FILE)
}

/// Legacy worktree-local path, read-only.
fn legacy_annotations_path(project_root: &Path) -> PathBuf {
    project_root.join(LEGACY_REVIEW_DIR).join(ANNOTATIONS_FILE)
}

/// Current review data directory: `<git-dir>/ijevim/review`.
fn review_dir(project_root: &Path) -> PathBuf {
    git_dir(project_root).join("ijevim").join("review")
}

/// Resolve the repository's git directory without changing the cwd.
///
/// Handles the `.git` file used by worktrees and submodules (`gitdir: ...`).
fn git_dir(project_root: &Path) -> PathBuf {
    let dot_git = project_root.join(".git");
    if dot_git.is_dir() {
        return dot_git;
    }
    if let Ok(content) = std::fs::read_to_string(&dot_git) {
        if let Some(rest) = content.trim().strip_prefix("gitdir:") {
            let path = PathBuf::from(rest.trim());
            return if path.is_absolute() {
                path
            } else {
                project_root.join(path)
            };
        }
    }
    dot_git
}

/// Save annotations to the JSON file.
fn save_annotations(project_root: &Path, annotations: &[Annotation]) -> Result<(), String> {
    let review_dir = review_dir(project_root);
    std::fs::create_dir_all(&review_dir)
        .map_err(|e| format!("Failed to create review directory: {}", e))?;

    let json = annotations_to_json(annotations);
    let path = annotations_path(project_root);
    std::fs::write(&path, &json).map_err(|e| format!("Failed to write annotations: {}", e))?;
    Ok(())
}

/// Convert annotations to JSON string.
fn annotations_to_json(annotations: &[Annotation]) -> String {
    let mut json = format!(
        "{{\n  \"version\": {},\n  \"annotations\": [\n",
        FORMAT_VERSION
    );
    for (i, a) in annotations.iter().enumerate() {
        if i > 0 {
            json.push_str(",\n");
        }
        json.push_str("    {");
        json.push_str(&format!(r#""id": "{}""#, escape_json(&a.id)));
        json.push_str(", ");
        json.push_str(&format!(r#""file_path": "{}""#, escape_json(&a.file_path)));
        json.push_str(", ");
        json.push_str(&format!(r#""line": {}"#, a.line));
        json.push_str(", ");
        match a.end_line {
            Some(end) => json.push_str(&format!(r#""end_line": {}"#, end)),
            None => json.push_str(r#""end_line": null"#),
        }
        json.push_str(", ");
        json.push_str(&format!(r#""text": "{}""#, escape_json(&a.text)));
        json.push_str(", ");
        json.push_str(&format!(
            r#""resolved": {}"#,
            if a.resolved { "true" } else { "false" }
        ));
        json.push_str(", ");
        json.push_str(&format!(
            r#""created_at": "{}""#,
            escape_json(&a.created_at)
        ));
        json.push('}');
    }
    json.push_str("\n  ]\n}\n");
    json
}

/// Parse annotations from a JSON string.
fn parse_annotations_json(content: &str) -> Vec<Annotation> {
    let mut annotations = Vec::new();

    // Find the "annotations" array
    let array_start = match content.find("\"annotations\"") {
        Some(pos) => {
            // Find the opening bracket after the colon
            let after = &content[pos..];
            match after.find('[') {
                Some(bracket) => pos + bracket + 1,
                None => return annotations,
            }
        }
        None => return annotations,
    };

    let mut i = array_start;
    let bytes = content.as_bytes();

    loop {
        // Skip whitespace
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        if bytes[i] == b']' {
            break;
        }
        if bytes[i] == b'{' {
            // Parse one annotation object
            let end = find_matching_brace(content, i);
            if let Some(end_pos) = end {
                if let Some(ann) = parse_annotation_object(&content[i..=end_pos]) {
                    annotations.push(ann);
                }
                i = end_pos + 1;
            } else {
                break;
            }
        } else if bytes[i] == b',' {
            i += 1;
        } else {
            break;
        }
    }

    annotations
}

/// Find the matching closing brace for an opening brace at position `start`.
fn find_matching_brace(content: &str, start: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    if start >= bytes.len() || bytes[start] != b'{' {
        return None;
    }
    let mut depth = 1u32;
    let mut i = start + 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'"' => {
                // Skip string content
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        break;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if depth == 0 {
        Some(i - 1)
    } else {
        None
    }
}

/// Parse a single annotation JSON object.
fn parse_annotation_object(obj: &str) -> Option<Annotation> {
    let mut id = String::new();
    let mut file_path = String::new();
    let mut line = 0usize;
    let mut end_line: Option<usize> = None;
    let mut text = String::new();
    let mut resolved = false;
    let mut created_at = String::new();

    // Simple key-value extraction
    if let Some(inner) = obj.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        let mut pos = 0;
        let bytes = inner.as_bytes();
        while pos < bytes.len() {
            // Skip whitespace and commas
            while pos < bytes.len() && (bytes[pos].is_ascii_whitespace() || bytes[pos] == b',') {
                pos += 1;
            }
            if pos >= bytes.len() {
                break;
            }

            // Find key
            if bytes[pos] != b'"' {
                break;
            }
            let key_start = pos + 1;
            pos = key_start;
            while pos < bytes.len() && bytes[pos] != b'"' {
                pos += 1;
            }
            let key = &inner[key_start..pos];
            pos += 1; // skip closing quote

            // Skip colon and whitespace
            while pos < bytes.len() && (bytes[pos].is_ascii_whitespace() || bytes[pos] == b':') {
                pos += 1;
            }
            if pos >= bytes.len() {
                break;
            }

            // Parse value
            if bytes[pos] == b'"' {
                // String value
                let val_start = pos + 1;
                pos = val_start;
                while pos < bytes.len() {
                    if bytes[pos] == b'\\' {
                        pos += 2;
                        continue;
                    }
                    if bytes[pos] == b'"' {
                        break;
                    }
                    pos += 1;
                }
                let value = &inner[val_start..pos];
                pos += 1; // skip closing quote

                match key {
                    "id" => id = unescape_json(value),
                    "file_path" => file_path = unescape_json(value),
                    "text" => text = unescape_json(value),
                    "created_at" => created_at = unescape_json(value),
                    _ => {}
                }
            } else {
                // Number or boolean value
                let val_start = pos;
                while pos < bytes.len()
                    && !bytes[pos].is_ascii_whitespace()
                    && bytes[pos] != b','
                    && bytes[pos] != b'}'
                {
                    pos += 1;
                }
                let value = &inner[val_start..pos];
                match key {
                    "line" => line = value.parse().unwrap_or(0),
                    "end_line" => end_line = value.parse().ok(),
                    "resolved" => resolved = value == "true",
                    _ => {}
                }
            }
        }
    }

    if id.is_empty() {
        return None;
    }

    Some(Annotation {
        id,
        file_path,
        line,
        end_line,
        text,
        resolved,
        created_at,
    })
}

/// Escape a string for JSON.
fn escape_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c => result.push(c),
        }
    }
    result
}

/// Unescape a JSON string.
fn unescape_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_roundtrip() {
        let original = r#"hello "world" and
newline and \ backslash"#;
        let escaped = escape_json(original);
        let unescaped = unescape_json(&escaped);
        assert_eq!(original, unescaped);
    }

    #[test]
    fn test_annotations_json_roundtrip() {
        let anns = vec![
            Annotation {
                id: "ann-123".to_string(),
                file_path: "src/main.rs".to_string(),
                line: 42,
                end_line: None,
                text: "check null case".to_string(),
                resolved: false,
                created_at: "2026-06-08T12:00:00Z".to_string(),
            },
            Annotation {
                id: "ann-456".to_string(),
                file_path: "src/lib.rs".to_string(),
                line: 10,
                end_line: Some(12),
                text: r#"needs "quotes" escaped"#.to_string(),
                resolved: true,
                created_at: "2026-06-08T13:00:00Z".to_string(),
            },
        ];

        let json = annotations_to_json(&anns);
        let parsed = parse_annotations_json(&json);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "ann-123");
        assert_eq!(parsed[0].file_path, "src/main.rs");
        assert_eq!(parsed[0].line, 42);
        assert!(!parsed[0].resolved);
        assert_eq!(parsed[1].text, r#"needs "quotes" escaped"#);
        assert!(parsed[1].resolved);
        assert_eq!(parsed[1].end_line, Some(12));
    }

    #[test]
    fn format_iso8601_matches_unix_epochs() {
        assert_eq!(format_iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_iso8601(1_717_776_000), "2024-06-07T16:00:00Z");
    }

    #[test]
    fn reads_legacy_version_1_file() {
        let parsed = parse_annotations_json(
            r#"{"version":1,"annotations":[{"id":"a","file_path":"x","line":3,"text":"old","resolved":false,"created_at":"2026-01-01T00:00:00Z"}]}"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].line, 3);
        assert_eq!(parsed[0].end_line, None);
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ijevim-annotations-{}-{}",
            name,
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        dir
    }

    #[test]
    fn saves_under_git_dir_and_reloads() {
        let root = temp_root("save");
        let annotation = Annotation {
            id: "ann-x".to_string(),
            file_path: "src/main.rs".to_string(),
            line: 7,
            end_line: None,
            text: "note".to_string(),
            resolved: false,
            created_at: now_iso8601(),
        };

        let saved = add_annotation(&root, annotation).unwrap();
        assert_eq!(saved.len(), 1);

        let path = annotations_path(&root);
        assert!(path.starts_with(root.join(".git")));
        assert!(path.exists());
        assert!(!root.join(".review").exists());

        let loaded = load_annotations(&root);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].line, 7);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn falls_back_to_legacy_review_directory() {
        let root = temp_root("legacy");
        let legacy = legacy_annotations_path(&root);
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(
            &legacy,
            r#"{"version":1,"annotations":[{"id":"a","file_path":"x","line":3,"text":"old","resolved":false,"created_at":"2026-01-01T00:00:00Z"}]}"#,
        )
        .unwrap();

        let loaded = load_annotations(&root);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "a");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_parse_empty_json() {
        let parsed = parse_annotations_json(r#"{"version":1,"annotations":[]}"#);
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_parse_no_annotations_key() {
        let parsed = parse_annotations_json(r#"{}"#);
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_generate_id_unique() {
        let id1 = generate_annotation_id();
        let id2 = generate_annotation_id();
        assert_ne!(id1, id2);
    }
}

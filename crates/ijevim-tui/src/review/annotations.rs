//! Annotation storage for Review Mode.
//!
//! Annotations are comments attached to specific lines in a diff.
//! They are persisted as JSON files in `.review/annotations.json`
//! relative to the git project root.
//!
//! Format:
//! ```json
//! {
//!   "version": 1,
//!   "annotations": [
//!     {
//!       "id": "1717776000-0",
//!       "file_path": "src/main.rs",
//!       "hunk_index": 0,
//!       "line_index": 3,
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

/// The filename used for annotation storage inside `.review/`.
const ANNOTATIONS_FILE: &str = "annotations.json";
const REVIEW_DIR: &str = ".review";

/// Load annotations for a given project root.
///
/// Returns an empty vec if the file doesn't exist or can't be read.
pub fn load_annotations(project_root: &Path) -> Vec<Annotation> {
    let path = annotations_path(project_root);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_annotations_json(&content)
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

/// Generate a unique annotation id based on current timestamp + counter.
pub fn generate_annotation_id() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("ann-{}", since_epoch.as_nanos())
}

// ── JSON helpers (no serde dependency) ────────────────────────────────

/// Path to the annotations file for a given project root.
fn annotations_path(project_root: &Path) -> PathBuf {
    let mut path = PathBuf::from(project_root);
    path.push(REVIEW_DIR);
    path.push(ANNOTATIONS_FILE);
    path
}

/// Save annotations to the JSON file.
fn save_annotations(project_root: &Path, annotations: &[Annotation]) -> Result<(), String> {
    let review_dir = project_root.join(REVIEW_DIR);
    std::fs::create_dir_all(&review_dir)
        .map_err(|e| format!("Failed to create .review directory: {}", e))?;

    let json = annotations_to_json(annotations);
    let path = annotations_path(project_root);
    std::fs::write(&path, &json).map_err(|e| format!("Failed to write annotations: {}", e))?;
    Ok(())
}

/// Convert annotations to JSON string.
fn annotations_to_json(annotations: &[Annotation]) -> String {
    let mut json = String::from("{\n  \"version\": 1,\n  \"annotations\": [\n");
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
                text: "check null case".to_string(),
                resolved: false,
                created_at: "2026-06-08T12:00:00Z".to_string(),
            },
            Annotation {
                id: "ann-456".to_string(),
                file_path: "src/lib.rs".to_string(),
                line: 10,
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

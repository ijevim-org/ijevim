//! Temporary differential probe against real Vim.
//!
//! Reads `PROBE_TEXT` (buffer contents) and `PROBE_KEYS` (a key sequence with
//! `<Esc>`, `<Enter>`, `<BS>` and `<C-x>` tokens) from the environment, applies the keys in
//! Vim keymap mode, then writes the buffer to `PROBE_OUT` and
//! `<line> <col>` to `PROBE_POS`.
//!
//! Used by `scripts/vim-diff.sh` to compare behaviour with `vim -es`.

use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::Keymap;

fn parse_keys(keys: &str) -> Vec<(KeyCode, KeyModifiers)> {
    let mut out = Vec::new();
    let mut chars = keys.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut token = String::new();
            let mut closed = false;
            for d in chars.by_ref() {
                if d == '>' {
                    closed = true;
                    break;
                }
                token.push(d);
            }
            if closed {
                let ctrl = token
                    .strip_prefix("C-")
                    .or_else(|| token.strip_prefix("c-"))
                    .and_then(|rest| {
                        let mut it = rest.chars();
                        match (it.next(), it.next()) {
                            (Some(c), None) => Some(c),
                            _ => None,
                        }
                    });
                if let Some(c) = ctrl {
                    out.push((KeyCode::Char(c), KeyModifiers::CONTROL));
                } else {
                    let code = match token.as_str() {
                        "Esc" => KeyCode::Esc,
                        "Enter" | "CR" => KeyCode::Enter,
                        "BS" => KeyCode::Backspace,
                        "Tab" => KeyCode::Tab,
                        "Space" => KeyCode::Char(' '),
                        other => panic!("unsupported key token: <{}>", other),
                    };
                    out.push((code, KeyModifiers::NONE));
                }
                continue;
            }
            out.push((KeyCode::Char('<'), KeyModifiers::NONE));
            for d in token.chars() {
                out.push((KeyCode::Char(d), KeyModifiers::NONE));
            }
            continue;
        }
        out.push((KeyCode::Char(c), KeyModifiers::NONE));
    }
    out
}

#[tokio::test]
async fn probe() {
    let Ok(text) = std::env::var("PROBE_TEXT") else {
        return;
    };
    // The shell passes `\n` / `\t` literally; decode them to real characters.
    let text = text.replace("\\n", "\n").replace("\\t", "\t");
    let keys = std::env::var("PROBE_KEYS").unwrap_or_default();
    let mut ed = Editor::new_headless_for_test(Keymap::Vim).expect("headless editor");
    ed.set_buffer_for_test(&text);
    for (key, modifiers) in parse_keys(&keys) {
        ed.handle_key_for_test(key, modifiers).await;
    }
    let (buf, line, col, _mode) = ed.snapshot_for_test();
    if let Ok(path) = std::env::var("PROBE_OUT") {
        std::fs::write(path, &buf).expect("write buffer");
    }
    if let Ok(path) = std::env::var("PROBE_POS") {
        std::fs::write(path, format!("{} {}", line, col)).expect("write position");
    }
    println!("PROBE_DONE");
}

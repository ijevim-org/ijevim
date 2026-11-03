//! Temporary differential probe against real Vim.
//!
//! Reads `PROBE_TEXT` (buffer contents) and `PROBE_KEYS` (a key sequence with
//! `<Esc>`, `<Enter>`, `<BS>` tokens) from the environment, applies the keys in
//! Vim keymap mode, then writes the buffer to `PROBE_OUT` and
//! `<line> <col>` to `PROBE_POS`.
//!
//! Used by `scripts/vim-diff.sh` to compare behaviour with `vim -es`.

use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::Keymap;

fn parse_keys(keys: &str) -> Vec<KeyCode> {
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
                match token.as_str() {
                    "Esc" => out.push(KeyCode::Esc),
                    "Enter" | "CR" => out.push(KeyCode::Enter),
                    "BS" => out.push(KeyCode::Backspace),
                    "Tab" => out.push(KeyCode::Tab),
                    "Space" => out.push(KeyCode::Char(' ')),
                    other => panic!("unsupported key token: <{}>", other),
                }
                continue;
            }
            out.push(KeyCode::Char('<'));
            for d in token.chars() {
                out.push(KeyCode::Char(d));
            }
            continue;
        }
        out.push(KeyCode::Char(c));
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
    for key in parse_keys(&keys) {
        ed.handle_key_for_test(key, KeyModifiers::NONE).await;
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

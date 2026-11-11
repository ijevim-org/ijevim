//! Differential probe against real Emacs.
//!
//! Reads `PROBE_TEXT` (buffer contents) and `PROBE_KEYS` (a key sequence with
//! `<C-x>`, `<M-x>`, `<C-M-x>`, `<Esc>`, `<Enter>`, `<BS>`, `<Tab>` and
//! `<Space>` tokens) from the environment, applies the keys in Emacs keymap
//! mode, then writes the buffer to `PROBE_OUT` and `<line> <col>` to
//! `PROBE_POS`.
//!
//! Used by `scripts/emacs-diff.sh` to compare behaviour with `emacs -Q --batch`.

use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::Keymap;

fn parse_token(token: &str) -> (KeyCode, KeyModifiers) {
    let mut modifiers = KeyModifiers::NONE;
    let mut rest = token;
    loop {
        if let Some(r) = rest.strip_prefix("C-") {
            modifiers |= KeyModifiers::CONTROL;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("M-") {
            modifiers |= KeyModifiers::ALT;
            rest = r;
        } else {
            break;
        }
    }

    let code = match rest {
        "Esc" => KeyCode::Esc,
        "Enter" | "CR" => KeyCode::Enter,
        "BS" => KeyCode::Backspace,
        "Tab" => KeyCode::Tab,
        "Delete" => KeyCode::Delete,
        "Space" => KeyCode::Char(' '),
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => panic!("unsupported key token: <{}>", token),
            }
        }
    };
    (code, modifiers)
}

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
            if closed && !token.is_empty() {
                out.push(parse_token(&token));
                continue;
            }
            // Not a recognised token: treat the consumed characters literally.
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
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless editor");
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

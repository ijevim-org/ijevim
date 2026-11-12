//! Regression tests for keeping the Emacs keymap independent of Vim.
//!
//! Emacs point semantics differ from Vim normal-mode cursor semantics: point
//! may sit at the end of a line (on the newline) and may cross line breaks.
//! These tests also pin down that keymap-local state does not leak when the
//! user switches between Vim and Emacs.

use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::{Keymap, Mode, Position};

fn editor(keymap: Keymap, text: &str) -> Editor {
    let mut editor = Editor::new_headless_for_test(keymap).expect("headless editor");
    editor.set_buffer_for_test(text);
    editor
}

async fn press(editor: &mut Editor, key: KeyCode, modifiers: KeyModifiers) {
    editor.handle_key_for_test(key, modifiers).await;
}

async fn ctrl(editor: &mut Editor, c: char) {
    press(editor, KeyCode::Char(c), KeyModifiers::CONTROL).await;
}

async fn alt(editor: &mut Editor, c: char) {
    press(editor, KeyCode::Char(c), KeyModifiers::ALT).await;
}

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

#[tokio::test]
async fn ctrl_f_crosses_a_newline_instead_of_clamping() {
    let mut e = editor(Keymap::Emacs, "abc\ndef\n");

    ctrl(&mut e, 'e').await;
    assert_eq!(e.engine.state.cursor, pos(1, 3));

    ctrl(&mut e, 'f').await;
    assert_eq!(
        e.engine.state.cursor,
        pos(2, 0),
        "Emacs C-f moves point onto the next line, not back to the last char"
    );
}

#[tokio::test]
async fn ctrl_f_stops_at_the_end_of_the_buffer() {
    let mut e = editor(Keymap::Emacs, "abc");
    ctrl(&mut e, 'e').await;
    ctrl(&mut e, 'f').await;
    assert_eq!(e.engine.state.cursor, pos(1, 3));
}

#[tokio::test]
async fn ctrl_b_crosses_to_the_previous_line_end() {
    let mut e = editor(Keymap::Emacs, "abc\ndef\n");
    e.engine.state.cursor = pos(2, 0);

    ctrl(&mut e, 'b').await;
    assert_eq!(e.engine.state.cursor, pos(1, 3));
}

#[tokio::test]
async fn ctrl_n_clamps_to_point_not_the_last_character() {
    let mut e = editor(Keymap::Emacs, "abcd\nab\n");
    ctrl(&mut e, 'e').await;
    assert_eq!(e.engine.state.cursor, pos(1, 4));

    ctrl(&mut e, 'n').await;
    assert_eq!(
        e.engine.state.cursor,
        pos(2, 2),
        "a short line accepts point at its content length"
    );
}

#[tokio::test]
async fn meta_f_and_meta_b_use_emacs_word_boundaries() {
    let mut e = editor(Keymap::Emacs, "foo bar\n");

    alt(&mut e, 'f').await;
    assert_eq!(
        e.engine.state.cursor,
        pos(1, 3),
        "M-f stops at the word end"
    );

    alt(&mut e, 'f').await;
    assert_eq!(e.engine.state.cursor, pos(1, 7));

    alt(&mut e, 'b').await;
    assert_eq!(
        e.engine.state.cursor,
        pos(1, 4),
        "M-b stops at the word start"
    );
}

#[tokio::test]
async fn meta_backspace_kills_the_previous_emacs_word() {
    let mut e = editor(Keymap::Emacs, "foo bar\n");
    ctrl(&mut e, 'e').await;

    press(&mut e, KeyCode::Backspace, KeyModifiers::ALT).await;

    assert_eq!(e.engine.buffer.to_string(), "foo \n");
    assert_eq!(e.engine.state.cursor, pos(1, 4));
}

#[tokio::test]
async fn emacs_region_does_not_leak_into_vim() {
    let mut e = editor(Keymap::Emacs, "abc\n");
    press(&mut e, KeyCode::Char(' '), KeyModifiers::CONTROL).await;
    ctrl(&mut e, 'f').await;
    assert!(e.engine.state.region_active);

    e.switch_keymap("vim");

    assert!(
        !e.engine.state.region_active,
        "the Emacs region marker must be deactivated"
    );
    assert_eq!(e.engine.state.keymap, Keymap::Vim);
}

#[tokio::test]
async fn emacs_transient_state_is_cleared_on_keymap_switch() {
    let mut e = editor(Keymap::Emacs, "abc\n");
    ctrl(&mut e, 'u').await;
    assert!(e.engine.emacs_state.numeric_arg.is_some());

    e.switch_keymap("vim");

    assert_eq!(e.engine.emacs_state.numeric_arg, None);
    assert_eq!(e.engine.emacs_state.prefix, None);
    assert!(!e.engine.emacs_state.last_command_was_kill);
}

#[tokio::test]
async fn emacs_prefix_is_cleared_on_keymap_switch() {
    let mut e = editor(Keymap::Emacs, "abc\n");
    ctrl(&mut e, 'x').await;
    assert_eq!(e.engine.emacs_state.prefix, Some('x'));

    e.switch_keymap("vim");
    assert_eq!(e.engine.emacs_state.prefix, None);
}

#[tokio::test]
async fn vim_insert_state_does_not_leak_into_emacs() {
    let mut e = editor(Keymap::Vim, "abc\n");
    press(&mut e, KeyCode::Char('i'), KeyModifiers::NONE).await;
    press(&mut e, KeyCode::Char('x'), KeyModifiers::NONE).await;
    assert_eq!(e.engine.state.mode, Mode::Insert);
    assert_eq!(e.engine.insert_state.accum, "x");

    e.switch_keymap("emacs");

    assert_eq!(e.engine.state.mode, Mode::Normal);
    assert!(e.engine.insert_state.accum.is_empty());
    assert!(e.engine.insert_state.start_pos.is_none());
    assert_eq!(e.engine.state.keymap, Keymap::Emacs);
}

#[tokio::test]
async fn cancelling_the_emacs_minibuffer_clears_transient_state() {
    let mut e = editor(Keymap::Emacs, "abc\n");
    ctrl(&mut e, 's').await;
    assert_eq!(e.engine.state.mode, Mode::Command);

    ctrl(&mut e, 'g').await;

    assert_eq!(e.engine.state.mode, Mode::Normal);
    assert_eq!(e.engine.state.keymap, Keymap::Emacs);
    assert_eq!(e.engine.emacs_state.prefix, None);
    assert_eq!(e.engine.emacs_state.numeric_arg, None);
}

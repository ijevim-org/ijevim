//! Regression tests for Emacs keymap semantics.
//!
//! These cover behaviour that the older Emacs handler either got wrong or did
//! not implement: command-line routing, undo granularity, prefix consumption,
//! kill-ring appending, and multi-line yank point placement.

use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::{Keymap, Mode, Position};

fn ed(text: &str) -> Editor {
    let mut editor = Editor::new_headless_for_test(Keymap::Emacs).expect("headless editor");
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

#[tokio::test]
async fn self_insert_is_one_undo_unit() {
    let mut e = ed("");
    press(&mut e, KeyCode::Char('a'), KeyModifiers::NONE).await;
    press(&mut e, KeyCode::Char('b'), KeyModifiers::NONE).await;
    press(&mut e, KeyCode::Char('c'), KeyModifiers::NONE).await;
    assert_eq!(e.engine.buffer.to_string(), "abc");

    ctrl(&mut e, '/').await;
    assert_eq!(e.engine.buffer.to_string(), "", "C-/ undoes the typing run");

    press(
        &mut e,
        KeyCode::Char('/'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    )
    .await;
    assert_eq!(
        e.engine.buffer.to_string(),
        "abc",
        "C-M-/ redoes the typing run"
    );
}

#[tokio::test]
async fn backspace_is_undoable() {
    let mut e = ed("");
    press(&mut e, KeyCode::Char('a'), KeyModifiers::NONE).await;
    press(&mut e, KeyCode::Char('b'), KeyModifiers::NONE).await;
    press(&mut e, KeyCode::Backspace, KeyModifiers::NONE).await;
    assert_eq!(e.engine.buffer.to_string(), "a");

    ctrl(&mut e, '/').await;
    assert_eq!(
        e.engine.buffer.to_string(),
        "ab",
        "C-/ restores the deleted char"
    );
}

#[tokio::test]
async fn newline_is_undoable_at_column_zero() {
    let mut e = ed("");
    press(&mut e, KeyCode::Enter, KeyModifiers::NONE).await;
    assert_eq!(e.engine.buffer.to_string(), "\n");

    ctrl(&mut e, '/').await;
    assert_eq!(
        e.engine.buffer.to_string(),
        "",
        "undo removes a newline inserted at (1, 0)"
    );
}

#[tokio::test]
async fn ctrl_s_runs_a_forward_search() {
    let mut e = ed("foo bar foo\n");
    ctrl(&mut e, 's').await;
    assert_eq!(e.engine.state.mode, Mode::Command);
    assert_eq!(e.engine.state.command_buffer, "/");

    for c in "bar".chars() {
        press(&mut e, KeyCode::Char(c), KeyModifiers::NONE).await;
    }
    assert_eq!(
        e.engine.buffer.to_string(),
        "foo bar foo\n",
        "typing in the search prompt must not edit the buffer"
    );

    press(&mut e, KeyCode::Enter, KeyModifiers::NONE).await;
    assert_eq!(e.engine.state.mode, Mode::Normal);
    assert_eq!(e.engine.search_state.query, "bar");
    assert_eq!(
        e.engine.state.cursor,
        Position { line: 1, col: 4 },
        "point lands on the match"
    );
}

#[tokio::test]
async fn ctrl_r_runs_a_backward_search() {
    let mut e = ed("foo bar foo\n");
    e.engine.state.cursor = Position { line: 1, col: 8 };
    ctrl(&mut e, 'r').await;
    assert_eq!(e.engine.state.command_buffer, "?");

    for c in "bar".chars() {
        press(&mut e, KeyCode::Char(c), KeyModifiers::NONE).await;
    }
    press(&mut e, KeyCode::Enter, KeyModifiers::NONE).await;

    assert_eq!(e.engine.state.mode, Mode::Normal);
    assert_eq!(e.engine.search_state.query, "bar");
    assert_eq!(e.engine.state.cursor, Position { line: 1, col: 4 });
}

#[tokio::test]
async fn ctrl_g_cancels_the_search_prompt() {
    let mut e = ed("foo bar\n");
    ctrl(&mut e, 's').await;
    press(&mut e, KeyCode::Char('b'), KeyModifiers::NONE).await;
    ctrl(&mut e, 'g').await;

    assert_eq!(e.engine.state.mode, Mode::Normal);
    assert_eq!(e.engine.state.command_buffer, "");
    assert_eq!(e.engine.buffer.to_string(), "foo bar\n");
}

#[tokio::test]
async fn undefined_c_x_sequence_is_consumed() {
    let mut e = ed("abcdef\n");
    e.engine.state.cursor.col = 1;

    ctrl(&mut e, 'x').await;
    ctrl(&mut e, 'q').await;

    assert_eq!(
        e.engine.state.cursor.col, 1,
        "C-x C-q must not fall through to a normal binding"
    );
    assert_eq!(e.engine.buffer.to_string(), "abcdef\n");
    assert!(
        e.engine
            .state
            .last_message
            .as_deref()
            .is_some_and(|m| m.contains("undefined")),
        "the unbound sequence is reported"
    );
}

#[tokio::test]
async fn multiline_kill_yank_restores_point() {
    let mut e = ed("ab\ncd\n");
    e.engine.state.mark = Some(Position { line: 1, col: 0 });
    e.engine.state.region_active = true;
    e.engine.state.cursor = Position { line: 2, col: 2 };

    ctrl(&mut e, 'w').await;
    assert_eq!(e.engine.buffer.to_string(), "\n");

    ctrl(&mut e, 'y').await;
    assert_eq!(e.engine.buffer.to_string(), "ab\ncd\n");
    assert_eq!(
        e.engine.state.cursor,
        Position { line: 2, col: 2 },
        "point ends after the last line of a multi-line yank"
    );
}

#[tokio::test]
async fn consecutive_kills_append_to_one_entry() {
    let mut e = ed("one\ntwo\n");
    ctrl(&mut e, 'k').await;
    assert_eq!(
        e.engine.buffer.to_string(),
        "two\n",
        "C-k at bol kills the whole line"
    );

    ctrl(&mut e, 'k').await;
    assert_eq!(e.engine.buffer.to_string(), "");
    assert_eq!(
        e.kill_ring().yank(),
        Some("one\ntwo\n"),
        "the second C-k appends to the first kill"
    );

    ctrl(&mut e, 'y').await;
    assert_eq!(e.engine.buffer.to_string(), "one\ntwo\n");
}

#[tokio::test]
async fn non_kill_command_breaks_the_kill_chain() {
    let mut e = ed("one two\n");
    ctrl(&mut e, 'k').await; // "one two"
    press(&mut e, KeyCode::Char('x'), KeyModifiers::NONE).await; // self-insert
    ctrl(&mut e, 'a').await;
    ctrl(&mut e, 'k').await; // "x"

    assert_eq!(e.kill_ring().yank(), Some("x"));
}

#[tokio::test]
async fn ctrl_w_without_region_does_not_kill_a_word() {
    let mut e = ed("hello world\n");
    ctrl(&mut e, 'w').await;
    assert_eq!(e.engine.buffer.to_string(), "hello world\n");
    assert!(
        e.engine
            .state
            .last_message
            .as_deref()
            .is_some_and(|m| m.contains("no region")),
        "C-w reports the missing region"
    );
}

#[tokio::test]
async fn ctrl_v_and_meta_v_move_point_by_a_page() {
    let text: String = (1..=60).map(|n| format!("line {}\n", n)).collect();
    let mut e = ed(&text);

    ctrl(&mut e, 'v').await;
    let after_down = e.engine.state.cursor.line;
    assert!(
        after_down > 1,
        "C-v moves point forward, got line {}",
        after_down
    );

    alt(&mut e, 'v').await;
    assert!(
        e.engine.state.cursor.line < after_down,
        "M-v moves point backward"
    );
}

#[tokio::test]
async fn ctrl_o_opens_a_line_without_moving_point() {
    let mut e = ed("abcd\n");
    e.engine.state.cursor.col = 2;

    ctrl(&mut e, 'o').await;
    assert_eq!(e.engine.buffer.to_string(), "ab\ncd\n");
    assert_eq!(
        e.engine.state.cursor,
        Position { line: 1, col: 2 },
        "point stays before the inserted newline"
    );
}

#[tokio::test]
async fn ctrl_x_ctrl_x_exchanges_point_and_mark() {
    let mut e = ed("abcdef\n");
    e.engine.state.mark = Some(Position { line: 1, col: 1 });
    e.engine.state.cursor = Position { line: 1, col: 4 };
    e.engine.state.region_active = true;

    ctrl(&mut e, 'x').await;
    ctrl(&mut e, 'x').await;

    assert_eq!(e.engine.state.cursor, Position { line: 1, col: 1 });
    assert_eq!(e.engine.state.mark, Some(Position { line: 1, col: 4 }));
}

#[tokio::test]
async fn ctrl_d_at_end_of_line_joins_the_next_line() {
    let mut e = ed("ab\ncd\n");
    e.engine.state.cursor = Position { line: 1, col: 2 };

    ctrl(&mut e, 'd').await;
    assert_eq!(e.engine.buffer.to_string(), "abcd\n");
    assert_eq!(e.engine.state.cursor, Position { line: 1, col: 2 });

    ctrl(&mut e, '/').await;
    assert_eq!(
        e.engine.buffer.to_string(),
        "ab\ncd\n",
        "C-d join is undoable"
    );
}

#[tokio::test]
async fn ctrl_k_at_end_of_line_kills_the_newline() {
    let mut e = ed("ab\ncd\n");
    e.engine.state.cursor = Position { line: 1, col: 2 };

    ctrl(&mut e, 'k').await;
    assert_eq!(e.engine.buffer.to_string(), "abcd\n");
    assert_eq!(e.kill_ring().yank(), Some("\n"));
}

#[tokio::test]
async fn transpose_at_end_of_line_swaps_the_last_two_chars() {
    let mut e = ed("abc\n");
    e.engine.state.cursor = Position { line: 1, col: 3 };

    ctrl(&mut e, 't').await;
    assert_eq!(e.engine.buffer.to_string(), "acb\n");
    assert_eq!(
        e.engine.state.cursor,
        Position { line: 1, col: 3 },
        "point stays at end of line"
    );

    ctrl(&mut e, '/').await;
    assert_eq!(e.engine.buffer.to_string(), "abc\n", "C-t is undoable");
}

#[tokio::test]
async fn meta_x_opens_the_command_line() {
    let mut e = ed("x\n");
    alt(&mut e, 'x').await;
    assert_eq!(e.engine.state.mode, Mode::Command);
    assert_eq!(e.engine.state.command_buffer, "");
}

// ── Numeric arguments ──────────────────────────────────────────────

#[tokio::test]
async fn c_u_3_moves_three_chars() {
    let mut e = ed("abcdef\n");
    ctrl(&mut e, 'u').await;
    press(&mut e, KeyCode::Char('3'), KeyModifiers::NONE).await;
    ctrl(&mut e, 'f').await;
    assert_eq!(e.engine.state.cursor, Position { line: 1, col: 3 });
}

#[tokio::test]
async fn meta_digit_is_a_numeric_argument() {
    let mut e = ed("abcdef\n");
    alt(&mut e, '3').await;
    ctrl(&mut e, 'f').await;
    assert_eq!(e.engine.state.cursor, Position { line: 1, col: 3 });
}

#[tokio::test]
async fn c_u_defaults_to_four() {
    let mut e = ed("abcdef\n");
    ctrl(&mut e, 'u').await;
    ctrl(&mut e, 'f').await;
    assert_eq!(e.engine.state.cursor, Position { line: 1, col: 4 });
}

#[tokio::test]
async fn negative_argument_moves_backwards() {
    let mut e = ed("abcdef\n");
    e.engine.state.cursor = Position { line: 1, col: 5 };
    alt(&mut e, '-').await;
    ctrl(&mut e, 'f').await;
    assert_eq!(e.engine.state.cursor, Position { line: 1, col: 4 });
}

#[tokio::test]
async fn c_u_repeats_self_insert() {
    let mut e = ed("");
    ctrl(&mut e, 'u').await;
    press(&mut e, KeyCode::Char('2'), KeyModifiers::NONE).await;
    press(&mut e, KeyCode::Char('x'), KeyModifiers::NONE).await;
    assert_eq!(e.engine.buffer.to_string(), "xx");
}

#[tokio::test]
async fn c_u_repeats_delete() {
    let mut e = ed("abcdef");
    ctrl(&mut e, 'u').await;
    press(&mut e, KeyCode::Char('3'), KeyModifiers::NONE).await;
    ctrl(&mut e, 'd').await;
    assert_eq!(e.engine.buffer.to_string(), "def");
}

#[tokio::test]
async fn c_u_kills_multiple_lines_in_one_entry() {
    let mut e = ed("a\nb\nc\n");
    ctrl(&mut e, 'u').await;
    press(&mut e, KeyCode::Char('2'), KeyModifiers::NONE).await;
    ctrl(&mut e, 'k').await;
    assert_eq!(e.engine.buffer.to_string(), "c\n");
    assert_eq!(e.kill_ring().yank(), Some("a\nb\n"));
}

// ── Files and buffers ──────────────────────────────────────────────

#[tokio::test]
async fn c_x_ctrl_f_opens_a_find_file_prompt() {
    let mut e = ed("x\n");
    ctrl(&mut e, 'x').await;
    ctrl(&mut e, 'f').await;
    assert_eq!(e.engine.state.mode, Mode::Command);
    assert!(
        e.engine.state.command_buffer.starts_with("e "),
        "got {:?}",
        e.engine.state.command_buffer
    );
}

#[tokio::test]
async fn c_x_ctrl_w_opens_a_write_file_prompt() {
    let mut e = ed("x\n");
    ctrl(&mut e, 'x').await;
    ctrl(&mut e, 'w').await;
    assert_eq!(e.engine.state.mode, Mode::Command);
    assert!(e.engine.state.command_buffer.starts_with("w "));
}

#[tokio::test]
async fn c_x_b_opens_a_buffer_prompt() {
    let mut e = ed("x\n");
    ctrl(&mut e, 'x').await;
    press(&mut e, KeyCode::Char('b'), KeyModifiers::NONE).await;
    assert_eq!(e.engine.state.mode, Mode::Command);
    assert_eq!(e.engine.state.command_buffer, "b ");
}

#[tokio::test]
async fn c_x_d_opens_a_dired_prompt() {
    let mut e = ed("x\n");
    ctrl(&mut e, 'x').await;
    press(&mut e, KeyCode::Char('d'), KeyModifiers::NONE).await;
    assert_eq!(e.engine.state.mode, Mode::Command);
    assert!(e.engine.state.command_buffer.starts_with("e "));
}

#[tokio::test]
async fn c_x_k_refuses_to_kill_a_modified_buffer() {
    let mut e = ed("x\n");
    press(&mut e, KeyCode::Char('z'), KeyModifiers::NONE).await;
    let before = e.engine.buffer.to_string();

    ctrl(&mut e, 'x').await;
    press(&mut e, KeyCode::Char('k'), KeyModifiers::NONE).await;

    assert_eq!(e.engine.buffer.to_string(), before);
    assert!(
        e.engine
            .state
            .last_message
            .as_deref()
            .unwrap_or("")
            .contains("modified"),
        "the refusal must be reported"
    );
}

#[tokio::test]
async fn c_x_b_switches_by_file_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    std::fs::write(&first, "first\n").expect("write first");
    std::fs::write(&second, "second\n").expect("write second");

    let mut e = ed("scratch\n");
    e.engine
        .buffer_open(first.to_str().unwrap())
        .expect("open first");
    e.engine
        .buffer_open(second.to_str().unwrap())
        .expect("open second");
    assert_eq!(e.engine.buffer.to_string(), "second\n");

    ctrl(&mut e, 'x').await;
    press(&mut e, KeyCode::Char('b'), KeyModifiers::NONE).await;
    for c in "first.txt".chars() {
        press(&mut e, KeyCode::Char(c), KeyModifiers::NONE).await;
    }
    press(&mut e, KeyCode::Enter, KeyModifiers::NONE).await;

    assert_eq!(e.engine.buffer.to_string(), "first\n");
    assert_eq!(e.engine.state.file_path.as_deref(), Some(first.as_path()));
}

// ── Windows ────────────────────────────────────────────────────────

#[tokio::test]
async fn c_x_window_commands_split_focus_and_close() {
    let mut e = ed("x\n");
    e.engine.update_window_layout(24, 80);

    ctrl(&mut e, 'x').await;
    press(&mut e, KeyCode::Char('2'), KeyModifiers::NONE).await;
    assert_eq!(e.engine.window_layout.windows.len(), 2, "C-x 2 splits");

    ctrl(&mut e, 'x').await;
    press(&mut e, KeyCode::Char('3'), KeyModifiers::NONE).await;
    assert_eq!(e.engine.window_layout.windows.len(), 3, "C-x 3 splits");

    let before = e.engine.window_layout.focused;
    ctrl(&mut e, 'x').await;
    press(&mut e, KeyCode::Char('o'), KeyModifiers::NONE).await;
    assert_ne!(
        e.engine.window_layout.focused, before,
        "C-x o moves focus to another window"
    );

    ctrl(&mut e, 'x').await;
    press(&mut e, KeyCode::Char('0'), KeyModifiers::NONE).await;
    assert_eq!(
        e.engine.window_layout.windows.len(),
        2,
        "C-x 0 closes the focused window"
    );

    ctrl(&mut e, 'x').await;
    press(&mut e, KeyCode::Char('1'), KeyModifiers::NONE).await;
    assert_eq!(
        e.engine.window_layout.windows.len(),
        1,
        "C-x 1 closes every other window"
    );
}

#[tokio::test]
async fn zero_argument_does_not_move() {
    let mut e = ed("abcdef\n");
    ctrl(&mut e, 'u').await;
    press(&mut e, KeyCode::Char('0'), KeyModifiers::NONE).await;
    ctrl(&mut e, 'f').await;
    assert_eq!(e.engine.state.cursor, Position { line: 1, col: 0 });
}

#[tokio::test]
async fn c_x_ctrl_w_saves_to_the_typed_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("written.txt");

    let mut e = ed("hello\n");
    ctrl(&mut e, 'x').await;
    ctrl(&mut e, 'w').await;
    for c in path.to_string_lossy().chars() {
        press(&mut e, KeyCode::Char(c), KeyModifiers::NONE).await;
    }
    press(&mut e, KeyCode::Enter, KeyModifiers::NONE).await;

    assert_eq!(
        std::fs::read_to_string(&path).expect("written file"),
        "hello\n"
    );
    assert_eq!(e.engine.state.file_path.as_deref(), Some(path.as_path()));
}

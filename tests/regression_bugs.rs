//! Regression tests for bugs found by auditing against real Vim.

use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::{Keymap, Mode};

fn editor(text: &str) -> Editor {
    let mut e = Editor::new_headless_for_test(Keymap::Vim).expect("headless editor");
    e.set_buffer_for_test(text);
    e
}

async fn press(e: &mut Editor, key: KeyCode, modifiers: KeyModifiers) {
    e.handle_key_for_test(key, modifiers).await;
}

async fn keys(e: &mut Editor, s: &str) {
    for ch in s.chars() {
        match ch {
            '\x1b' => press(e, KeyCode::Esc, KeyModifiers::NONE).await,
            '\n' => press(e, KeyCode::Enter, KeyModifiers::NONE).await,
            '\x08' => press(e, KeyCode::Backspace, KeyModifiers::NONE).await,
            '\t' => press(e, KeyCode::Tab, KeyModifiers::NONE).await,
            _ => press(e, KeyCode::Char(ch), KeyModifiers::NONE).await,
        }
    }
}

fn snapshot(e: &Editor) -> (String, usize, usize, Mode) {
    e.snapshot_for_test()
}

// ── Linewise yanks ─────────────────────────────────────────────

#[tokio::test]
async fn yank_two_lines_pastes_without_an_extra_blank_line() {
    let mut e = editor("a\nb\nc\n");
    keys(&mut e, "yjp").await;
    assert_eq!(snapshot(&e).0, "a\na\nb\nb\nc\n");
}

#[tokio::test]
async fn yank_last_line_without_trailing_newline_stays_linewise() {
    let mut e = editor("a\nb");
    // `yy` on the last line must paste as a whole line, not inline.
    keys(&mut e, "jyyP").await;
    assert_eq!(snapshot(&e).0, "a\nb\nb");
}

// ── Undo / redo ────────────────────────────────────────────────

#[tokio::test]
async fn redo_join_merges_the_following_line() {
    let mut e = editor("a\nb\nc\n");
    keys(&mut e, "J").await;
    assert_eq!(snapshot(&e).0, "a b\nc\n");
    keys(&mut e, "u").await;
    assert_eq!(snapshot(&e).0, "a\nb\nc\n");
    press(&mut e, KeyCode::Char('r'), KeyModifiers::CONTROL).await;
    assert_eq!(snapshot(&e).0, "a b\nc\n");
}

#[tokio::test]
async fn redo_join_removes_indentation_like_the_original() {
    let mut e = editor("x\n  y\nz\n");
    keys(&mut e, "J").await;
    assert_eq!(snapshot(&e).0, "x y\nz\n");
    keys(&mut e, "u").await;
    press(&mut e, KeyCode::Char('r'), KeyModifiers::CONTROL).await;
    assert_eq!(snapshot(&e).0, "x y\nz\n");
}

#[tokio::test]
async fn undo_counted_replace_starting_at_the_cursor() {
    let mut e = editor("abcdef\n");
    keys(&mut e, "3rX").await;
    assert_eq!(snapshot(&e).0, "XXXdef\n");
    keys(&mut e, "u").await;
    assert_eq!(snapshot(&e).0, "abcdef\n");
}

#[tokio::test]
async fn g_minus_undoes_the_current_edit() {
    let mut e = editor("abc\n");
    keys(&mut e, "xx").await;
    assert_eq!(snapshot(&e).0, "c\n");
    keys(&mut e, "g-").await;
    assert_eq!(snapshot(&e).0, "bc\n");
}

// ── Insert-mode deletions ──────────────────────────────────────

#[tokio::test]
async fn insert_backspace_over_existing_text_is_undoable() {
    let mut e = editor("abc\n");
    keys(&mut e, "li\x08x\x1b").await;
    assert_eq!(snapshot(&e).0, "xbc\n");
    keys(&mut e, "u").await;
    assert_eq!(snapshot(&e).0, "abc\n");
}

#[tokio::test]
async fn insert_backspace_joining_lines_is_undoable() {
    let mut e = editor("ab\ncd\n");
    keys(&mut e, "ji\x08X\x1b").await;
    assert_eq!(snapshot(&e).0, "abXcd\n");
    keys(&mut e, "u").await;
    assert_eq!(snapshot(&e).0, "ab\ncd\n");
}

#[tokio::test]
async fn insert_ctrl_w_over_existing_text_is_undoable() {
    let mut e = editor("hello world\n");
    press(&mut e, KeyCode::Char('A'), KeyModifiers::NONE).await;
    press(&mut e, KeyCode::Char('w'), KeyModifiers::CONTROL).await;
    keys(&mut e, "X\x1b").await;
    assert_eq!(snapshot(&e).0, "hello X\n");
    keys(&mut e, "u").await;
    assert_eq!(snapshot(&e).0, "hello world\n");
}

#[tokio::test]
async fn insert_ctrl_u_over_existing_text_is_undoable() {
    let mut e = editor("hello world\n");
    // Insert at column 5, delete back to BOL, then type.
    keys(&mut e, "05li").await;
    press(&mut e, KeyCode::Char('u'), KeyModifiers::CONTROL).await;
    keys(&mut e, "X\x1b").await;
    assert_eq!(snapshot(&e).0, "X world\n");
    keys(&mut e, "u").await;
    assert_eq!(snapshot(&e).0, "hello world\n");
}

#[tokio::test]
async fn undo_after_append_clamps_the_normal_cursor() {
    let mut e = editor("abc def\n");
    press(&mut e, KeyCode::Char('A'), KeyModifiers::NONE).await;
    press(&mut e, KeyCode::Char('w'), KeyModifiers::CONTROL).await;
    keys(&mut e, "X\x1b").await;
    keys(&mut e, "u").await;
    let (buf, line, col, mode) = snapshot(&e);
    assert_eq!(buf, "abc def\n");
    assert_eq!((line, col), (1, 6));
    assert_eq!(mode, Mode::Normal);
}

// ── Replace mode ───────────────────────────────────────────────

#[tokio::test]
async fn replace_mode_appending_past_eol_undoes_to_empty() {
    let mut e = editor("\n");
    keys(&mut e, "Rab\x1b").await;
    assert_eq!(snapshot(&e).0, "ab\n");
    keys(&mut e, "u").await;
    assert_eq!(snapshot(&e).0, "\n");
}

#[tokio::test]
async fn replace_mode_undo_restores_the_session_start_cursor() {
    let mut e = editor("abc def\n");
    keys(&mut e, "RXY\x1b").await;
    keys(&mut e, "u").await;
    assert_eq!(snapshot(&e), ("abc def\n".to_string(), 1, 0, Mode::Normal));
}

// ── Dot repeat of Insert sessions ──────────────────────────────

#[tokio::test]
async fn dot_repeats_inline_insert() {
    let mut e = editor("abc\n");
    keys(&mut e, "iX\x1b.").await;
    assert_eq!(snapshot(&e).0, "XXabc\n");
}

#[tokio::test]
async fn dot_repeats_append_at_eol() {
    let mut e = editor("abc\n");
    keys(&mut e, "A!\x1b.").await;
    assert_eq!(snapshot(&e).0, "abc!!\n");
}

#[tokio::test]
async fn dot_repeats_open_line_below_and_above() {
    let mut e = editor("a\nb\n");
    keys(&mut e, "ox\x1b.").await;
    assert_eq!(snapshot(&e).0, "a\nx\nx\nb\n");

    let mut e = editor("a\nb\n");
    keys(&mut e, "Ox\x1b.").await;
    assert_eq!(snapshot(&e).0, "x\nx\na\nb\n");
}

// ── Counts ─────────────────────────────────────────────────────

#[tokio::test]
async fn operator_and_motion_counts_multiply() {
    // `2d2w` deletes four words, not `22w` worth of them.
    let mut e = editor("a b c d e f\n");
    keys(&mut e, "2d2w").await;
    assert_eq!(snapshot(&e).0, "e f\n");
}

#[tokio::test]
async fn counted_insert_repeats_the_typed_text() {
    let mut e = editor("abc\n");
    keys(&mut e, "3iX\x1b").await;
    assert_eq!(snapshot(&e).0, "XXXabc\n");
    let mut e = editor("abc\n");
    keys(&mut e, "3aX\x1b").await;
    assert_eq!(snapshot(&e).0, "aXXXbc\n");
}

#[tokio::test]
async fn counted_open_line_opens_every_line() {
    let mut e = editor("abc\n");
    keys(&mut e, "3oX\x1b").await;
    assert_eq!(snapshot(&e).0, "abc\nX\nX\nX\n");
    let mut e = editor("abc\n");
    keys(&mut e, "3OX\x1b").await;
    assert_eq!(snapshot(&e).0, "X\nX\nX\nabc\n");
}

#[tokio::test]
async fn counted_insert_is_a_single_undo_step() {
    let mut e = editor("abc\n");
    keys(&mut e, "3iX\x1bu").await;
    assert_eq!(snapshot(&e).0, "abc\n");
}

// ── Word motions used as operator targets ──────────────────────

#[tokio::test]
async fn word_delete_past_the_last_word_reaches_the_line_end() {
    let mut e = editor("a b c\n");
    keys(&mut e, "d4w").await;
    assert_eq!(snapshot(&e).0, "\n");
}

#[tokio::test]
async fn word_delete_across_two_line_breaks_keeps_the_final_newline() {
    let mut e = editor("a b\nc d\ne f");
    keys(&mut e, "d4w").await;
    assert_eq!(snapshot(&e).0, "e f");
}

#[tokio::test]
async fn word_delete_to_the_next_line_start_keeps_the_break() {
    let mut e = editor("a b\nc d");
    keys(&mut e, "d2w").await;
    assert_eq!(snapshot(&e).0, "\nc d");
}

// ── Multi-line spans ──────────────────────────────────────────

#[tokio::test]
async fn deleting_a_span_over_three_lines_removes_each_break_once() {
    let mut e = editor("a\nb\nc\nd\n");
    keys(&mut e, "vjjd").await;
    assert_eq!(snapshot(&e).0, "\nd\n");
}

#[tokio::test]
async fn yanking_a_span_over_three_lines_does_not_double_breaks() {
    let mut e = editor("a\nb\nc\nd\n");
    keys(&mut e, "vjjyGp").await;
    assert_eq!(snapshot(&e).0, "a\nb\nc\nda\nb\nc\n");
}

#[tokio::test]
async fn outer_quote_object_eats_one_side_of_whitespace() {
    let mut e = editor("'a b' c\n");
    keys(&mut e, "da'").await;
    assert_eq!(snapshot(&e).0, "c\n");

    let mut e = editor("x 'a b'\n");
    keys(&mut e, "da'").await;
    assert_eq!(snapshot(&e).0, "x\n");
}

#[tokio::test]
async fn replace_mode_leaves_the_cursor_on_the_last_replaced_char() {
    let mut e = editor("abc\n");
    keys(&mut e, "Rux\x1b").await;
    assert_eq!(snapshot(&e), ("uxc\n".to_string(), 1, 1, Mode::Normal));
}

#[tokio::test]
async fn counted_line_end_commands_fail_on_a_single_line() {
    for (seq, expected) in [
        ("2D", "a\n"),
        ("3D", "a\n"),
        ("2C\x1b", "a\n"),
        ("2S\x1b", "a\n"),
    ] {
        let mut e = editor("a\n");
        keys(&mut e, seq).await;
        assert_eq!(snapshot(&e).0, expected, "keys={seq}");
        assert_eq!(snapshot(&e).3, Mode::Normal, "keys={seq}");
    }
}

#[tokio::test]
async fn counted_line_end_commands_still_work_across_lines() {
    let mut e = editor("a\nb\nc\n");
    keys(&mut e, "2D").await;
    assert_eq!(snapshot(&e).0, "c\n");
}

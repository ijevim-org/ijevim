use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::{Keymap, Mode, PendingOperator, VisualType};

fn test_vim_editor(text: &str) -> Editor {
    let mut editor = Editor::new_headless_for_test(Keymap::Vim).expect("headless editor");
    editor.set_buffer_for_test(text);
    editor
}

#[tokio::test]
async fn vim_input_flow_and_boundaries() {
    let mut editor = Editor::new_headless_for_test(Keymap::Vim).expect("headless editor");
    editor.set_buffer_for_test("abc\n");

    editor
        .handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char(':'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Enter, KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char(':'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Enter, KeyModifiers::NONE)
        .await;

    let (buf, line, col, mode) = editor.snapshot_for_test();
    assert_eq!(buf, "abc\n");
    assert_eq!((line, col), (1, 0));
    assert_eq!(mode, Mode::Normal);

    editor.set_buffer_for_test("\n");
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, line, col, mode) = editor.snapshot_for_test();
    assert_eq!(buf, "\n");
    assert_eq!((line, col), (1, 0));
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn emacs_input_flow_and_boundaries() {
    let mut editor = Editor::new_headless_for_test(Keymap::Emacs).expect("headless editor");
    editor.set_buffer_for_test("abc\nxyz");

    editor
        .handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('b'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('n'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('p'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('s'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('k'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('/'), KeyModifiers::CONTROL)
        .await;

    let (buf, line, col, mode) = editor.snapshot_for_test();
    assert_eq!(buf, "abc\nxyz");
    assert_eq!((line, col), (1, 1));
    assert_eq!(mode, Mode::Normal);

    editor.set_buffer_for_test("x");
    editor
        .handle_key_for_test(KeyCode::Char('b'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await;
    let (buf, line, col, mode) = editor.snapshot_for_test();
    assert_eq!(buf, "");
    assert_eq!((line, col), (1, 0));
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn vim_redo_restores_after_undo() {
    let mut ed = test_vim_editor("hello\nworld\n");
    // dd to delete line 1
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "world\n", "after dd");

    // 'u' undo
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello\nworld\n", "after undo");

    // Ctrl-R redo
    ed.handle_key_for_test(KeyCode::Char('r'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "world\n", "after redo");
}

#[tokio::test]
async fn vim_page_down_moves_cursor_down() {
    let mut ed = test_vim_editor("line1\nline2\nline3\nline4\nline5\n");
    assert_eq!(ed.snapshot_for_test().1, 1, "starts at line 1");

    // Ctrl-F (page down) should move cursor down
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert!(line > 1, "page down moved cursor to line {}", line);
}

#[tokio::test]
async fn vim_page_up_after_page_down_returns() {
    let mut ed = test_vim_editor("line1\nline2\nline3\nline4\nline5");
    // Go to bottom
    ed.handle_key_for_test(KeyCode::Char('G'), KeyModifiers::NONE)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 5, "G goes to last line (5)");

    // Ctrl-B (page up) should move cursor up
    ed.handle_key_for_test(KeyCode::Char('b'), KeyModifiers::CONTROL)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert!(line < 5, "page up moved cursor to line {}", line);
}

#[tokio::test]
async fn vim_delete_inner_word() {
    let mut ed = test_vim_editor("hello world\n");
    // cursor is at (1,0) on 'h'
    // diw  → delete inner word
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, " world\n", "diw deleted 'hello'");
    assert_eq!((line, col), (1, 0), "cursor at start");
}

#[tokio::test]
async fn vim_undo_delete_inner_word() {
    let mut ed = test_vim_editor("hello world\n");
    // diw
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, " world\n", "diw before undo");

    // u → undo
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "undo restored original");
}

#[tokio::test]
async fn vim_visual_yank() {
    let mut ed = test_vim_editor("abcdef\n");
    // vllly   → visual select "abc" then yank
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::NONE)
        .await;
    // move right twice to select "abc"
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    // yank
    ed.handle_key_for_test(KeyCode::Char('y'), KeyModifiers::NONE)
        .await;
    // buffer unchanged
    let (buf, _line, _col, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "abcdef\n", "buffer unchanged after yank");
    assert_eq!(mode, Mode::Normal, "back to normal after yank");
}

#[tokio::test]
async fn vim_visual_delete() {
    let mut ed = test_vim_editor("abcdef\n");
    // vllld   → visual select "abc" then delete
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, _line, _col, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "def\n", "visual delete removed 'abc'");
    assert_eq!(mode, Mode::Normal, "back to normal after delete");
}

#[tokio::test]
async fn vim_insert_undo_removes_inserted_text() {
    let mut ed = test_vim_editor("\n");
    // i → insert "xyz" → Esc → u
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    for ch in "xyz".chars() {
        ed.handle_key_for_test(KeyCode::Char(ch), KeyModifiers::NONE)
            .await;
    }
    ed.handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "xyz\n", "text inserted");

    // 'u' should remove "xyz"
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\n", "undo removed 'xyz'");
}

#[tokio::test]
async fn vim_insert_enter_undo() {
    let mut ed = test_vim_editor("ab\n");
    // Move to 'b', insert newline, Esc, undo
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Enter, KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "a\nb\n", "newline inserted after 'a'");

    // 'u' should remove the newline
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ab\n", "undo removed newline");
}

// ── Vim word movement tests ─────────────────────────────────

#[tokio::test]
async fn vim_w_word_forward() {
    let mut ed = test_vim_editor("hello world foo\n");
    assert_eq!(ed.snapshot_for_test().2, 0);
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 5, "w moved to space after 'hello'");
}

#[tokio::test]
async fn vim_b_word_backward() {
    let mut ed = test_vim_editor("hello world\n");
    ed.engine.state.cursor.col = 8;
    ed.handle_key_for_test(KeyCode::Char('b'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 6, "b moved to start of 'world'");
}

#[tokio::test]
async fn vim_e_word_end() {
    let mut ed = test_vim_editor("hello world\n");
    ed.handle_key_for_test(KeyCode::Char('e'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 4, "e moved to end of 'hello' (index 4 = 'o')");
}

#[tokio::test]
async fn vim_dollar_moves_to_line_end() {
    let mut ed = test_vim_editor("hello\n");
    ed.handle_key_for_test(KeyCode::Char('$'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 4, "$ moved to last char 'o' (0-indexed 4)");
}

#[tokio::test]
async fn vim_zero_moves_to_column_zero() {
    let mut ed = test_vim_editor("hello\n");
    ed.engine.state.cursor.col = 3;
    ed.handle_key_for_test(KeyCode::Char('0'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.snapshot_for_test().2, 0);
}

// ── Vim visual line mode tests ──────────────────────────────

#[tokio::test]
async fn vim_visual_line_yank() {
    let mut ed = test_vim_editor("abc\ndef\n");
    // V → visual line
    ed.handle_key_for_test(KeyCode::Char('V'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.engine.state.mode, Mode::Visual);
    // y → yank
    ed.handle_key_for_test(KeyCode::Char('y'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "abc\ndef\n", "buffer unchanged after V y");
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn vim_visual_line_delete() {
    let mut ed = test_vim_editor("abc\ndef\nghi\n");
    // V j d → visual line, extend, delete
    ed.handle_key_for_test(KeyCode::Char('V'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('j'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "ghi\n", "V j d deleted first two lines");
    assert_eq!(mode, Mode::Normal);
}

// ── Vim C, S, s tests ──────────────────────────────────────

#[tokio::test]
async fn vim_capital_c_deletes_to_end_and_inserts() {
    let mut ed = test_vim_editor("hello world\n");
    ed.engine.state.cursor.col = 5;
    ed.handle_key_for_test(KeyCode::Char('C'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.engine.state.mode, Mode::Insert);
    let line = ed.engine.buffer.get_line(1);
    assert_eq!(line, "hello\n", "C deleted from col 5 to end");
}

#[tokio::test]
async fn vim_capital_s_substitutes_line() {
    let mut ed = test_vim_editor("hello\n");
    ed.handle_key_for_test(KeyCode::Char('S'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.engine.state.mode, Mode::Insert);
    let line = ed.engine.buffer.get_line(1);
    assert_eq!(line, "\n", "S cleared the line");
}

#[tokio::test]
async fn vim_s_substitutes_char() {
    let mut ed = test_vim_editor("hello\n");
    ed.handle_key_for_test(KeyCode::Char('s'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.engine.state.mode, Mode::Insert);
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ello\n", "s deleted first char");
}

// ── Vim C, D tests ─────────────────────────────────────────

#[tokio::test]
async fn vim_capital_d_deletes_to_end() {
    let mut ed = test_vim_editor("hello world\n");
    ed.engine.state.cursor.col = 5;
    ed.handle_key_for_test(KeyCode::Char('D'), KeyModifiers::NONE)
        .await;
    let line = ed.engine.buffer.get_line(1);
    assert_eq!(line, "hello\n", "D deleted ' world'");
}

// ── Vim find char tests (f/F/t/T/;/,) ──────────────────────

#[tokio::test]
async fn vim_find_char_f() {
    let mut ed = test_vim_editor("hello world\n");
    // fw: find 'w'
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 6, "f w moves to 'w' at col 6");
}

#[tokio::test]
async fn vim_find_char_till_t() {
    let mut ed = test_vim_editor("hello world\n");
    // tw: till 'w'
    ed.handle_key_for_test(KeyCode::Char('t'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 5, "t w stops before 'w' at col 5");
}

#[tokio::test]
async fn vim_find_repeat_semicolon() {
    let mut ed = test_vim_editor("he.llo wo.rld\n");
    // f. → find first '.'
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('.'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 2, "f. moved to first '.' at col 2");
    // ; → repeat find next '.'
    ed.handle_key_for_test(KeyCode::Char(';'), KeyModifiers::NONE)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 9, "; repeated to second '.' at col 9");
}

// ── Vim J join lines ───────────────────────────────────────

#[tokio::test]
async fn vim_join_lines_j() {
    let mut ed = test_vim_editor("hello\nworld\n");
    ed.handle_key_for_test(KeyCode::Char('J'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "J joined lines with space");
}

// ── Vim percent bracket matching ───────────────────────────

#[tokio::test]
async fn vim_percent_matching_bracket() {
    let mut ed = test_vim_editor("(hello)\n");
    // cursor at col 0 '('
    ed.handle_key_for_test(KeyCode::Char('%'), KeyModifiers::NONE)
        .await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!(line, 1);
    assert_eq!(col, 6, "% moved to ')' at col 6");
}

// ── Vim gg / G ────────────────────────────────────────────

#[tokio::test]
async fn vim_gg_goes_to_first_line() {
    let mut ed = test_vim_editor("a\nb\nc\n");
    ed.engine.state.cursor.line = 3;
    // gg
    ed.handle_key_for_test(KeyCode::Char('g'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('g'), KeyModifiers::NONE)
        .await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!(line, 1, "gg went to line 1");
    assert_eq!(col, 0, "gg went to col 0");
}

#[tokio::test]
async fn vim_g_with_count_goes_to_line() {
    let mut ed = test_vim_editor("a\nb\nc\nd\n");
    // 2G → go to line 2
    ed.handle_key_for_test(KeyCode::Char('2'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('G'), KeyModifiers::NONE)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 2, "2G went to line 2");
}

// ── Vim ZZ save and quit ───────────────────────────────────

#[tokio::test]
async fn vim_zz_save_and_quit_clean_file() {
    let mut ed = test_vim_editor("hello\n");
    ed.engine.state.file_path = Some(std::path::PathBuf::from("/tmp/zz_test.txt"));
    // Make buffer not dirty
    ed.engine.buffer.reset_clean_state();
    // ZZ
    ed.handle_key_for_test(KeyCode::Char('Z'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('Z'), KeyModifiers::NONE)
        .await;
    assert!(!ed.running, "ZZ quit after clean save");
}

// ── Vim count prefix tests ─────────────────────────────────

#[tokio::test]
async fn vim_count_prefix_with_j() {
    let mut ed = test_vim_editor("a\nb\nc\nd\ne\n");
    // 3j → down 3 lines
    ed.handle_key_for_test(KeyCode::Char('3'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('j'), KeyModifiers::NONE)
        .await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 4, "3j went to line 4");
}

#[tokio::test]
async fn vim_count_prefix_with_dd() {
    let mut ed = test_vim_editor("a\nb\nc\n");
    // 2dd → delete 2 lines
    ed.handle_key_for_test(KeyCode::Char('2'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "c\n", "2dd deleted first 2 lines");
}

// ── Vim indent tests ───────────────────────────────────────

#[tokio::test]
async fn vim_indent_right_shift() {
    let mut ed = test_vim_editor("hello\n");
    // >>
    ed.handle_key_for_test(KeyCode::Char('>'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('>'), KeyModifiers::NONE)
        .await;
    let line = ed.engine.buffer.get_line(1);
    assert!(line.starts_with('\t'), ">> indented with tab");
}

// ── Vim visual change ──────────────────────────────────────

#[tokio::test]
async fn vim_visual_change_enters_insert() {
    let mut ed = test_vim_editor("hello world\n");
    // vlllc  → visual select "hel" then change
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('c'), KeyModifiers::NONE)
        .await;
    let (buf, _, col, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "lo world\n", "visual change deleted 'hel'");
    assert_eq!(col, 0, "cursor at start");
    assert_eq!(mode, Mode::Insert, "entered insert mode");
}

// ── Vim question mark search ───────────────────────────────

#[tokio::test]
async fn vim_question_mark_enters_backward_search() {
    let mut ed = test_vim_editor("hello world\n");
    ed.handle_key_for_test(KeyCode::Char('?'), KeyModifiers::NONE)
        .await;
    assert_eq!(ed.engine.state.mode, Mode::Command);
    assert_eq!(ed.engine.state.command_buffer, "?");
}

// ── Emacs M-f / M-b word movement ──────────────────────────

#[tokio::test]
async fn emacs_meta_f_moves_word_forward() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world foo\n");
    // M-f
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::ALT)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 5, "M-f moved to space after 'hello'");
}

#[tokio::test]
async fn emacs_meta_b_moves_word_backward() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    ed.engine.state.cursor.col = 8;
    // M-b
    ed.handle_key_for_test(KeyCode::Char('b'), KeyModifiers::ALT)
        .await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 6, "M-b moved to start of 'world'");
}

// ── Emacs C-t transpose ────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_t_transpose_chars() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello\n");
    ed.engine.state.cursor.col = 2; // at 'e' and 'l' (col 2 → between 'e' and 'l')
                                    // Actually transpose swaps chars before and at cursor
                                    // Cursor at col 1: swaps 'h'(0) and 'e'(1)
    ed.engine.state.cursor.col = 1;
    ed.handle_key_for_test(KeyCode::Char('t'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ehllo\n", "C-t transposed 'he' to 'eh'");
    assert_eq!(col, 2, "cursor advanced after transpose");
}

// ── Emacs C-g abort ────────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_g_aborts_command_mode() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.engine.state.mode = Mode::Command;
    ed.engine.state.command_buffer = "/search".to_string();
    ed.handle_key_for_test(KeyCode::Char('g'), KeyModifiers::CONTROL)
        .await;
    assert_eq!(ed.engine.state.mode, Mode::Normal);
    assert!(
        ed.engine.state.command_buffer.is_empty(),
        "C-g cleared command buffer"
    );
}

// ── Emacs C-x u redo ───────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_x_u_redo() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    // Make a change, undo it, then C-x u redo
    ed.engine.state.cursor.col = 5;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "helloworld\n", "C-d deleted space");

    // Undo
    ed.handle_key_for_test(KeyCode::Char('/'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "C-/ undid deletion");

    // C-x u redo
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "helloworld\n", "C-x u redid deletion");
}

// ── Emacs C-x C-s save file ────────────────────────────────
#[tokio::test]
async fn emacs_ctrl_x_ctrl_s_saves_file() {
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut temp_file = NamedTempFile::new().expect("temp file");
    let path = temp_file.path().to_path_buf();

    // Write initial content
    temp_file.write_all(b"original\n").expect("write initial");
    temp_file.flush().expect("flush");

    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("modified\n");

    // Set buffer's file path using public API
    ed.engine.buffer.set_file_path(Some(path.clone()));

    // C-x C-s to save
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('s'), KeyModifiers::CONTROL)
        .await;

    let saved = fs::read_to_string(&path).expect("read saved");
    assert_eq!(saved, "modified\n", "C-x C-s saved the file");
}

// ── Emacs C-x C-c quit ─────────────────────────────────────
#[tokio::test]
async fn emacs_ctrl_x_ctrl_c_quits() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.running = true; // headless test editor needs running = true
    ed.set_buffer_for_test("hello\n");
    // Make buffer dirty by deleting a character (C-d in Emacs mode)
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await; // delete 'h'
    assert!(
        ed.engine.buffer.is_dirty(),
        "buffer should be dirty after C-d"
    );

    // C-x C-c should trigger quit confirmation
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
        .await;

    assert!(
        ed.engine.state.has_confirmation(),
        "C-x C-c triggers quit confirmation"
    );
    assert!(ed.running, "editor still running after confirmation prompt");

    // Accept quit with 'y'
    ed.handle_key_for_test(KeyCode::Char('y'), KeyModifiers::NONE)
        .await;
    assert!(!ed.running, "editor quit after 'y'");
}

// ── Emacs C-x h / C-x d line start/end ──────────────────────

#[tokio::test]
async fn emacs_ctrl_x_h_moves_to_line_start() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    ed.engine.state.cursor.col = 5; // Middle of "hello"

    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('h'), KeyModifiers::CONTROL)
        .await;

    assert_eq!(ed.engine.state.cursor.col, 0, "C-x h moved to line start");
}

#[tokio::test]
async fn emacs_ctrl_x_d_moves_to_line_end() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    ed.engine.state.cursor.col = 2; // Near start

    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await;

    assert_eq!(
        ed.engine.state.cursor.col, 11,
        "C-x d moved to line end (before newline)"
    );
}

// ── Emacs C-x C-/ redo ──────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_x_ctrl_slash_redo() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    ed.engine.state.cursor.col = 5;

    // C-d delete space
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "helloworld\n", "C-d deleted space");

    // Undo with C-/
    ed.handle_key_for_test(KeyCode::Char('/'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "C-/ undid deletion");

    // C-x C-/ redo
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('/'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "helloworld\n", "C-x C-/ redid deletion");
}

// ── Emacs C-y yank from kill ring ──────────────────────────

#[tokio::test]
async fn emacs_ctrl_y_yanks_from_kill_ring() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello\n");
    // Kill "hello"
    ed.handle_key_for_test(KeyCode::Char('k'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\n", "C-k killed line content");

    // Yank back at start
    ed.engine.state.cursor.col = 0;
    ed.handle_key_for_test(KeyCode::Char('y'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello\n", "C-y yanked 'hello' back");
}

// ── Emacs region kill (C-w) ────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_w_kills_region() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    // Set mark at 0, move cursor to 5, C-w kills region
    ed.handle_key_for_test(KeyCode::Char(' '), KeyModifiers::CONTROL)
        .await;
    for _ in 0..5 {
        ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
            .await;
    }
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, " world\n", "C-w killed 'hello'");
}

// ── Emacs M-w copy region ──────────────────────────────────

#[tokio::test]
async fn emacs_meta_w_copies_region() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    // Set mark at 0, move cursor to 5, M-w copies region
    ed.handle_key_for_test(KeyCode::Char(' '), KeyModifiers::CONTROL)
        .await;
    for _ in 0..5 {
        ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
            .await;
    }
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::ALT)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "M-w did not change buffer");
    // kill-ring should have "hello" (checked via yank after)
    ed.engine.state.cursor.col = 0;
    ed.handle_key_for_test(KeyCode::Char('y'), KeyModifiers::CONTROL)
        .await;
    let (buf2, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf2, "hellohello world\n", "C-y yanked copied 'hello' back");
}

// ── Keymap switching tests ─────────────────────────────────

#[tokio::test]
async fn switch_from_vim_to_emacs_and_back() {
    let mut ed = test_vim_editor("hello world\n");
    assert!(matches!(ed.engine.state.mode, Mode::Normal));

    // Switch to Emacs
    ed.switch_keymap("emacs");
    // In Emacs mode, M-f should move word forward (not insert 'f')
    let col_before = ed.engine.state.cursor.col;
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::ALT)
        .await;
    let col_after = ed.engine.state.cursor.col;
    assert!(
        col_after > col_before,
        "M-f moved cursor forward after emacs switch"
    );
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "buffer unchanged after M-f");

    // Switch back to Vim
    ed.switch_keymap("vim");
    ed.engine.state.cursor.col = 0;
    // Vim: 'x' should delete a character
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ello world\n", "x deleted first char after vim switch");
}

#[tokio::test]
async fn switch_keymap_resets_pending_state() {
    let mut ed = test_vim_editor("abc\ndef\nghi\n");
    // Start dd with count (partial command)
    ed.handle_key_for_test(KeyCode::Char('2'), KeyModifiers::NONE)
        .await;
    // Switch keymap mid-command — should reset pending count
    ed.switch_keymap("emacs");
    assert!(
        ed.engine.operator_state.count.is_none(),
        "pending_count cleared"
    );
    assert!(
        matches!(ed.engine.operator_state.pending, PendingOperator::Idle),
        "command_state reset"
    );
    assert_eq!(ed.engine.state.mode, Mode::Normal);
}

#[tokio::test]
async fn switch_keymap_via_command_mode() {
    let mut ed = test_vim_editor("hello\n");
    // :keymap emacs in command mode
    ed.handle_key_for_test(KeyCode::Char(':'), KeyModifiers::NONE)
        .await;
    for ch in "keymap emacs".chars() {
        ed.handle_key_for_test(KeyCode::Char(ch), KeyModifiers::NONE)
            .await;
    }
    ed.handle_key_for_test(KeyCode::Enter, KeyModifiers::NONE)
        .await;

    // Now in Emacs mode: M-f should move word forward
    let col_before = ed.engine.state.cursor.col;
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::ALT)
        .await;
    assert!(
        ed.engine.state.cursor.col > col_before,
        "M-f works after :keymap emacs"
    );
}

#[tokio::test]
async fn switch_keymap_unknown_name_does_not_panic() {
    let mut ed = test_vim_editor("hello\n");
    ed.switch_keymap("unknown_mode");
    // Still in Vim mode — 'x' deletes char
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ello\n", "still in vim mode after unknown keymap");
}

#[tokio::test]
async fn vim_c_after_emacs_switch_enters_visual_block() {
    let mut ed = test_vim_editor("hello\nworld\n");
    // Switch to Emacs then back to Vim
    ed.switch_keymap("emacs");
    ed.switch_keymap("vim");

    // Ctrl-v should enter visual block mode
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
    assert_eq!(ed.engine.state.mode, Mode::Visual);
    assert_eq!(ed.engine.state.visual_type, Some(VisualType::Block));
    // Esc should exit
    ed.handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;
    assert_eq!(ed.engine.state.mode, Mode::Normal);
}

// ── Macro recording and playback tests ──────────────────────

#[tokio::test]
async fn vim_macro_keys_are_stored_correctly() {
    let mut ed = test_vim_editor("abc\ndef\n");

    // Record qajq (just a 'j' movement)
    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('j'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;

    let keys = ed.engine.state.macros.get('a').cloned();
    assert!(keys.is_some(), "macro 'a' should exist");
    let keys = keys.unwrap();
    assert_eq!(keys.len(), 1, "macro should have exactly 1 key (j)");
    assert_eq!(keys[0], "j", "macro key should be 'j'");

    // Rewind cursor, play macro via @a
    ed.engine.state.cursor.line = 1;
    ed.engine.state.cursor.col = 0;

    ed.handle_key_for_test(KeyCode::Char('@'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;

    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 2, "@a should move to line 2");
}

#[tokio::test]
async fn vim_macro_playback_dispatch_has_correct_state() {
    let mut ed = test_vim_editor("abc\ndef\n");

    // Record qa: x then j
    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('j'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;

    let keys = ed.engine.state.macros.get('a').cloned().unwrap();
    assert_eq!(keys, vec!["x", "j"], "macro should be ['x', 'j']");

    // Reset and play via @a
    ed.set_buffer_for_test("abc\ndef\n");
    ed.handle_key_for_test(KeyCode::Char('@'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;

    let (buf, line, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "bc\ndef\n", "x deleted first char on playback");
    assert_eq!(line, 2, "j moved to line 2 on playback");
}

#[tokio::test]
async fn vim_macro_with_insert_mode() {
    let mut ed = test_vim_editor("ab\ncd\n");

    // qa → i → X → Esc → j → q
    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;

    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('X'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('j'), KeyModifiers::NONE)
        .await;

    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "Xab\ncd\n", "macro inserted 'X' and moved down");

    // Reset buffer and replay
    ed.set_buffer_for_test("ab\ncd\n");

    ed.handle_key_for_test(KeyCode::Char('@'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "Xab\ncd\n", "replayed macro on fresh buffer");
}

#[tokio::test]
async fn vim_macro_delete_word() {
    let mut ed = test_vim_editor("hello world\nfoo bar\n");

    // Record: diw
    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;

    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;

    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, " world\nfoo bar\n", "macro deleted 'hello'");
}

#[tokio::test]
async fn vim_macro_with_count() {
    let mut ed = test_vim_editor("a\nb\nc\nd\ne\n");

    // qa → 2j → q
    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;

    ed.handle_key_for_test(KeyCode::Char('2'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('j'), KeyModifiers::NONE)
        .await;

    ed.handle_key_for_test(KeyCode::Char('q'), KeyModifiers::NONE)
        .await;

    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 3, "2j moved to line 3");

    // Reset and replay
    ed.engine.state.cursor.line = 1;

    ed.handle_key_for_test(KeyCode::Char('@'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;

    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 3, "@a repeated 2j");
}

// ── New feature tests ────────────────────────────────────────────

#[tokio::test]
async fn vim_tilde_toggle_case() {
    let mut ed = test_vim_editor("abc\n");
    // Move to 'a' (already at col 0) and toggle case
    ed.handle_key_for_test(KeyCode::Char('~'), KeyModifiers::NONE)
        .await;
    let (buf, _, col, _) = ed.snapshot_for_test();
    assert!(buf.starts_with("Abc"), "toggle 'a' -> 'A': got {:?}", buf);
    assert_eq!(col, 1, "cursor should advance to col 1");
}

#[tokio::test]
async fn vim_tilde_toggle_lowercase() {
    let mut ed = test_vim_editor("ABC\n");
    ed.handle_key_for_test(KeyCode::Char('~'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert!(buf.starts_with("aBC"), "toggle 'A' -> 'a': got {:?}", buf);
}

#[tokio::test]
async fn vim_find_char_cross_line_forward() {
    let mut ed = test_vim_editor("hello\nworld\n");
    // 'f' then 'w' should find 'w' on line 2
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!(line, 2, "cross-line f should go to line 2");
    assert_eq!(col, 0, "cross-line f should land on col 0");
}

#[tokio::test]
async fn vim_find_char_backward_cross_line() {
    let mut ed = test_vim_editor("hello\nworld\n");
    // Move to line 2 col 2
    ed.engine.state.cursor.line = 2;
    ed.engine.state.cursor.col = 2;
    // 'F' then 'e' should find 'e' on line 1
    ed.handle_key_for_test(KeyCode::Char('F'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('e'), KeyModifiers::NONE)
        .await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!(line, 1, "backward F should go to line 1");
    assert_eq!(col, 1, "backward F should land on col 1");
}

#[tokio::test]
async fn vim_till_char_cross_line() {
    let mut ed = test_vim_editor("hello\nworld\n");
    // 't' then 'o' should find 'o' on line 2 and land before it
    ed.handle_key_for_test(KeyCode::Char('t'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('o'), KeyModifiers::NONE)
        .await;
    let (_, line, col, _) = ed.snapshot_for_test();
    // t forward to next line: cursor ends up at end of line 1
    assert_eq!(
        line, 1,
        "cross-line t should be on line 1 (before line 2 char)"
    );
    assert_eq!(
        col, 3,
        "till char should be at col 3 (before 'o' on line 2)"
    );
}

#[tokio::test]
async fn vim_text_object_i_w() {
    // iw on "hello world" should select "hello" (word)
    let mut ed = test_vim_editor("hello world\n");
    ed.engine.state.cursor.line = 1;
    ed.engine.state.cursor.col = 0;

    // d i W
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('i'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('W'), KeyModifiers::NONE)
        .await;

    let (buf, _, col, _) = ed.snapshot_for_test();
    // iW deletes the WORD under cursor = "hello"
    assert_eq!(buf.trim(), "world", "iW should delete WORD 'hello'");
    assert_eq!(col, 0, "cursor at col 0 after delete");
}

#[tokio::test]
async fn vim_replace_mode_undo_group() {
    let mut ed = test_vim_editor("abc\n");
    // Enter replace mode with R
    ed.handle_key_for_test(KeyCode::Char('R'), KeyModifiers::NONE)
        .await;

    // Type some chars
    ed.handle_key_for_test(KeyCode::Char('X'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('Y'), KeyModifiers::NONE)
        .await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf.trim(), "XYc", "replace mode should replace chars");

    // Exit replace mode
    ed.handle_key_for_test(KeyCode::Esc, KeyModifiers::NONE)
        .await;

    // Undo should revert the whole replace session
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(
        buf.trim(),
        "abc",
        "undo after replace should restore original"
    );
}

#[tokio::test]
async fn vim_dot_repeat_join() {
    let mut ed = test_vim_editor("line1\nline2\nline3\n");

    // Join lines with J
    ed.handle_key_for_test(KeyCode::Char('J'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert!(
        buf.starts_with("line1 line2"),
        "J should join: got {:?}",
        buf
    );

    // Move to beginning of current line first, then . to repeat
    ed.handle_key_for_test(KeyCode::Char('0'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('.'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert!(
        buf.contains("line1 line2 line3"),
        ". should repeat J: got {:?}",
        buf
    );
}

#[tokio::test]
async fn vim_indent_uses_expandtab() {
    let mut ed = test_vim_editor("hello\n");
    // Set expandtab
    ed.engine.state.expandtab = true;
    ed.engine.state.shiftwidth = 4;

    // >> to indent
    ed.handle_key_for_test(KeyCode::Char('>'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('>'), KeyModifiers::NONE)
        .await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert!(
        buf.starts_with("    hello"),
        ">> with expandtab should insert 4 spaces: got {:?}",
        buf
    );
}

#[tokio::test]
async fn vim_multibuffer_switch() {
    let mut ed = test_vim_editor("buffer1\n");

    // Open another buffer
    let _ = ed.engine.buffer_open("buffer2.txt"); // Will fail since file doesn't exist, but that's OK

    // Manually set up a second buffer for testing
    let mut buf2 = ijevim::buffer::TextBuffer::new();
    buf2.insert(1, 0, "buffer2 content\n");
    let undo2 = ijevim::undo::UndoManager::new();
    ed.engine.inactive_buffers.push((
        buf2,
        undo2,
        Some(std::path::PathBuf::from("buffer2.txt")),
        ijevim::types::Position { line: 1, col: 0 },
    ));

    // Switch to next buffer
    ed.engine.buffer_next().unwrap();
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf.trim(), "buffer2 content", "bn should switch to buffer2");

    // Switch back
    ed.engine.buffer_prev().unwrap();
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf.trim(), "buffer1", "bp should switch back to buffer1");
}

#[tokio::test]
async fn vim_dot_repeat_indent() {
    let mut ed = test_vim_editor("line1\nline2\n");

    // >> to indent
    ed.handle_key_for_test(KeyCode::Char('>'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('>'), KeyModifiers::NONE)
        .await;

    // Move down
    ed.handle_key_for_test(KeyCode::Char('j'), KeyModifiers::NONE)
        .await;

    // Repeat indent with .
    ed.handle_key_for_test(KeyCode::Char('.'), KeyModifiers::NONE)
        .await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    // Both lines should be indented (tab or spaces depending on expandtab)
    let lines: Vec<&str> = buf.lines().collect();
    assert!(
        lines[0].starts_with('\t') || lines[0].starts_with("    "),
        "line1 should be indented"
    );
    assert!(
        lines[1].starts_with('\t') || lines[1].starts_with("    "),
        "line2 should be indented by dot repeat"
    );
}

#[tokio::test]
async fn vim_command_line_ctrl_a_ctrl_e() {
    let mut ed = test_vim_editor("hello\n");

    // Enter command mode
    ed.handle_key_for_test(KeyCode::Char(':'), KeyModifiers::NONE)
        .await;

    // Type some text
    ed.handle_key_for_test(KeyCode::Char('h'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('e'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('l'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('o'), KeyModifiers::NONE)
        .await;

    assert_eq!(ed.engine.state.command_buffer, "hello");
    assert_eq!(ed.engine.state.command_cursor_pos, 5);

    // Ctrl-a to go to beginning
    ed.handle_key_for_test(KeyCode::Char('a'), KeyModifiers::CONTROL)
        .await;
    assert_eq!(ed.engine.state.command_cursor_pos, 0);

    // Ctrl-e to go to end
    ed.handle_key_for_test(KeyCode::Char('e'), KeyModifiers::CONTROL)
        .await;
    assert_eq!(ed.engine.state.command_cursor_pos, 5);
}

#[tokio::test]
async fn vim_command_line_ctrl_w() {
    let mut ed = test_vim_editor("hello\n");

    // Enter command mode
    ed.handle_key_for_test(KeyCode::Char(':'), KeyModifiers::NONE)
        .await;

    // Type "foo bar"
    for c in "foo bar".chars() {
        ed.handle_key_for_test(KeyCode::Char(c), KeyModifiers::NONE)
            .await;
    }

    assert_eq!(ed.engine.state.command_buffer, "foo bar");

    // Ctrl-w to delete word backward
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::CONTROL)
        .await;

    assert_eq!(
        ed.engine.state.command_buffer, "foo",
        "Ctrl-w should delete ' bar'"
    );
}

// ── Regression tests ────────────────────────────────────────────────
// Each of these pins a bug that shipped: they must stay green.

async fn press_key(ed: &mut Editor, key: KeyCode) {
    ed.handle_key_for_test(key, KeyModifiers::NONE).await;
}

async fn type_keys(ed: &mut Editor, keys: &str) {
    for c in keys.chars() {
        press_key(ed, KeyCode::Char(c)).await;
    }
}

/// Run an ex command (`:cmd<Enter>`) from normal mode.
async fn run_command(ed: &mut Editor, cmd: &str) {
    press_key(ed, KeyCode::Char(':')).await;
    type_keys(ed, cmd).await;
    press_key(ed, KeyCode::Enter).await;
}

#[tokio::test]
async fn substitute_keeps_line_breaks() {
    let mut ed = test_vim_editor("aaa\nbbb\nccc\n");

    run_command(&mut ed, "%s/[abc]/X/g").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(
        buf, "XXX\nXXX\nXXX\n",
        "each line must stay on its own line"
    );
}

#[tokio::test]
async fn substitute_is_one_undo_step() {
    let mut ed = test_vim_editor("aaa\nbbb\n");

    run_command(&mut ed, "%s/a/X/g").await;
    press_key(&mut ed, KeyCode::Char('u')).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "aaa\nbbb\n");
}

#[tokio::test]
async fn visual_delete_is_undoable() {
    let mut ed = test_vim_editor("abcdef\n");

    press_key(&mut ed, KeyCode::Char('v')).await;
    press_key(&mut ed, KeyCode::Char('l')).await;
    press_key(&mut ed, KeyCode::Char('l')).await;
    press_key(&mut ed, KeyCode::Char('d')).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "def\n");

    press_key(&mut ed, KeyCode::Char('u')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "abcdef\n", "visual delete must be undoable");
}

#[tokio::test]
async fn vim_dirty_quit_confirmation_accept_exits() {
    let mut ed = test_vim_editor("abc\n");
    ed.running = true;
    press_key(&mut ed, KeyCode::Char('i')).await;
    type_keys(&mut ed, "x").await;
    press_key(&mut ed, KeyCode::Esc).await;

    run_command(&mut ed, "q").await;
    assert!(ed.running, "dirty quit must ask for confirmation first");

    press_key(&mut ed, KeyCode::Char('y')).await;
    assert!(!ed.running, "y must accept the quit prompt in Vim mode");
}

#[tokio::test]
async fn named_register_yank_and_paste() {
    let mut ed = test_vim_editor("abc\ndef\n");

    type_keys(&mut ed, "\"ayy").await;
    type_keys(&mut ed, "\"ap").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "abc\nabc\ndef\n");
}

#[tokio::test]
async fn mark_set_and_jump() {
    let mut ed = test_vim_editor("abc\ndef\n");

    type_keys(&mut ed, "ma").await;
    press_key(&mut ed, KeyCode::Char('j')).await;

    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 2);

    type_keys(&mut ed, "`a").await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 1, "`a must jump back to the mark");
}

#[tokio::test]
async fn emacs_enter_does_not_duplicate_text() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless editor");
    ed.set_buffer_for_test("abc\n");

    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    press_key(&mut ed, KeyCode::Enter).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "a\nbc\n");
}

#[tokio::test]
async fn emacs_enter_handles_multibyte() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless editor");
    ed.set_buffer_for_test("あいう\n");

    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .await;
    press_key(&mut ed, KeyCode::Enter).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "あ\nいう\n");
}

#[tokio::test]
async fn join_is_undoable() {
    let mut ed = test_vim_editor("a\nb\n");

    press_key(&mut ed, KeyCode::Char('J')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "a b\n");

    press_key(&mut ed, KeyCode::Char('u')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "a\nb\n");
}

#[tokio::test]
async fn insert_mode_tab_inserts_whitespace() {
    let mut ed = test_vim_editor("abc\n");

    press_key(&mut ed, KeyCode::Char('i')).await;
    press_key(&mut ed, KeyCode::Tab).await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_ne!(buf, "abc\n", "Tab must insert indentation in Insert mode");
    assert!(
        buf.starts_with(' ') || buf.starts_with('\t'),
        "expected indentation, got {:?}",
        buf
    );
}

#[tokio::test]
async fn three_dd_is_one_undo_step() {
    let mut ed = test_vim_editor("1\n2\n3\n4\n");

    type_keys(&mut ed, "3dd").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "4\n");

    press_key(&mut ed, KeyCode::Char('u')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "1\n2\n3\n4\n", "3dd must undo in one step");
}

#[tokio::test]
async fn change_inner_word_is_undoable() {
    let mut ed = test_vim_editor("hello world\n");

    type_keys(&mut ed, "ciw").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, " world\n");

    press_key(&mut ed, KeyCode::Char('u')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n");
}

#[tokio::test]
async fn messages_command_does_not_grow_history() {
    let mut ed = test_vim_editor("abc\n");
    run_command(&mut ed, "messages").await;
    let after_first = ed.engine.state.message_history.len();
    run_command(&mut ed, "messages").await;
    let after_second = ed.engine.state.message_history.len();
    assert_eq!(
        after_first, after_second,
        ":messages must not append the whole log back into itself"
    );
}

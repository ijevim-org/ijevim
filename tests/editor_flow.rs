use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::{Keymap, Mode, PendingOperator, SearchDirection, VisualType};

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
    assert_eq!(col, 6, "w moves to the start of the next word");
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
    assert_eq!(col, 6, "M-f moves to the start of the next word");
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

// ── Emacs C-x u undo ───────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_x_u_undo() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    ed.engine.state.cursor.col = 5;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "helloworld\n", "C-d deleted space");

    // C-x u is undo in Emacs.
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('u'), KeyModifiers::NONE)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "C-x u undid deletion");

    // Redo stays reachable through the documented extension C-M-/.
    ed.handle_key_for_test(
        KeyCode::Char('/'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    )
    .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "helloworld\n", "C-M-/ redid deletion");
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

// ── Emacs C-x h / C-x d ────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_x_h_marks_whole_buffer() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    ed.engine.state.cursor = ijevim::types::Position { line: 1, col: 5 };

    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('h'), KeyModifiers::NONE)
        .await;

    assert_eq!(
        ed.engine.state.mark,
        Some(ijevim::types::Position { line: 1, col: 0 }),
        "C-x h puts the mark at the start of the buffer"
    );
    assert_eq!(
        ed.engine.state.cursor,
        ijevim::types::Position { line: 1, col: 11 },
        "C-x h puts point at the end of the buffer"
    );
    assert!(ed.engine.state.region_active, "C-x h activates the region");
}

#[tokio::test]
async fn emacs_ctrl_x_d_is_undefined() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    ed.engine.state.cursor.col = 2;

    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await;

    assert_eq!(
        ed.engine.state.cursor.col, 2,
        "C-x d must not move point (dired is not implemented)"
    );
    assert!(
        ed.engine
            .state
            .last_message
            .as_deref()
            .is_some_and(|m| m.contains("undefined")),
        "C-x d reports an undefined sequence"
    );
}

// ── Emacs C-x C-/ undo ──────────────────────────────────────

#[tokio::test]
async fn emacs_ctrl_x_ctrl_slash_undo() {
    let mut ed = Editor::new_headless_for_test(Keymap::Emacs).expect("headless");
    ed.set_buffer_for_test("hello world\n");
    ed.engine.state.cursor.col = 5;

    // C-d delete space
    ed.handle_key_for_test(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "helloworld\n", "C-d deleted space");

    // C-x C-/ is also undo in Emacs.
    ed.handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    ed.handle_key_for_test(KeyCode::Char('/'), KeyModifiers::CONTROL)
        .await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "C-x C-/ undid deletion");
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
    assert_eq!(buf, "", "C-k at bol killed the whole line");

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
async fn vim_find_char_stays_on_the_line_forward() {
    let mut ed = test_vim_editor("hello\nworld\n");
    // Vim's `f` never crosses a line, so this finds nothing.
    ed.handle_key_for_test(KeyCode::Char('f'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!(line, 1, "f must stay on the current line");
    assert_eq!(col, 0);
}

#[tokio::test]
async fn vim_find_char_stays_on_the_line_backward() {
    let mut ed = test_vim_editor("hello\nworld\n");
    // Move to line 2 col 2
    ed.engine.state.cursor.line = 2;
    ed.engine.state.cursor.col = 2;
    // `F` only searches the current line, so it finds nothing here.
    ed.handle_key_for_test(KeyCode::Char('F'), KeyModifiers::NONE)
        .await;
    ed.handle_key_for_test(KeyCode::Char('e'), KeyModifiers::NONE)
        .await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!(line, 2, "F must stay on the current line");
    assert_eq!(col, 2);
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

#[tokio::test]
async fn delete_to_end_of_line() {
    let mut ed = test_vim_editor("hello world\nnext\n");
    type_keys(&mut ed, "de").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, " world\nnext\n");

    type_keys(&mut ed, "d$").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\nnext\n");
}

#[tokio::test]
async fn delete_to_line_start() {
    let mut ed = test_vim_editor("hello world\n");
    type_keys(&mut ed, "lll").await;
    type_keys(&mut ed, "d0").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "lo world\n");
}

#[tokio::test]
async fn delete_word_backward_motion() {
    let mut ed = test_vim_editor("hello world\n");
    type_keys(&mut ed, "w").await;
    type_keys(&mut ed, "db").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "world\n");
}

#[tokio::test]
async fn delete_next_line_with_dj() {
    let mut ed = test_vim_editor("1\n2\n3\n");
    type_keys(&mut ed, "dj").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "3\n");
}

#[tokio::test]
async fn delete_previous_line_with_dk() {
    let mut ed = test_vim_editor("1\n2\n3\n");
    press_key(&mut ed, KeyCode::Char('j')).await;
    type_keys(&mut ed, "dk").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "3\n");
}

#[tokio::test]
async fn delete_to_end_of_file_with_dg() {
    let mut ed = test_vim_editor("1\n2\n3\n");
    press_key(&mut ed, KeyCode::Char('j')).await;
    type_keys(&mut ed, "dG").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "1\n");
}

#[tokio::test]
async fn yank_to_end_of_line_then_paste() {
    let mut ed = test_vim_editor("hello world\n");
    type_keys(&mut ed, "y$").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n", "yank must not modify the buffer");

    type_keys(&mut ed, "p").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert!(
        buf.contains("hello world"),
        "pasted content missing: {:?}",
        buf
    );
}

#[tokio::test]
async fn change_to_end_of_line_enters_insert() {
    let mut ed = test_vim_editor("hello world\n");
    type_keys(&mut ed, "wc$").await;
    press_key(&mut ed, KeyCode::Esc).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello \n");
}

#[tokio::test]
async fn operator_motions_are_undoable() {
    let mut ed = test_vim_editor("1\n2\n3\n");
    type_keys(&mut ed, "dj").await;
    press_key(&mut ed, KeyCode::Char('u')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "1\n2\n3\n");
}

// ── Cursor bounds never include the newline (B31) ───────────────────

#[tokio::test]
async fn cursor_never_rests_on_newline() {
    let mut ed = test_vim_editor("abc\n");
    for _ in 0..10 {
        press_key(&mut ed, KeyCode::Char('l')).await;
    }
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(
        col, 2,
        "`l` must stop on the last character, not the newline"
    );
}

#[tokio::test]
async fn line_end_motion_lands_on_last_char() {
    let mut ed = test_vim_editor("hello world\n");
    press_key(&mut ed, KeyCode::Char('$')).await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 10);

    // `x` at `$` must delete 'd', never the line terminator.
    press_key(&mut ed, KeyCode::Char('x')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello worl\n");
}

#[tokio::test]
async fn motions_on_empty_line_do_not_panic() {
    let mut ed = test_vim_editor("\n");
    type_keys(&mut ed, "llll$0jjkweb").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\n");
}

#[tokio::test]
async fn vertical_motion_clamps_to_shorter_line() {
    let mut ed = test_vim_editor("longer line\nx\n");
    press_key(&mut ed, KeyCode::Char('$')).await;
    press_key(&mut ed, KeyCode::Char('j')).await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!(line, 2);
    assert_eq!(
        col, 0,
        "j must clamp to the last character of the target line"
    );
}

#[tokio::test]
async fn delete_to_end_of_line_is_undoable() {
    let mut ed = test_vim_editor("hello world\n");
    type_keys(&mut ed, "wd$").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello \n");

    press_key(&mut ed, KeyCode::Char('u')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n");
}

#[tokio::test]
async fn line_end_on_tabbed_line_stops_before_newline() {
    let mut ed = test_vim_editor("\tab\n");
    press_key(&mut ed, KeyCode::Char('$')).await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 2, "the tab counts as one character");
}

// ── Window splits survive a resize (B15) ────────────────────────────

fn window_bounds(ed: &Editor) -> Vec<(usize, usize, usize, usize)> {
    ed.engine
        .window_layout
        .windows
        .iter()
        .map(|w| (w.row_start, w.row_end, w.col_start, w.col_end))
        .collect()
}

#[tokio::test]
async fn split_survives_resize() {
    let mut ed = test_vim_editor("abc\n");
    run_command(&mut ed, "split").await;
    assert_eq!(
        ed.engine.window_layout.windows.len(),
        2,
        ":split must create a second window"
    );

    ed.engine.update_window_layout(40, 100);

    assert_eq!(
        ed.engine.window_layout.windows.len(),
        2,
        "a resize must not close the split"
    );
    for (row_start, row_end, col_start, col_end) in window_bounds(&ed) {
        assert!(row_end > row_start, "window collapsed vertically");
        assert!(col_end > col_start, "window collapsed horizontally");
    }
}

#[tokio::test]
async fn vsplit_survives_resize() {
    let mut ed = test_vim_editor("abc\n");
    run_command(&mut ed, "vsplit").await;
    assert_eq!(ed.engine.window_layout.windows.len(), 2);

    ed.engine.update_window_layout(40, 100);

    let bounds = window_bounds(&ed);
    assert_eq!(bounds.len(), 2, "a resize must not close the vsplit");
    assert_eq!(bounds[1].2, bounds[0].3, "windows must stay contiguous");
    assert_eq!(
        bounds[1].3, 100,
        "the last window must reach the right edge"
    );
}

// ── WORD vs word (B58) ─────────────────────────────────────────────

#[tokio::test]
async fn word_motion_stops_at_punctuation() {
    let mut ed = test_vim_editor("foo.bar baz\n");
    press_key(&mut ed, KeyCode::Char('w')).await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 3, "w must stop at the punctuation");
}

#[tokio::test]
async fn word_motion_big_skips_punctuation() {
    let mut ed = test_vim_editor("foo.bar baz\n");
    press_key(&mut ed, KeyCode::Char('W')).await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 8, "W must jump over the whole non-blank run");
}

#[tokio::test]
async fn word_backward_big_skips_punctuation() {
    let mut ed = test_vim_editor("foo.bar baz\n");
    type_keys(&mut ed, "WB").await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 0);
}

#[tokio::test]
async fn word_end_big_stops_at_word_end() {
    let mut ed = test_vim_editor("foo.bar baz\n");
    type_keys(&mut ed, "E").await;
    let (_, _, col, _) = ed.snapshot_for_test();
    assert_eq!(col, 6, "E must stop on the last char of the WORD");
}

#[tokio::test]
async fn inner_word_text_object_stops_at_punctuation() {
    let mut ed = test_vim_editor("foo.bar\n");
    type_keys(&mut ed, "diw").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, ".bar\n");
}

#[tokio::test]
async fn inner_word_big_deletes_the_whole_word() {
    let mut ed = test_vim_editor("foo.bar baz\n");
    type_keys(&mut ed, "diW").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, " baz\n");
}

#[tokio::test]
async fn delete_word_operator_is_undoable() {
    let mut ed = test_vim_editor("hello world\n");
    type_keys(&mut ed, "dw").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "world\n", "dw must take the trailing blank with it");

    press_key(&mut ed, KeyCode::Char('u')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n");
}

#[tokio::test]
async fn delete_word_with_count_is_one_undo_step() {
    let mut ed = test_vim_editor("one two three\n");
    type_keys(&mut ed, "2dw").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "three\n");

    press_key(&mut ed, KeyCode::Char('u')).await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "one two three\n");
}

#[tokio::test]
async fn delete_word_big_operator() {
    let mut ed = test_vim_editor("foo.bar baz\n");
    type_keys(&mut ed, "dW").await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "baz\n");
}

// ── hlsearch (B14) ─────────────────────────────────────────────────

#[tokio::test]
async fn search_records_the_pattern_for_highlighting() {
    let mut ed = test_vim_editor("foo bar foo\n");
    press_key(&mut ed, KeyCode::Char('/')).await;
    type_keys(&mut ed, "foo").await;
    press_key(&mut ed, KeyCode::Enter).await;

    assert!(ed.engine.state.hlsearch, "hlsearch is on by default");
    assert_eq!(
        ed.engine.state.search_pattern.as_deref(),
        Some("foo"),
        "the renderer needs the pattern to paint matches"
    );
    assert_eq!(ed.engine.search_state.query, "foo");
}

#[tokio::test]
async fn nohlsearch_disables_the_highlight() {
    let mut ed = test_vim_editor("foo bar foo\n");
    press_key(&mut ed, KeyCode::Char('/')).await;
    type_keys(&mut ed, "foo").await;
    press_key(&mut ed, KeyCode::Enter).await;

    run_command(&mut ed, "nohlsearch").await;
    assert!(!ed.engine.state.hlsearch);

    run_command(&mut ed, "set hlsearch").await;
    assert!(ed.engine.state.hlsearch, "`set hlsearch` must re-enable it");
}

#[tokio::test]
async fn slash_starts_a_forward_search_and_n_repeats() {
    let mut ed = test_vim_editor("foo bar foo\n");
    press_key(&mut ed, KeyCode::Char('/')).await;
    type_keys(&mut ed, "foo").await;
    press_key(&mut ed, KeyCode::Enter).await;

    assert_eq!(ed.engine.state.mode, Mode::Normal);
    assert_eq!(ed.engine.search_state.direction, SearchDirection::Forward);
    assert_eq!(ed.engine.search_state.results.len(), 2);

    let first = ed.snapshot_for_test().2;
    press_key(&mut ed, KeyCode::Char('n')).await;
    let second = ed.snapshot_for_test().2;
    assert_ne!(first, second, "n must jump to the next match");
}

#[tokio::test]
async fn question_mark_starts_a_backward_search() {
    let mut ed = test_vim_editor("foo bar foo\n");
    press_key(&mut ed, KeyCode::Char('?')).await;
    type_keys(&mut ed, "foo").await;
    press_key(&mut ed, KeyCode::Enter).await;

    assert_eq!(ed.engine.search_state.direction, SearchDirection::Backward);
    assert_eq!(ed.engine.state.search_pattern.as_deref(), Some("foo"));
    assert_eq!(ed.engine.search_state.results.len(), 2);
}

// ── Review mode must use real git data (B28) ────────────────────────

#[tokio::test]
async fn review_enters_mode_from_a_real_repository() {
    let mut ed = test_vim_editor("abc\n");
    run_command(&mut ed, "review").await;

    assert!(
        ed.engine.review_state.is_some(),
        ":review inside a git checkout must build a review state from git"
    );
}

#[tokio::test]
async fn review_reports_git_failures_instead_of_mocking() {
    let mut ed = test_vim_editor("abc\n");
    run_command(&mut ed, "review deadbeef").await;

    assert!(
        ed.engine.review_state.is_none(),
        "a failed git diff must not fabricate review data"
    );
    assert!(
        ed.engine
            .state
            .message_history
            .iter()
            .any(|m| m.text.contains("review:")),
        "the failure reason must be reported: {:?}",
        ed.engine
            .state
            .message_history
            .iter()
            .map(|m| m.text.clone())
            .collect::<Vec<_>>()
    );
}

// ── :substitute ranges and replacement syntax (B19) ─────────────────

#[tokio::test]
async fn substitute_supports_ampersand_for_the_whole_match() {
    let mut ed = test_vim_editor("foo bar foo\n");
    run_command(&mut ed, "%s/foo/<&>/g").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "<foo> bar <foo>\n");
}

#[tokio::test]
async fn substitute_supports_capture_group_backreferences() {
    let mut ed = test_vim_editor("ab\n");
    run_command(&mut ed, r"%s/(a)(b)/\2\1/").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ba\n");
}

#[tokio::test]
async fn substitute_respects_a_line_range() {
    let mut ed = test_vim_editor("x\nx\nx\n");
    run_command(&mut ed, "1,2s/x/y/").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "y\ny\nx\n");
}

#[tokio::test]
async fn substitute_supports_the_last_line_address() {
    let mut ed = test_vim_editor("x\nx\n");
    // `$` is the final buffer line; the trailing empty line is line 3.
    run_command(&mut ed, "2s/x/y/").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "x\ny\n");
}

#[tokio::test]
async fn substitute_reports_an_invalid_range() {
    let mut ed = test_vim_editor("x\n");
    run_command(&mut ed, "1,xs/x/y/").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "x\n", "an invalid range must not modify the buffer");
    assert!(
        ed.engine
            .state
            .message_history
            .iter()
            .any(|m| m.text.contains("Invalid range")),
        "the range error must be reported"
    );
}

// ── Vim motion compatibility (verified against Vim 9.1) ─────────────

#[tokio::test]
async fn word_forward_crosses_lines_to_the_first_non_blank() {
    let mut ed = test_vim_editor("foo bar\n   baz\n");
    type_keys(&mut ed, "ww").await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "foo bar\n   baz\n");
    assert_eq!(
        (line, col),
        (2, 3),
        "w must cross the line break and skip blanks"
    );
}

#[tokio::test]
async fn word_forward_stops_on_an_empty_line() {
    let mut ed = test_vim_editor("foo bar\n\nbaz\n");
    type_keys(&mut ed, "ww").await;

    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (2, 0), "an empty line is a word stop");
}

#[tokio::test]
async fn word_backward_crosses_lines() {
    let mut ed = test_vim_editor("foo bar\nbaz\n");
    type_keys(&mut ed, "ww").await;
    type_keys(&mut ed, "b").await;

    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (1, 4), "b must walk back over the line break");
}

#[tokio::test]
async fn word_end_honours_the_count() {
    let mut ed = test_vim_editor("foo bar baz\n");
    type_keys(&mut ed, "2e").await;

    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (1, 6), "2e ends on the second word");
}

#[tokio::test]
async fn delete_word_with_count_crosses_lines() {
    let mut ed = test_vim_editor("foo bar\nqux quux\nzap\n");
    type_keys(&mut ed, "3dw").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "quux\nzap\n", "3dw spans the line break");
}

#[tokio::test]
async fn delete_word_end_is_inclusive() {
    let mut ed = test_vim_editor("foo bar\n");
    type_keys(&mut ed, "dE").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, " bar\n", "dE deletes through the end of the WORD");
}

#[tokio::test]
async fn change_line_keeps_one_empty_line() {
    let mut ed = test_vim_editor("foo bar\nqux quux\n");
    type_keys(&mut ed, "cc").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\nqux quux\n", "cc clears the line but keeps it");
}

#[tokio::test]
async fn linewise_paste_inserts_below_and_moves_the_cursor() {
    let mut ed = test_vim_editor("foo bar\nqux quux\nzap\n");
    type_keys(&mut ed, "ddp").await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "qux quux\nfoo bar\nzap\n", "p puts the line below");
    assert_eq!((line, col), (2, 0), "cursor lands on the pasted line");
}

#[tokio::test]
async fn charwise_paste_leaves_the_cursor_on_the_last_char() {
    let mut ed = test_vim_editor("foo bar\n");
    type_keys(&mut ed, "xp").await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ofo bar\n");
    assert_eq!((line, col), (1, 1));
}

#[tokio::test]
async fn paste_repeats_with_a_count() {
    let mut ed = test_vim_editor("foo bar\nqux quux\n");
    type_keys(&mut ed, "yy2p").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "foo bar\nfoo bar\nfoo bar\nqux quux\n");
}

#[tokio::test]
async fn g_goes_to_the_last_real_line_first_non_blank() {
    let mut ed = test_vim_editor("foo bar\n  qux quux\nzap\n");
    press_key(&mut ed, KeyCode::Char('G')).await;

    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (3, 0), "G ignores the phantom trailing line");
}

#[tokio::test]
async fn count_g_goes_to_that_line_first_non_blank() {
    let mut ed = test_vim_editor("foo bar\n  qux quux\nzap\n");
    type_keys(&mut ed, "2G").await;

    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (2, 2), "2G lands on the first non-blank");
}

#[tokio::test]
async fn linewise_delete_at_the_first_line_is_a_no_op_for_k() {
    let mut ed = test_vim_editor("foo bar\nqux quux\n");
    type_keys(&mut ed, "dk").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(
        buf, "foo bar\nqux quux\n",
        "dk on the first line deletes nothing"
    );
}

#[tokio::test]
async fn linewise_delete_at_the_last_line_is_a_no_op_for_j() {
    let mut ed = test_vim_editor("foo bar\nqux quux\n");
    type_keys(&mut ed, "Gdj").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(
        buf, "foo bar\nqux quux\n",
        "dj on the last line deletes nothing"
    );
}

#[tokio::test]
async fn delete_to_gg_crosses_to_the_first_line() {
    let mut ed = test_vim_editor("foo bar\nqux quux\nzap\n");
    type_keys(&mut ed, "jdgg").await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "zap\n");
    assert_eq!((line, col), (1, 0));
}

#[tokio::test]
async fn change_to_gg_keeps_one_empty_line() {
    let mut ed = test_vim_editor("foo bar\nqux quux\nzap\n");
    type_keys(&mut ed, "jcgg").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\nzap\n", "cgg replaces lines 1-2 with one empty line");
}

#[tokio::test]
async fn join_with_count_leaves_the_cursor_on_the_last_separator() {
    let mut ed = test_vim_editor("foo bar\nqux quux\nzap\n");
    type_keys(&mut ed, "3J").await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "foo bar qux quux zap\n");
    assert_eq!(
        (line, col),
        (1, 16),
        "3J leaves the cursor on the last join"
    );
}

#[tokio::test]
async fn insert_at_first_non_blank_uses_the_first_non_blank() {
    let mut ed = test_vim_editor("    indented\nline\n");
    type_keys(&mut ed, "I!").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "    !indented\nline\n");
    assert_eq!((line, col), (1, 4));
}

#[tokio::test]
async fn indent_with_count_indents_several_lines() {
    let mut ed = test_vim_editor("one\ntwo\nthree\n");
    type_keys(&mut ed, "2>>").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\tone\n\ttwo\nthree\n", "2>> indents two lines");
}

#[tokio::test]
async fn replace_with_count_leaves_the_cursor_on_the_last_char() {
    let mut ed = test_vim_editor("foo bar\n");
    type_keys(&mut ed, "3r!").await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "!!! bar\n");
    assert_eq!((line, col), (1, 2));
}

#[tokio::test]
async fn replace_aborts_when_the_count_exceeds_the_line() {
    let mut ed = test_vim_editor("ab\n");
    type_keys(&mut ed, "5r!").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ab\n", "5r! on a two-character line changes nothing");
}

#[tokio::test]
async fn join_drops_the_indentation_of_the_next_line() {
    let mut ed = test_vim_editor("a\n   b\n");
    type_keys(&mut ed, "J").await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "a b\n", "J joins with a single space");
    assert_eq!((line, col), (1, 1), "cursor rests on the separator");
}

// ── Vim compatibility: character search stays on the current line ──

#[tokio::test]
async fn find_char_does_not_cross_line_boundaries() {
    let mut ed = test_vim_editor("abc\ndef\n");
    type_keys(&mut ed, "fd").await;

    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (1, 0), "f must not jump to the next line");

    type_keys(&mut ed, "fc").await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (1, 2));
}

#[tokio::test]
async fn till_char_backward_does_not_cross_line_boundaries() {
    let mut ed = test_vim_editor("abc\ndef\n");
    ed.engine.state.cursor.line = 2;
    ed.engine.state.cursor.col = 2;
    type_keys(&mut ed, "Ta").await;

    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (2, 2), "T must not leave the current line");
}

// ── Vim motions: ge/gE, paragraphs ────────────────────────────────

#[tokio::test]
async fn ge_moves_to_the_end_of_the_previous_word() {
    let mut ed = test_vim_editor("foo bar\n");
    ed.engine.state.cursor.col = 4;
    type_keys(&mut ed, "ge").await;

    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (1, 2));
}

#[tokio::test]
async fn paragraph_motions_stop_on_blank_lines() {
    let mut ed = test_vim_editor("a\n\nb\n\nc\n");
    type_keys(&mut ed, "}").await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 2);

    type_keys(&mut ed, "}").await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 4);

    type_keys(&mut ed, "}").await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 5, "the last }} reaches the end of the buffer");

    type_keys(&mut ed, "{").await;
    let (_, line, _, _) = ed.snapshot_for_test();
    assert_eq!(line, 4);
}

// ── Visual mode ───────────────────────────────────────────────────

#[tokio::test]
async fn visual_char_delete_removes_the_selection() {
    let mut ed = test_vim_editor("hello world\n");
    type_keys(&mut ed, "vlld").await;

    let (buf, line, col, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "lo world\n");
    assert_eq!((line, col), (1, 0));
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn visual_line_delete_removes_whole_lines() {
    let mut ed = test_vim_editor("one\ntwo\nthree\n");
    type_keys(&mut ed, "Vjd").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "three\n");
}

#[tokio::test]
async fn visual_block_delete_removes_the_block() {
    let mut ed = test_vim_editor("abc\ndef\n");
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
    type_keys(&mut ed, "jd").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "bc\nef\n");
}

#[tokio::test]
async fn visual_yank_fills_the_register() {
    let mut ed = test_vim_editor("hello world\n");
    type_keys(&mut ed, "vlllly").await;

    assert_eq!(ed.engine.register.get('"'), "hello");
    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "hello world\n");
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn visual_indent_shifts_every_selected_line() {
    let mut ed = test_vim_editor("one\ntwo\n");
    type_keys(&mut ed, "Vj>").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\tone\n\ttwo\n");
}

#[tokio::test]
async fn visual_replace_char_replaces_the_selection() {
    let mut ed = test_vim_editor("abc\n");
    type_keys(&mut ed, "vlrx").await;

    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "xxc\n");
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn visual_change_enters_insert_mode() {
    let mut ed = test_vim_editor("abc\n");
    type_keys(&mut ed, "vlcZ").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "Zc\n");
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn gv_restores_the_last_visual_selection() {
    let mut ed = test_vim_editor("hello\n");
    type_keys(&mut ed, "vll").await;
    press_key(&mut ed, KeyCode::Esc).await;
    type_keys(&mut ed, "gvd").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "lo\n");
}

// ── Command-line history and completion ───────────────────────────

#[tokio::test]
async fn command_history_recalls_previous_commands() {
    let mut ed = test_vim_editor("x\n");
    run_command(&mut ed, "set number").await;

    press_key(&mut ed, KeyCode::Char(':')).await;
    press_key(&mut ed, KeyCode::Up).await;
    assert_eq!(ed.engine.state.command_buffer, "set number");

    press_key(&mut ed, KeyCode::Down).await;
    assert_eq!(ed.engine.state.command_buffer, "");
}

#[tokio::test]
async fn search_history_is_separate_from_command_history() {
    let mut ed = test_vim_editor("foo bar\n");
    press_key(&mut ed, KeyCode::Char('/')).await;
    type_keys(&mut ed, "foo").await;
    press_key(&mut ed, KeyCode::Enter).await;
    run_command(&mut ed, "set number").await;

    press_key(&mut ed, KeyCode::Char('/')).await;
    press_key(&mut ed, KeyCode::Up).await;
    assert_eq!(ed.engine.state.command_buffer, "/foo");
}

#[tokio::test]
async fn tab_completes_set_options() {
    let mut ed = test_vim_editor("x\n");
    press_key(&mut ed, KeyCode::Char(':')).await;
    type_keys(&mut ed, "set nu").await;
    press_key(&mut ed, KeyCode::Tab).await;

    assert_eq!(ed.engine.state.command_buffer, "set number");
    assert!(ed
        .engine
        .state
        .command_hint
        .as_deref()
        .is_some_and(|h| h.starts_with("[1/")));
}

#[tokio::test]
async fn tab_cycles_between_candidates() {
    let mut ed = test_vim_editor("x\n");
    press_key(&mut ed, KeyCode::Char(':')).await;
    type_keys(&mut ed, "keymap ").await;
    press_key(&mut ed, KeyCode::Tab).await;
    let first = ed.engine.state.command_buffer.clone();

    press_key(&mut ed, KeyCode::Tab).await;
    let second = ed.engine.state.command_buffer.clone();

    assert_ne!(first, second, "second Tab must advance the cycle");
    assert!(first.starts_with("keymap "));
    assert!(second.starts_with("keymap "));
}

// ── Incremental search preview ────────────────────────────────────

#[tokio::test]
async fn incsearch_previews_matches_while_typing() {
    let mut ed = test_vim_editor("alpha\nbeta\nalpha\n");
    press_key(&mut ed, KeyCode::Char('/')).await;
    type_keys(&mut ed, "be").await;

    let (_, line, col, mode) = ed.snapshot_for_test();
    assert_eq!(mode, Mode::Command);
    assert_eq!((line, col), (2, 0), "preview jumps to the first match");
    assert_eq!(ed.engine.state.incsearch_match, Some((2, 0, 2)));

    press_key(&mut ed, KeyCode::Enter).await;
    let (_, line, col, mode) = ed.snapshot_for_test();
    assert_eq!(mode, Mode::Normal);
    assert_eq!((line, col), (2, 0));
    assert_eq!(ed.engine.state.search_pattern.as_deref(), Some("be"));
    assert_eq!(ed.engine.state.incsearch_match, None);
}

#[tokio::test]
async fn incsearch_cancel_restores_the_cursor() {
    let mut ed = test_vim_editor("alpha\nbeta\nalpha\n");
    press_key(&mut ed, KeyCode::Char('/')).await;
    type_keys(&mut ed, "be").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (_, line, col, mode) = ed.snapshot_for_test();
    assert_eq!(mode, Mode::Normal);
    assert_eq!((line, col), (1, 0));
    assert_eq!(ed.engine.state.incsearch_match, None);
    assert_eq!(ed.engine.state.command_buffer, "");
}

#[tokio::test]
async fn incsearch_can_be_disabled() {
    let mut ed = test_vim_editor("alpha\nbeta\n");
    ed.config.incsearch = false;
    press_key(&mut ed, KeyCode::Char('/')).await;
    type_keys(&mut ed, "be").await;

    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (1, 0), "no preview without incsearch");
}

// ── Ctrl-a / Ctrl-x ───────────────────────────────────────────────

async fn ctrl_key(ed: &mut Editor, c: char) {
    ed.handle_key_for_test(KeyCode::Char(c), KeyModifiers::CONTROL)
        .await;
}

#[tokio::test]
async fn ctrl_a_increments_the_number_under_the_cursor() {
    let mut ed = test_vim_editor("val 41\n");
    ctrl_key(&mut ed, 'a').await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "val 42\n");
    assert_eq!((line, col), (1, 4));
}

#[tokio::test]
async fn ctrl_x_decrements_the_number_under_the_cursor() {
    let mut ed = test_vim_editor("val 41\n");
    ctrl_key(&mut ed, 'x').await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "val 40\n");
}

#[tokio::test]
async fn ctrl_a_preserves_radix_and_padding() {
    let mut ed = test_vim_editor("0x1f 007\n");
    ctrl_key(&mut ed, 'a').await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "0x20 007\n");

    ed.set_buffer_for_test("007\n");
    ed.engine.state.cursor.col = 0;
    ctrl_key(&mut ed, 'a').await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "010\n");
}

#[tokio::test]
async fn ctrl_a_handles_negative_numbers() {
    let mut ed = test_vim_editor("-5\n");
    ctrl_key(&mut ed, 'a').await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "-4\n");
}

#[tokio::test]
async fn ctrl_a_uses_the_count() {
    let mut ed = test_vim_editor("7\n");
    ed.engine.operator_state.count = Some(3);
    ctrl_key(&mut ed, 'a').await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "10\n");
}

// ── Insert mode: Ctrl-t/d/o/v ─────────────────────────────────────

#[tokio::test]
async fn insert_ctrl_t_and_ctrl_d_indent_the_current_line() {
    let mut ed = test_vim_editor("x\n");
    ed.engine.state.expandtab = true;
    ed.engine.state.shiftwidth = 4;
    press_key(&mut ed, KeyCode::Char('i')).await;
    ctrl_key(&mut ed, 't').await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "    x\n");

    ctrl_key(&mut ed, 'd').await;
    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "x\n");
}

#[tokio::test]
async fn insert_ctrl_o_runs_one_normal_command() {
    let mut ed = test_vim_editor("abc\n");
    type_keys(&mut ed, "iX").await;
    ctrl_key(&mut ed, 'o').await;
    type_keys(&mut ed, "0Y").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "YXabc\n");
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn insert_ctrl_v_inserts_the_next_key_literally() {
    let mut ed = test_vim_editor("x\n");
    ed.engine.state.expandtab = true;
    ed.engine.state.shiftwidth = 4;
    press_key(&mut ed, KeyCode::Char('i')).await;
    ctrl_key(&mut ed, 'v').await;
    press_key(&mut ed, KeyCode::Tab).await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\tx\n", "Ctrl-v Tab inserts a literal tab");
}

// ── Viewport scrolling ────────────────────────────────────────────

fn prepare_scrolling(ed: &mut Editor) {
    let text: String = (1..=40).map(|n| format!("line{n}\n")).collect();
    ed.set_buffer_for_test(&text);
    ed.engine.update_window_layout(12, 40);
}

#[tokio::test]
async fn ctrl_e_and_ctrl_y_scroll_the_viewport() {
    let mut ed = test_vim_editor("");
    prepare_scrolling(&mut ed);
    ed.engine.state.cursor.line = 1;

    ctrl_key(&mut ed, 'e').await;
    let top = ed.engine.window_layout.windows[0].scroll_top;
    assert_eq!(top, 2);
    assert_eq!(
        ed.engine.state.cursor.line, 2,
        "cursor follows the top line"
    );

    ctrl_key(&mut ed, 'y').await;
    assert_eq!(ed.engine.window_layout.windows[0].scroll_top, 1);
    assert_eq!(ed.engine.state.cursor.line, 2);
}

#[tokio::test]
async fn zt_zz_zb_move_the_viewport_not_the_cursor() {
    let mut ed = test_vim_editor("");
    prepare_scrolling(&mut ed);
    ed.engine.state.cursor.line = 20;
    let rows = ed.engine.window_layout.windows[0]
        .row_end
        .saturating_sub(ed.engine.window_layout.windows[0].row_start);

    type_keys(&mut ed, "zt").await;
    assert_eq!(ed.engine.window_layout.windows[0].scroll_top, 20);
    assert_eq!(ed.engine.state.cursor.line, 20);

    type_keys(&mut ed, "zz").await;
    assert_eq!(ed.engine.window_layout.windows[0].scroll_top, 20 - rows / 2);
    assert_eq!(ed.engine.state.cursor.line, 20);

    type_keys(&mut ed, "zb").await;
    assert_eq!(
        ed.engine.window_layout.windows[0].scroll_top,
        20 - (rows - 1)
    );
    assert_eq!(ed.engine.state.cursor.line, 20);
}

// ── Ex ranges from Visual mode ────────────────────────────────────

#[tokio::test]
async fn visual_colon_delete_removes_the_selected_lines() {
    let mut ed = test_vim_editor("one\ntwo\nthree\n");
    type_keys(&mut ed, "vj").await;
    press_key(&mut ed, KeyCode::Char(':')).await;
    assert_eq!(ed.engine.state.command_buffer, "'<,'>");
    type_keys(&mut ed, "d").await;
    press_key(&mut ed, KeyCode::Enter).await;

    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "three\n");
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn visual_colon_substitute_applies_to_the_selection() {
    let mut ed = test_vim_editor("aa\nbb\naa\n");
    type_keys(&mut ed, "vj").await;
    press_key(&mut ed, KeyCode::Char(':')).await;
    type_keys(&mut ed, "s/a/x/g").await;
    press_key(&mut ed, KeyCode::Enter).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "xx\nbb\naa\n");
}

#[tokio::test]
async fn percent_range_indents_every_line() {
    let mut ed = test_vim_editor("one\ntwo\n");
    run_command(&mut ed, "%>").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "\tone\n\ttwo\n");
}

// ── Block-wise insert (Ctrl-v + I/A/c) ────────────────────────────

async fn ctrl_v(ed: &mut Editor) {
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
}

#[tokio::test]
async fn block_insert_replays_on_every_selected_line() {
    let mut ed = test_vim_editor("abc\ndef\nghi\n");
    ctrl_v(&mut ed).await;
    type_keys(&mut ed, "jjI").await;
    type_keys(&mut ed, "X").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "Xabc\nXdef\nXghi\n");
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn block_append_inserts_after_the_rightmost_column() {
    let mut ed = test_vim_editor("abc\ndef\n");
    ctrl_v(&mut ed).await;
    type_keys(&mut ed, "jA").await;
    type_keys(&mut ed, "X").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "aXbc\ndXef\n");
}

#[tokio::test]
async fn block_change_replays_on_every_selected_line() {
    let mut ed = test_vim_editor("abc\ndef\n");
    ctrl_v(&mut ed).await;
    type_keys(&mut ed, "jlc").await;
    type_keys(&mut ed, "Z").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "Zc\nZf\n");
}

#[tokio::test]
async fn block_insert_undoes_as_one_change() {
    let mut ed = test_vim_editor("abc\ndef\n");
    ctrl_v(&mut ed).await;
    type_keys(&mut ed, "jI").await;
    type_keys(&mut ed, "X").await;
    press_key(&mut ed, KeyCode::Esc).await;
    type_keys(&mut ed, "u").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "abc\ndef\n");
}

// ── Operator + h/l motions ───────────────────────────────────────

#[tokio::test]
async fn yank_char_with_l_motion_copies_one_char() {
    let mut ed = test_vim_editor("abcdef\n");
    type_keys(&mut ed, "ylP").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "aabcdef\n");
}

#[tokio::test]
async fn yank_char_honours_the_count() {
    let mut ed = test_vim_editor("abcdef\n");
    type_keys(&mut ed, "y2lP").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ababcdef\n");
}

#[tokio::test]
async fn delete_char_before_cursor_with_dh() {
    let mut ed = test_vim_editor("abcdef\n");
    type_keys(&mut ed, "llldh").await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "abdef\n");
    assert_eq!((line, col), (1, 2));
}

#[tokio::test]
async fn delete_char_under_cursor_with_dl() {
    let mut ed = test_vim_editor("abcdef\n");
    type_keys(&mut ed, "lld2l").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "abef\n");
}

#[tokio::test]
async fn change_char_with_cl_enters_insert_mode() {
    let mut ed = test_vim_editor("abcdef\n");
    type_keys(&mut ed, "llclZ").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "abZdef\n");
    assert_eq!(mode, Mode::Normal);
}

// ── Blockwise yank / put and X / D ───────────────────────────────

#[tokio::test]
async fn block_yank_and_put_copies_the_rectangle() {
    let mut ed = test_vim_editor("abcdef\nghijkl\nmnopqr\nstuvwx\n");
    type_keys(&mut ed, "ll").await;
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
    type_keys(&mut ed, "jlYjj").await;
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
    type_keys(&mut ed, "jlP").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "abcdef\nghijkl\nmncdqr\nstijwx\n");
}

#[tokio::test]
async fn block_put_pads_short_lines() {
    let mut ed = test_vim_editor("abcdef\nghijkl\nxy\n");
    type_keys(&mut ed, "ll").await;
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
    type_keys(&mut ed, "jlYjjp").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "abcdef\nghijkl\nxycd\n  ij");
}

#[tokio::test]
async fn block_x_deletes_the_rectangle() {
    let mut ed = test_vim_editor("abcdef\nghijkl\n");
    type_keys(&mut ed, "ll").await;
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
    type_keys(&mut ed, "jlX").await;

    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "abef\nghkl\n");
    assert_eq!(mode, Mode::Normal);
}

#[tokio::test]
async fn block_d_truncates_every_selected_line() {
    let mut ed = test_vim_editor("abcdef\nghijkl\n");
    type_keys(&mut ed, "ll").await;
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
    type_keys(&mut ed, "jlD").await;

    let (buf, _, _, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ab\ngh\n");
}

#[tokio::test]
async fn block_append_lands_on_the_block_left_edge() {
    let mut ed = test_vim_editor("abcdef\nghijkl\n");
    type_keys(&mut ed, "ll").await;
    ed.handle_key_for_test(KeyCode::Char('v'), KeyModifiers::CONTROL)
        .await;
    type_keys(&mut ed, "jAX").await;
    press_key(&mut ed, KeyCode::Esc).await;

    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "abcXdef\nghiXjkl\n");
    assert_eq!((line, col), (1, 2));
}

#[tokio::test]
async fn visual_put_with_an_empty_register_returns_to_normal() {
    let mut ed = test_vim_editor("abc\n");
    ed.engine.register.set('"', "");
    type_keys(&mut ed, "vlp").await;

    let (buf, _, _, mode) = ed.snapshot_for_test();
    assert_eq!(buf, "abc\n");
    assert_eq!(mode, Mode::Normal);
}

/// Send a short key string; `\u{1b}` is Escape. Every key is unmodified.
async fn send_keys(ed: &mut Editor, keys: &str) {
    for c in keys.chars() {
        let key = if c == '\u{1b}' {
            KeyCode::Esc
        } else {
            KeyCode::Char(c)
        };
        ed.handle_key_for_test(key, KeyModifiers::NONE).await;
    }
}

#[tokio::test]
async fn undo_after_append_restores_the_original_buffer() {
    let mut ed = test_vim_editor("abc\n");
    send_keys(&mut ed, "Axy\u{1b}u").await;
    assert_eq!(ed.snapshot_for_test().0, "abc\n");
}

#[tokio::test]
async fn undo_after_appending_at_end_restores_the_original_buffer() {
    let mut ed = test_vim_editor("abc\ndef\n");
    send_keys(&mut ed, "oX\u{1b}u").await;
    assert_eq!(ed.snapshot_for_test().0, "abc\ndef\n");
}

#[tokio::test]
async fn undo_after_open_line_above_restores_the_original_buffer() {
    let mut ed = test_vim_editor("abc\ndef\n");
    send_keys(&mut ed, "OX\u{1b}u").await;
    assert_eq!(ed.snapshot_for_test().0, "abc\ndef\n");
}

#[tokio::test]
async fn undo_after_change_word_restores_the_original_buffer() {
    let mut ed = test_vim_editor("abc def\n");
    send_keys(&mut ed, "wcwX\u{1b}u").await;
    assert_eq!(ed.snapshot_for_test().0, "abc def\n");
}

#[tokio::test]
async fn undo_after_substitute_restores_the_original_buffer() {
    let mut ed = test_vim_editor("abc\n");
    send_keys(&mut ed, "3sX\u{1b}u").await;
    assert_eq!(ed.snapshot_for_test().0, "abc\n");
}

#[tokio::test]
async fn capital_d_keeps_the_line_break() {
    let mut ed = test_vim_editor("abc\ndef\n");
    send_keys(&mut ed, "D").await;
    assert_eq!(ed.snapshot_for_test().0, "\ndef\n");
}

#[tokio::test]
async fn capital_d_with_count_in_column_zero_is_linewise() {
    let mut ed = test_vim_editor("abc\ndef\nghi\n");
    send_keys(&mut ed, "2D").await;
    assert_eq!(ed.snapshot_for_test().0, "ghi\n");
}

#[tokio::test]
async fn x_stops_at_the_end_of_the_line() {
    let mut ed = test_vim_editor("abc\n");
    send_keys(&mut ed, "$3x").await;
    let (buf, line, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "ab\n");
    assert_eq!((line, col), (1, 1));
}

#[tokio::test]
async fn x_on_an_empty_line_does_not_join_lines() {
    let mut ed = test_vim_editor("abc\n\ndef\n");
    send_keys(&mut ed, "jx").await;
    assert_eq!(ed.snapshot_for_test().0, "abc\n\ndef\n");
}

#[tokio::test]
async fn gg_honours_a_count_and_lands_on_first_non_blank() {
    let mut ed = test_vim_editor("  abc\n  def\n");
    send_keys(&mut ed, "2gg").await;
    let (_, line, col, _) = ed.snapshot_for_test();
    assert_eq!((line, col), (2, 2));
}

#[tokio::test]
async fn space_moves_right_like_l() {
    let mut ed = test_vim_editor("abc def\n");
    send_keys(&mut ed, "0 x").await;
    assert_eq!(ed.snapshot_for_test().0, "ac def\n");
}

#[tokio::test]
async fn cw_inserts_at_the_end_of_the_line() {
    let mut ed = test_vim_editor("abc def\n");
    send_keys(&mut ed, "wcwX\u{1b}").await;
    let (buf, _, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "abc X\n");
    assert_eq!(col, 4);
}

#[tokio::test]
async fn inner_bracket_object_works_from_before_the_bracket() {
    let mut ed = test_vim_editor("foo(bar)\n");
    send_keys(&mut ed, "di(").await;
    let (buf, _, col, _) = ed.snapshot_for_test();
    assert_eq!(buf, "foo()\n");
    assert_eq!(col, 4);
}

#[tokio::test]
async fn inner_bracket_object_picks_the_innermost_pair() {
    let mut ed = test_vim_editor("a(b(c)d)e\n");
    send_keys(&mut ed, "4ldi(").await;
    assert_eq!(ed.snapshot_for_test().0, "a(b()d)e\n");
}

#[tokio::test]
async fn inner_bracket_object_across_lines_keeps_the_newlines() {
    let mut ed = test_vim_editor("foo(\nbar\n)\n");
    send_keys(&mut ed, "di(").await;
    assert_eq!(ed.snapshot_for_test().0, "foo(\n)\n");
}

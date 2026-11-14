use super::*;
use crate::types::{ConfirmAction, Keymap};
use crossterm::event::{KeyCode, KeyModifiers};

/// Make the buffer dirty by performing a real edit, so tests exercise the
/// same path as user input (the core crate no longer exposes a
/// test-only dirty setter).
fn mark_dirty(editor: &mut Editor) {
    editor.engine.buffer.insert_char(1, 0, 'x');
}

fn test_editor(keymap: Keymap) -> Editor {
    let terminal = Terminal::new().expect("terminal");
    let buffer = TextBuffer::new();
    let plugin_manager = PluginManager::new();
    let state = EditorState {
        mode: Mode::Normal,
        keymap,
        cursor: crate::types::Position { line: 1, col: 0 },
        file_path: None,
        dirty: false,
        command_buffer: String::new(),
        command_cursor_pos: 0,
        last_message: None,
        visual_start: None,
        visual_type: None,
        last_visual_start: None,
        last_visual_end: None,
        last_visual_type: None,
        marks: crate::types::Marks::new(),
        macros: crate::types::Macros::new(),
        confirmation_prompt: None,
        show_line_numbers: true,
        show_relative_numbers: false,
        hlsearch: true,
        search_pattern: None,
        wrap: true,
        mark: None,
        region_active: false,
        tabstop: 8,
        shiftwidth: 4,
        expandtab: false,
        scrolloff: 0,
        message_history: VecDeque::new(),
        show_dashboard: false,
        incsearch_match: None,
        command_hint: None,
    };

    Editor {
        engine: Engine::new(buffer, plugin_manager, state),
        terminal,
        renderer: Renderer::new(),
        keymap_handler: create_keymap(keymap),
        running: true,
        needs_render: false,
        config: Config::default(),
        last_scroll_mod_count: 0,
        last_scroll_cursor_line: 0,
        current_keymap: keymap,
        saved_keymap: None,
        command_history: Vec::new(),
        search_history: Vec::new(),
        history_index: None,
        history_draft: String::new(),
        completion_session: None,
        search_preview: None,
        viewport_override: false,
        one_shot_normal: false,
        block_insert: None,
    }
}

#[test]
fn dirty_quit_confirmation_message_matches_expected() {
    let mut editor = test_editor(Keymap::Vim);
    mark_dirty(&mut editor);

    editor.handle_quit();

    let prompt = editor
        .engine
        .state
        .confirmation_prompt
        .as_ref()
        .expect("prompt should exist");
    assert_eq!(
        prompt.message,
        "No write since last change. Quit anyway? (y/n Enter/Esc: yes, n: no)"
    );
    assert!(matches!(prompt.action, ConfirmAction::Quit));
}

#[test]
fn emacs_quit_uses_same_dirty_confirmation_flow_as_vim() {
    let mut editor = test_editor(Keymap::Emacs);
    mark_dirty(&mut editor);

    editor.handle_quit();

    assert!(editor.engine.state.has_confirmation());
    let prompt = editor.engine.state.confirmation_prompt.as_ref().unwrap();
    assert!(matches!(prompt.action, ConfirmAction::Quit));
}

#[tokio::test]
async fn emacs_ctrl_x_ctrl_c_triggers_same_quit_path() {
    let mut editor = test_editor(Keymap::Emacs);
    mark_dirty(&mut editor);

    editor
        .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
        .await;

    assert!(editor.engine.state.has_confirmation());
    assert!(editor.running);
}

#[tokio::test]
async fn emacs_dirty_quit_confirmation_accept_exits_editor() {
    let mut editor = test_editor(Keymap::Emacs);
    mark_dirty(&mut editor);

    editor
        .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
        .await;
    assert!(editor.engine.state.has_confirmation());
    assert!(editor.running);

    editor
        .handle_key_for_test(KeyCode::Char('y'), KeyModifiers::NONE)
        .await;

    assert!(!editor.engine.state.has_confirmation());
    assert!(!editor.running);
}

#[tokio::test]
async fn emacs_clean_quit_exits_immediately() {
    let mut editor = test_editor(Keymap::Emacs);
    // Buffer starts clean.

    editor
        .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::CONTROL)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('c'), KeyModifiers::CONTROL)
        .await;

    assert!(!editor.engine.state.has_confirmation());
    assert!(!editor.running);
}

// ── Emacs Mark/Region tests ─────────────────────────────────

#[test]
fn emacs_mark_defaults_to_none() {
    let editor = test_editor(Keymap::Emacs);
    assert!(editor.engine.state.mark.is_none());
    assert!(!editor.engine.state.region_active);
}

#[tokio::test]
async fn emacs_c_space_sets_mark() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.set_buffer_for_test("hello world\n");

    editor
        .handle_key_for_test(KeyCode::Char(' '), KeyModifiers::CONTROL)
        .await;

    assert_eq!(editor.engine.state.mark, Some(editor.engine.state.cursor));
    assert!(editor.engine.state.region_active);
}

#[test]
fn emacs_has_active_region_true_when_mark_differs_from_cursor() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.set_buffer_for_test("hello world\n");

    editor.engine.state.mark = Some(crate::types::Position { line: 1, col: 0 });
    editor.engine.state.region_active = true;
    editor.engine.state.cursor.col = 5;

    assert!(editor.has_active_region());
}

// Note: has_active_region is defined in editor/emacs_ops.rs
// We test it through the editor instance.

#[tokio::test]
async fn emacs_kill_region_pushes_to_kill_ring() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.set_buffer_for_test("hello world\n");

    editor.engine.state.mark = Some(crate::types::Position { line: 1, col: 0 });
    editor.engine.state.region_active = true;
    editor.engine.state.cursor.col = 5;

    editor.kill_region();

    assert_eq!(editor.kill_ring().yank(), Some("hello"));
}

#[tokio::test]
async fn emacs_deactivate_region_clears_active_flag() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.engine.state.mark = Some(crate::types::Position { line: 1, col: 0 });
    editor.engine.state.region_active = true;

    editor.deactivate_region();

    assert!(!editor.engine.state.region_active);
    assert!(editor.engine.state.mark.is_some());
}

#[tokio::test]
async fn vim_0_moves_to_column_zero() {
    let mut editor = test_editor(Keymap::Vim);
    editor.set_buffer_for_test("hello\n");
    editor.engine.state.cursor.col = 3;
    editor.handle_normal(KeyCode::Char('0')).await;
    assert_eq!(editor.engine.state.cursor.col, 0);
}

#[tokio::test]
async fn vim_dollar_moves_to_end_of_line() {
    let mut editor = test_editor(Keymap::Vim);
    editor.set_buffer_for_test("hello\n");
    editor.engine.state.cursor.col = 0;
    editor.handle_normal(KeyCode::Char('$')).await;
    assert_eq!(editor.engine.state.cursor.col, 4);
}

#[tokio::test]
async fn vim_caret_moves_to_first_non_blank() {
    let mut editor = test_editor(Keymap::Vim);
    editor.set_buffer_for_test("  hello\n");
    editor.engine.state.cursor.col = 6;
    editor.handle_normal(KeyCode::Char('^')).await;
    assert_eq!(editor.engine.state.cursor.col, 2);
}

#[tokio::test]
async fn vim_d_capital_works() {
    let mut editor = test_editor(Keymap::Vim);
    editor.set_buffer_for_test("hello world\n");
    editor.engine.state.cursor.col = 5;
    editor.handle_normal(KeyCode::Char('D')).await;
    let text = editor.engine.buffer.get_line(1);
    assert!(text.starts_with("hello"));
}

#[tokio::test]
async fn vim_s_capital_substitutes_line() {
    let mut editor = test_editor(Keymap::Vim);
    editor.set_buffer_for_test("hello\n");
    editor.handle_normal(KeyCode::Char('S')).await;
    assert_eq!(editor.engine.state.mode, Mode::Insert);
    let text = editor.engine.buffer.get_line(1);
    assert_eq!(text, "\n");
}

#[tokio::test]
async fn vim_question_mark_enters_command_mode() {
    let mut editor = test_editor(Keymap::Vim);
    editor.handle_normal(KeyCode::Char('?')).await;
    assert_eq!(editor.engine.state.mode, Mode::Command);
    assert_eq!(editor.engine.state.command_buffer, "?");
}

#[test]
fn vim_ctrl_r_redo_does_not_panic() {
    let mut editor = test_editor(Keymap::Vim);
    editor.redo();
}

// ── Emacs new keybindings tests ────────────────────────────

#[tokio::test]
async fn emacs_copy_region_copies_without_deleting() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.set_buffer_for_test("hello world\n");
    editor.engine.state.mark = Some(crate::types::Position { line: 1, col: 0 });
    editor.engine.state.region_active = true;
    editor.engine.state.cursor.col = 5;

    editor.copy_region();

    let text = editor.engine.buffer.get_line(1);
    assert_eq!(text, "hello world\n");
    assert_eq!(editor.kill_ring().yank(), Some("hello"));
    assert!(!editor.engine.state.region_active);
}

#[tokio::test]
async fn emacs_yank_from_kill_ring_inserts_text() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.set_buffer_for_test("hello world\n");
    editor.kill_ring_mut().push("xyz");
    editor.engine.state.cursor.col = 5;

    editor.yank_from_kill_ring();

    let text = editor.engine.buffer.get_line(1);
    assert_eq!(text, "helloxyz world\n");
}

#[tokio::test]
async fn emacs_m_f_moves_word_forward() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.set_buffer_for_test("hello world foo\n");
    editor.engine.state.cursor.col = 0;
    editor.handle_normal(KeyCode::Char('w')).await;
    // `forward-word` lands on the start of the next word, not on the
    // blank that separates them.
    assert_eq!(editor.engine.state.cursor.col, 6);
}

#[tokio::test]
async fn emacs_m_b_moves_word_backward() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.set_buffer_for_test("hello world\n");
    editor.engine.state.cursor.col = 8;
    editor.handle_normal(KeyCode::Char('b')).await;
    assert_eq!(editor.engine.state.cursor.col, 6);
}

#[test]
fn emacs_ctrl_x_u_redo_does_not_panic() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.redo();
}

#[test]
fn emacs_ctrl_x_ctrl_slash_redo_does_not_panic() {
    let mut editor = test_editor(Keymap::Emacs);
    editor.redo();
}

// ── Regression tests for byte/char index and text-object bugs ─────────

#[test]
fn find_char_does_not_panic_on_empty_line() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 0 };
    assert!(!editor.find_char('a', false, true));
    assert!(!editor.find_char('a', false, false));
}

#[test]
fn find_char_past_last_char_is_a_no_op() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("abc\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 2 };
    assert!(!editor.find_char('a', false, true));
}

#[test]
fn delete_char_handles_multibyte_lines() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("あいう\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 0 };
    editor.delete_char('"');
    assert_eq!(editor.engine.buffer.get_line(1), "いう\n");
    assert_eq!(editor.engine.register.get('"'), "あ");
}

#[tokio::test]
async fn replace_inline_char_handles_multibyte_lines() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("あいう\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 1 };
    editor
        .handle_key_for_test(KeyCode::Char('r'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('x'), KeyModifiers::NONE)
        .await;
    assert_eq!(editor.engine.buffer.get_line(1), "あxう\n");
}

#[test]
fn page_down_clamps_column_by_character_count() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("あいう\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 10 };
    editor.page_down();
    assert_eq!(editor.engine.state.cursor.col, 2);
    editor.page_up();
    assert_eq!(editor.engine.state.cursor.col, 2);
}

#[tokio::test]
async fn da_w_at_end_of_line_does_not_absorb_newline() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("foo\nbaz\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 1 };
    editor
        .handle_text_object(KeyCode::Char('W'), '"', false)
        .await;
    assert_eq!(editor.engine.buffer.get_line(1), "\n");
    assert_eq!(editor.engine.buffer.get_line(2), "baz\n");
    assert_eq!(editor.engine.buffer.content_line_count(), 2);
}

#[tokio::test]
async fn visual_aw_at_end_of_line_stops_before_newline() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("foo\nbaz\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 1 };
    editor
        .handle_key_for_test(KeyCode::Char('v'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('a'), KeyModifiers::NONE)
        .await;
    editor
        .handle_key_for_test(KeyCode::Char('w'), KeyModifiers::NONE)
        .await;
    assert_eq!(editor.engine.state.visual_start.unwrap().col, 0);
    assert_eq!(editor.engine.state.cursor.col, 2);
}

#[tokio::test]
async fn quote_text_object_works_when_cursor_is_on_opening_quote() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("let x = \"abc\";\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 8 };
    editor
        .handle_text_object(KeyCode::Char('"'), '"', false)
        .await;
    assert_eq!(editor.engine.buffer.get_line(1), "let x = ;\n");
}

#[test]
fn increment_number_treats_09_as_decimal() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("09\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 0 };
    editor.increment_number(1, 1);
    assert_eq!(editor.engine.buffer.get_line(1), "10\n");
}

#[test]
fn increment_number_keeps_real_octal() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("010\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 0 };
    editor.increment_number(1, 1);
    assert_eq!(editor.engine.buffer.get_line(1), "011\n");
}

#[tokio::test]
async fn dollar_with_count_does_not_reach_phantom_line() {
    let mut editor = test_editor(Keymap::Vim);
    editor.engine.buffer = TextBuffer::with_text("one\ntwo\n");
    editor.engine.state.cursor = crate::types::Position { line: 1, col: 0 };
    editor.engine.operator_state.count = Some(5);
    editor.handle_normal(KeyCode::Char('$')).await;
    assert_eq!(editor.engine.state.cursor.line, 2);
}

#[test]
fn buffer_open_leaves_the_buffer_clean() {
    let mut editor = test_editor(Keymap::Vim);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("ijevim-open-{stamp}.txt"));
    std::fs::write(&path, "hello\n").unwrap();

    editor
        .engine
        .buffer_open(path.to_str().unwrap())
        .expect("open buffer");
    assert!(
        !editor.engine.buffer.is_dirty(),
        "loading a file must not mark the buffer modified"
    );
    assert_eq!(editor.engine.buffer.get_line(1), "hello\n");

    std::fs::remove_file(&path).ok();
}

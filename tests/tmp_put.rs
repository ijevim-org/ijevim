use crossterm::event::{KeyCode, KeyModifiers};
use ijevim::editor::Editor;
use ijevim::types::Keymap;

#[tokio::test]
async fn debug_visual_put() {
    let mut ed = Editor::new_headless_for_test(Keymap::Vim).unwrap();
    ed.set_buffer_for_test("abcdef\nghijkl\nmnopqr\n");
    for c in ['y', 'l', 'j', 'j', 'v', 'p'] {
        ed.handle_key_for_test(KeyCode::Char(c), KeyModifiers::NONE).await;
        let (buf, line, col, mode) = ed.snapshot_for_test();
        println!("{c}: {buf:?} {line}:{col} {mode:?} reg={:?}", ed.engine.register.get('"'));
    }
}

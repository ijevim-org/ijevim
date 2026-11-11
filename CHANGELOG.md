# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- README: generated dependency tables (runtime / optional / development, with
  versions, licensing and transitive crate counts). Regenerate with
  `scripts/deps-table.sh --write`; CI verifies it via `--check`.
- README: added CLI usage examples (`nestvim vim`, `nestvim emacs`, `nestvim <file>`).
- README: added link to keybinding specification document.
- README: added limitations section for currently unimplemented features.
- docs: introduced `docs/keybindings.md` documenting currently implemented keybindings.
- Added `CHANGELOG.md` for release-by-release change tracking.
- `crate::types::Result<T>` alias for `Result<T, IjevimError>`.
- `TextBuffer::is_dirty()` / `mark_clean()` / `file_path()` / `set_file_path()` accessors.
- Emacs: kill ring append for consecutive kill commands, `M-y` yank-pop, mark and
  region operations, `C-o` open-line, `C-d`/`C-k` line joining, `C-t` transpose.
- Emacs: `C-s` / `C-r` incremental search with live preview, and `C-g` cancel.
- Emacs: `C-x u` / `C-x C-/` undo, `C-M-/` redo, `C-x h` mark-whole-buffer,
  `C-x C-x` exchange point and mark, `M-x` command line.
- Emacs: `CursorState::yank_text` and `UndoManager::push_insert_coalescing` in
  `ijevim-core` (consecutive self-inserts form one undo unit).
- Tests: `tests/emacs_flow.rs` (18 regression tests) and the opt-in differential
  harness `tests/emacs_probe.rs` + `scripts/emacs-diff.sh` against `emacs -Q`.

### Changed

- `Editor::new` and `Editor::new_headless_for_test` now return `Result<Self, IjevimError>`
  instead of `Result<Self, Box<dyn std::error::Error>>`. `main` follows suit.
- `TextBuffer::dirty` and `TextBuffer::file_path` are no longer public fields.
  All access goes through the new accessors.
- Dirty state tracking is now centralised: `Editor::on_buffer_modified` is the
  only place that mutates `EditorState::dirty` for buffer mutations, mirroring
  `TextBuffer::is_dirty()`. All emacs operations funnel through it.
- `set_buffer_for_test` now seeds the buffer via `TextBuffer::with_text` instead
  of one character at a time.
- Command-mode prompts (search, `:`, `M-x`) are now routed centrally in
  `Editor::handle_key` instead of inside each keymap, so they behave the same in
  Vim and Emacs modes.
- Emacs: `C-v` / `M-v` now move by a page (previously half a page), `C-o` opens
  a line (previously saved the file), `C-x h` marks the whole buffer (previously
  beginning-of-line), `C-x d` is undefined (previously end-of-line), and
  `C-x u` / `C-x C-/` undo (previously redo).
- Emacs: inserts, deletes, kills, yanks and transposes are now undoable.

### Fixed

- `TextBuffer::insert` "extend past last line" branch no longer silently
  accepts gaps larger than one line; the intent is now expressed in code.
- Emacs: search prompts no longer edit the buffer; `C-s` / `C-r` move point to
  the match and `C-g` cancels the search.
- Emacs: an undefined `C-x` sequence is consumed with a message instead of
  leaking its second key to the buffer.
- Undo of a line split at column 0 now removes the inserted newline.


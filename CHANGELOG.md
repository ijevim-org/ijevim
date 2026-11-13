# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Review mode (`:review`): browse `git diff` in the TUI with a shared row model,
  unified/side-by-side rendering, hunk navigation, live file preview, and
  reload (`r`).
- Review mode staging: `Enter` stages/unstages a hunk, `S`/`U` stage/unstage a
  file, and `X` discards a file's changes after confirmation. Staged state is
  derived from `git diff --cached` instead of an in-memory flag.
- Review mode comments: annotations in `<git-dir>/ijevim/review/annotations.json`
  (JSON v2 with `end_line`, legacy `.review/annotations.json` read fallback),
  line and visual-range comments, annotation browser with jump, resolve toggle
  and delete.
- Review mode search and filtering: diff search with smart case and
  `n`/`N` navigation, plus case-insensitive file-list filtering with `/`.
- Review mode arguments: `:review --staged` / `--cached`, `:review <rev>`,
  `:review <rev1>..<rev2>`, and `:review -- <path>`.
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
- Emacs files and buffers: `C-x C-f` find-file, `C-x C-w` write-file, `C-x b`
  switch-to-buffer (including the new `:b <name>` command), `C-x k` kill-buffer,
  and `C-x d` dired.
- Emacs windows: `C-x 2` split-below, `C-x 3` split-right, `C-x o` / `C-x O`
  other-window (forward/backward), `C-x 0` delete-window, and `C-x 1`
  delete-other-windows.
- Emacs numeric arguments: `C-u`, `M-<digit>` and `M--`, applied to motions,
  deletion, kills, yanks, self-insert, paging and transpose.
- Emacs: `CursorState::yank_text` and `UndoManager::push_insert_coalescing` in
  `ijevim-core` (consecutive self-inserts form one undo unit).
- Emacs navigation: `M-<` / `M->` buffer edges, `M-m` back-to-indentation,
  `M-g g` goto-line (new `goto <n>` command).
- Emacs case commands: `M-u` upcase-word, `M-l` downcase-word, `M-c`
  capitalize-word, and `C-x C-u` / `C-x C-l` upcase/downcase-region.
- Emacs word kills: `M-d` kill-word and `M-DEL` / `M-Delete` backward-kill-word,
  including kill-ring append and undo.
- Emacs: `M-n` / `M-p` repeat the current search forward/backward (numeric
  arguments supported).
- Emacs dired: `Enter` visits the entry, `g` reverts the listing, `^` goes to
  the parent directory, and `q` buries the listing.
- Tests: `tests/emacs_flow.rs` (44 regression tests) and the opt-in differential
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
- Emacs `M-d` now kills from point to the end of the current/next word across
  line boundaries via `Editor::forward_word_bounds`, instead of stopping at the
  end of the line. `M-DEL` joins the previous line when point is at column 0.
- Command-mode prompts (search, `:`, `M-x`) are now routed centrally in
  `Editor::handle_key` instead of inside each keymap, so they behave the same in
  Vim and Emacs modes.
- Emacs: `C-v` / `M-v` now move by a page (previously half a page), `C-o` opens
  a line (previously saved the file), `C-x h` marks the whole buffer (previously
  beginning-of-line), `C-x d` opens dired (previously end-of-line), and
  `C-x u` / `C-x C-/` undo (previously redo).
- Emacs: inserts, deletes, kills, yanks and transposes are now undoable.
- Emacs: `C-k` at the beginning of a line now kills the whole line including
  its newline, matching Emacs (previously it left an empty line behind).

### Fixed

- `TextBuffer::insert` "extend past last line" branch no longer silently
  accepts gaps larger than one line; the intent is now expressed in code.
- Emacs: search prompts no longer edit the buffer; `C-s` / `C-r` move point to
  the match and `C-g` cancels the search.
- Emacs: an undefined `C-x` sequence is consumed with a message instead of
  leaking its second key to the buffer.
- Undo of a line split at column 0 now removes the inserted newline.


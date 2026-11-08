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

### Fixed

- `TextBuffer::insert` "extend past last line" branch no longer silently
  accepts gaps larger than one line; the intent is now expressed in code.


//! ijevim — a minimal Vim-like TUI editor.
//!
//! The workspace is split into focused crates:
//!
//! - `ijevim-core` — buffers, undo, registers, state, config, git helpers
//! - `ijevim-highlight` — syntax highlighting (optional `syntax` feature)
//! - `ijevim-plugin` — per-language plugin loaders
//! - `ijevim-tui` — screen buffer, terminal, renderer, review UI
//! - `ijevim` (this crate) — editor/engine/keymaps and the binary
//!
//! The re-exports below keep the historical `ijevim::<module>` paths stable
//! for downstream users, integration tests, and benchmarks.

pub mod editor;
pub(crate) mod engine;
pub mod keymap;

// Core model.
pub use ijevim_core::{buffer, config, git, register, state, types, undo};

// Supporting crates.
pub use ijevim_highlight as highlight;
pub use ijevim_plugin as plugin;
pub use ijevim_tui::{renderer, review, screen, terminal};

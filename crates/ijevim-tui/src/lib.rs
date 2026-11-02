//! Terminal UI layer for ijevim.
//!
//! Owns everything terminal-facing: the cell screen buffer, crossterm
//! terminal wrapper, renderer, welcome dashboard, and review-mode rendering.
//! The UI layer is stateless with respect to editing; it receives an
//! `EditorState` from the application crate and paints it.

pub mod dashboard;
pub mod renderer;
pub mod review;
pub mod screen;
pub mod terminal;

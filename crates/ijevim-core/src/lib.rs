//! Core editor model for ijevim.
//!
//! This crate contains the UI-independent pieces: the text buffer, undo
//! history, registers, cursor/operator state, configuration, and git helpers.
//! Nothing here depends on a terminal, so it can be reused by headless
//! front-ends and tests.

pub mod buffer;
pub mod config;
pub mod git;
pub mod register;
pub mod state;
pub mod types;
pub mod undo;

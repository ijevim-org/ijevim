//! Syntax highlighting for ijevim.
//!
//! Enabled via the `syntax` feature. When the feature is off, the crate
//! exposes a stub [`Style`] type so downstream call-sites can keep using
//! `Vec<Vec<(Style, String)>>` highlight segments without churn.

#[cfg(feature = "syntax")]
pub mod highlighter;

#[cfg(feature = "syntax")]
pub use highlighter::Highlighter;

#[cfg(feature = "syntax")]
pub use syntect::highlighting::Style;

/// Stub style used when the `syntax` feature is disabled.
///
/// The highlight pipeline stays in the call graph but `Highlighter` is not
/// compiled in, so every segment uses this no-op type. Renderers fall back
/// to [`CellStyle::default`] for every character.
#[cfg(not(feature = "syntax"))]
#[derive(Debug, Clone, Default)]
pub struct Style;

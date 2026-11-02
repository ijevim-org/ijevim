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

/// Renderer-agnostic colour and font data extracted from a highlight style.
///
/// Downstream crates (e.g. `ijevim-tui`) must not depend on `syntect`
/// directly. They convert [`Style`] through this type instead, which keeps
/// the syntax-highlighting dependency contained in this crate.
#[cfg(feature = "syntax")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StyleColors {
    /// Foreground RGB, or `None` when the style is fully transparent.
    pub fg: Option<(u8, u8, u8)>,
    /// Background RGB, or `None` when the style is fully transparent.
    pub bg: Option<(u8, u8, u8)>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[cfg(feature = "syntax")]
impl StyleColors {
    pub fn from_style(style: &Style) -> Self {
        use syntect::highlighting::FontStyle;

        fn rgb(c: syntect::highlighting::Color) -> Option<(u8, u8, u8)> {
            if c.a == 0 {
                None
            } else {
                Some((c.r, c.g, c.b))
            }
        }

        Self {
            fg: rgb(style.foreground),
            bg: rgb(style.background),
            bold: style.font_style.contains(FontStyle::BOLD),
            italic: style.font_style.contains(FontStyle::ITALIC),
            underline: style.font_style.contains(FontStyle::UNDERLINE),
        }
    }
}

/// Stub style used when the `syntax` feature is disabled.
///
/// The highlight pipeline stays in the call graph but `Highlighter` is not
/// compiled in, so every segment uses this no-op type. Renderers fall back
/// to [`CellStyle::default`] for every character.
#[cfg(not(feature = "syntax"))]
#[derive(Debug, Clone, Default)]
pub struct Style;

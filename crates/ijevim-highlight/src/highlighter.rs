use std::path::PathBuf;
use std::sync::Arc;
use syntect::{
    easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxReference, parsing::SyntaxSet,
    util::LinesWithEndings,
};

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    /// Cache the last syntax reference and file path to avoid repeated lookups.
    cached_syntax_path: Option<PathBuf>,
    cached_syntax: Option<std::sync::Arc<SyntaxReference>>,
}

impl Highlighter {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        Self {
            syntax_set,
            theme_set,
            cached_syntax_path: None,
            cached_syntax: None,
        }
    }

    pub fn update(
        &mut self,
        text: &str,
        file_path: Option<&std::path::Path>,
    ) -> Vec<Vec<(syntect::highlighting::Style, String)>> {
        let theme = &self.theme_set.themes["base16-ocean.dark"];

        // Use cached syntax if the file path hasn't changed.
        let syntax = if let Some(path) = file_path {
            if self.cached_syntax_path.as_deref() == Some(path) {
                self.cached_syntax
                    .as_deref()
                    .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
            } else {
                let s = file_path
                    .and_then(|p| self.syntax_set.find_syntax_for_file(p).ok().flatten())
                    .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
                self.cached_syntax_path = Some(path.to_path_buf());
                self.cached_syntax = Some(Arc::new(s.clone()));
                self.cached_syntax.as_deref().unwrap()
            }
        } else {
            self.syntax_set.find_syntax_plain_text()
        };

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();

        for line in LinesWithEndings::from(text) {
            let ranges: Vec<(syntect::highlighting::Style, &str)> = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();
            let styled: Vec<(syntect::highlighting::Style, String)> = ranges
                .into_iter()
                .map(|(style, s)| (style, s.to_string()))
                .collect();
            result.push(styled);
        }

        result
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

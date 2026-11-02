use std::path::PathBuf;

/// Editor configuration loaded from `~/.config/ijevim/config.toml`.
#[derive(Debug, Clone)]
pub struct Config {
    /// Default keymap: "vim" or "emacs".
    pub keymap: String,
    /// Show line numbers by default.
    pub show_line_numbers: bool,
    /// Show relative line numbers by default.
    pub show_relative_numbers: bool,
    /// Enable soft line wrapping.
    pub wrap: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keymap: "vim".to_string(),
            show_line_numbers: true,
            show_relative_numbers: false,
            wrap: true,
        }
    }
}

impl Config {
    /// Load config from the default path (`~/.config/ijevim/config.toml`).
    /// Returns `Ok(Config::default())` if the file does not exist or cannot be parsed.
    pub fn load() -> Self {
        let path = config_path();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        match content.parse::<toml::Table>() {
            Ok(table) => Self::from_table(&table, &path),
            Err(e) => {
                eprintln!(
                    "[config] Ignoring malformed config: {} ({})",
                    path.display(),
                    e
                );
                Self::default()
            }
        }
    }

    /// Build a `Config` from a parsed TOML table, falling back to defaults for
    /// any missing or mistyped keys.
    fn from_table(table: &toml::Table, path: &std::path::Path) -> Self {
        let mut out = Self::default();

        if let Some(v) = table.get("keymap") {
            match v.as_str() {
                Some("vim") | Some("emacs") => out.keymap = v.as_str().unwrap().to_string(),
                Some(other) => eprintln!(
                    "[config] Ignoring unknown keymap '{}' in {}; expected 'vim' or 'emacs'",
                    other,
                    path.display()
                ),
                None => eprintln!(
                    "[config] Ignoring 'keymap' in {}: not a string",
                    path.display()
                ),
            }
        }

        if let Some(v) = table.get("show_line_numbers") {
            match v.as_bool() {
                Some(b) => out.show_line_numbers = b,
                None => eprintln!(
                    "[config] Ignoring 'show_line_numbers' in {}: not a bool",
                    path.display()
                ),
            }
        }

        if let Some(v) = table.get("show_relative_numbers") {
            match v.as_bool() {
                Some(b) => out.show_relative_numbers = b,
                None => eprintln!(
                    "[config] Ignoring 'show_relative_numbers' in {}: not a bool",
                    path.display()
                ),
            }
        }

        if let Some(v) = table.get("wrap") {
            match v.as_bool() {
                Some(b) => out.wrap = b,
                None => eprintln!("[config] Ignoring 'wrap' in {}: not a bool", path.display()),
            }
        }

        out
    }
}

/// Return the config file path: `~/.config/ijevim/config.toml`
fn config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let mut path = PathBuf::from(home);
    path.push(".config");
    path.push("ijevim");
    path.push("config.toml");
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn defaults_when_empty_table() {
        let table: toml::Table = toml::Table::new();
        let cfg = Config::from_table(&table, Path::new("/dev/null"));
        assert_eq!(cfg.keymap, "vim");
        assert!(cfg.show_line_numbers);
        assert!(cfg.wrap);
    }

    #[test]
    fn parses_full_config() {
        let table: toml::Table = toml::from_str(
            r#"
                keymap = "emacs"
                show_line_numbers = false
                wrap = false
            "#,
        )
        .unwrap();
        let cfg = Config::from_table(&table, Path::new("/dev/null"));
        assert_eq!(cfg.keymap, "emacs");
        assert!(!cfg.show_line_numbers);
        assert!(!cfg.wrap);
    }

    #[test]
    fn falls_back_to_defaults_for_missing_keys() {
        let table: toml::Table = toml::from_str(r#"keymap = "emacs""#).unwrap();
        let cfg = Config::from_table(&table, Path::new("/dev/null"));
        assert_eq!(cfg.keymap, "emacs");
        assert!(cfg.show_line_numbers);
        assert!(cfg.wrap);
    }

    #[test]
    fn ignores_wrong_type_silently_falls_back() {
        let table: toml::Table = toml::from_str(
            r#"
                keymap = 42
                show_line_numbers = "yes"
                wrap = [true]
            "#,
        )
        .unwrap();
        let cfg = Config::from_table(&table, Path::new("/dev/null"));
        assert_eq!(cfg.keymap, "vim");
        assert!(cfg.show_line_numbers);
        assert!(cfg.wrap);
    }
}

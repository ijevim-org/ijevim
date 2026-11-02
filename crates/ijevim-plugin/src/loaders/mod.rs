#[cfg(feature = "plugin-js")]
pub mod javascript;
#[cfg(feature = "plugin-lisp")]
pub mod lisp;
#[cfg(feature = "plugin-lua")]
pub mod lua;
#[cfg(feature = "plugin-nix")]
pub mod nix;
#[cfg(feature = "plugin-rust")]
pub mod rust;

use crate::{Plugin, PluginApi};
use std::path::Path;
use std::rc::Rc;

pub const API_VERSION: u32 = 1;

#[derive(Debug)]
pub enum LoaderError {
    Io(String),
    Parse(String),
    UnsupportedLanguage(String),
    ApiVersionMismatch { expected: u32, actual: u32 },
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::Io(s) => write!(f, "IO error: {}", s),
            LoaderError::Parse(s) => write!(f, "Parse error: {}", s),
            LoaderError::UnsupportedLanguage(s) => write!(f, "Unsupported language: {}", s),
            LoaderError::ApiVersionMismatch { expected, actual } => {
                write!(
                    f,
                    "API version mismatch: expected v{}, got v{}",
                    expected, actual
                )
            }
        }
    }
}

impl std::error::Error for LoaderError {}

pub trait Loader: Send + Sync {
    fn supported_extensions(&self) -> &[&str];

    fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, LoaderError>;
}

pub struct LoaderRegistry {
    loaders: Vec<Box<dyn Loader>>,
}

impl LoaderRegistry {
    pub fn new() -> Self {
        Self {
            loaders: Vec::new(),
        }
    }

    pub fn register(&mut self, loader: Box<dyn Loader>) {
        self.loaders.push(loader);
    }

    pub fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, LoaderError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| LoaderError::UnsupportedLanguage("no extension".to_string()))?;

        for loader in &self.loaders {
            if loader.supported_extensions().contains(&ext) {
                return loader.load(path, api);
            }
        }

        Err(LoaderError::UnsupportedLanguage(ext.to_string()))
    }
}

impl Default for LoaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn create_default_registry() -> LoaderRegistry {
    #[allow(unused_mut)]
    let mut registry = LoaderRegistry::new();
    #[cfg(feature = "plugin-lua")]
    registry.register(Box::new(lua::LuaLoader));
    #[cfg(feature = "plugin-lisp")]
    registry.register(Box::new(lisp::LispLoader));
    #[cfg(feature = "plugin-js")]
    registry.register(Box::new(javascript::JavaScriptLoader));
    #[cfg(feature = "plugin-nix")]
    registry.register(Box::new(nix::NixLoader));
    #[cfg(feature = "plugin-rust")]
    registry.register(Box::new(rust::RustLoader));
    registry
}

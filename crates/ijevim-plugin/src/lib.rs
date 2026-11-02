mod api;
pub mod loaders;

pub use api::{Plugin, PluginApi};

use ijevim_core::types::{IjevimError, PluginEvent};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::rc::Rc;

pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    api: Rc<PluginApi>,
    registry: loaders::LoaderRegistry,
}

impl PluginManager {
    pub fn new() -> Self {
        let api = Rc::new(PluginApi::new());
        let registry = loaders::create_default_registry();

        Self {
            plugins: Vec::new(),
            api,
            registry,
        }
    }

    /// Config directory: `$XDG_CONFIG_HOME/ijevim/` or `~/.config/ijevim/`.
    fn config_dir() -> std::path::PathBuf {
        std::env::var("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").expect("HOME not set");
                std::path::PathBuf::from(home).join(".config")
            })
            .join("ijevim")
    }

    /// Plugin directory: `$IJEVIM_PLUGIN_DIR` or `$CONFIG_DIR/plugins/`.
    fn plugins_dir() -> std::path::PathBuf {
        if let Ok(dir) = std::env::var("IJEVIM_PLUGIN_DIR") {
            std::path::PathBuf::from(dir)
        } else {
            Self::config_dir().join("plugins")
        }
    }

    /// Load all plugins from init files and plugins directory.
    ///
    /// 1. Load `init.{lua,lisp,js,nix}` from config dir (first match wins per ext;
    ///    each extension is only consulted when its corresponding `plugin-*`
    ///    feature is enabled).
    /// 2. Load all files from plugins/ directory (extension-based dispatch).
    /// 3. Call `setup()` on every loaded plugin.
    /// 4. Emit `PluginEvent::Ready`.
    pub fn load_all(&mut self) -> Result<(), IjevimError> {
        // Step 1: load init files from config dir
        let config_dir = Self::config_dir();
        if config_dir.exists() {
            #[cfg(feature = "plugin-lua")]
            {
                let p = config_dir.join("init.lua");
                if p.exists() {
                    self.load_single(&p);
                }
            }
            #[cfg(feature = "plugin-lisp")]
            {
                let p = config_dir.join("init.lisp");
                if p.exists() {
                    self.load_single(&p);
                }
            }
            #[cfg(feature = "plugin-js")]
            {
                let p = config_dir.join("init.js");
                if p.exists() {
                    self.load_single(&p);
                }
            }
            #[cfg(feature = "plugin-nix")]
            {
                let p = config_dir.join("init.nix");
                if p.exists() {
                    self.load_single(&p);
                }
            }
        }

        // Step 2: load all files from plugins/ directory
        let plugins_dir = Self::plugins_dir();
        if plugins_dir.exists() {
            let entries = std::fs::read_dir(&plugins_dir).map_err(IjevimError::Io)?;
            for entry in entries {
                let entry = entry.map_err(IjevimError::Io)?;
                self.load_single(&entry.path());
            }
        }

        // Step 3 & 4: setup all plugins and emit Ready
        self.setup_all();
        self.emit(PluginEvent::Ready);

        Ok(())
    }

    fn load_single(&mut self, path: &Path) {
        match self.registry.load(path, self.api.clone()) {
            Ok(plugin) => {
                let name = plugin.name().to_string();
                self.plugins.push(plugin);
                eprintln!("[plugin] Loaded: {}", name);
            }
            Err(e) => {
                eprintln!("[plugin] Failed to load {}: {}", path.display(), e);
            }
        }
    }

    /// Call `setup(&api)` on every plugin.
    fn setup_all(&mut self) {
        for plugin in &mut self.plugins {
            let name = plugin.name().to_string();
            let api = self.api.clone();
            // Use a flag to track panic; can't move `plugin` into closure directly.
            let result = catch_unwind(AssertUnwindSafe(|| {
                plugin.setup(&api);
            }));
            if result.is_err() {
                eprintln!("[plugin] Panic in setup(): {}", name);
            }
        }
    }

    pub fn emit(&mut self, event: PluginEvent) {
        for plugin in &mut self.plugins {
            let name = plugin.name().to_string();
            let result = catch_unwind(AssertUnwindSafe(|| {
                plugin.handle_event(&event);
            }));
            if result.is_err() {
                eprintln!("[plugin] Panic in handle_event(): {}", name);
            }
        }

        let handlers = self.api.event_handlers();
        let event_name = match &event {
            PluginEvent::ModeChange { .. } => "mode_change",
            PluginEvent::BufferChange => "buffer_change",
            PluginEvent::Key { .. } => "key",
            PluginEvent::BufferSave { .. } => "buffer_save",
            PluginEvent::Ready => "ready",
        };
        if let Some(event_handlers) = handlers.borrow().get(event_name) {
            for handler in event_handlers {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    handler(&event);
                }));
                if result.is_err() {
                    eprintln!("[plugin] Panic in global event handler ({})", event_name);
                }
            }
        }
    }

    pub fn execute_command(&mut self, cmd: &str, args: Vec<String>) -> bool {
        let cmd_owned = cmd.to_string();
        for plugin in &mut self.plugins {
            let name = plugin.name().to_string();
            let cmd = cmd_owned.clone();
            let args = args.clone();
            let result = catch_unwind(AssertUnwindSafe(|| -> bool {
                plugin.execute_command(&cmd, args)
            }));
            match result {
                Ok(true) => return true,
                Ok(false) => continue,
                Err(_) => {
                    eprintln!("[plugin] Panic in execute_command(): {}", name);
                    continue;
                }
            }
        }

        let commands = self.api.commands();
        if let Some(f) = commands.borrow().get(&cmd_owned) {
            let args = args.clone();
            let result = catch_unwind(AssertUnwindSafe(|| {
                f(args);
            }));
            if result.is_err() {
                eprintln!("[plugin] Panic in global command handler: {}", cmd_owned);
            }
            true
        } else {
            false
        }
    }

    pub fn add_plugin(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

#![cfg(feature = "plugin-lua")]

use crate::{Plugin, PluginApi};
use ijevim_core::types::PluginEvent;
use mlua::{Function, Lua, RegistryKey, Table, Value};
use std::path::Path;
use std::rc::Rc;

pub struct LuaPlugin {
    name: String,
    lua: Lua,
    /// Key in the Lua registry referencing the plugin's returned table.
    table_key: RegistryKey,
}

/// Map PluginEvent -> the Lua-side event name string used in `on()`.
fn plugin_event_name(event: &PluginEvent) -> Option<&'static str> {
    match event {
        PluginEvent::ModeChange { .. } => Some("mode_change"),
        PluginEvent::BufferChange => Some("buffer_change"),
        PluginEvent::Key { .. } => Some("key"),
        PluginEvent::BufferSave { .. } => Some("buffer_save"),
        PluginEvent::Ready => Some("ready"),
    }
}

/// Build a Lua table with event data fields from a PluginEvent.
fn event_data_table<'lua>(lua: &'lua Lua, event: &PluginEvent) -> mlua::Result<Table<'lua>> {
    let t = lua.create_table()?;
    match event {
        PluginEvent::ModeChange { from, to } => {
            t.set("from", format!("{:?}", from))?;
            t.set("to", format!("{:?}", to))?;
        }
        PluginEvent::Key { mode, key } => {
            t.set("mode", format!("{:?}", mode))?;
            t.set("key", key.to_string())?;
        }
        PluginEvent::BufferSave { file_path } => {
            if let Some(p) = file_path {
                t.set("file", p.to_string_lossy().as_ref())?;
            }
        }
        PluginEvent::BufferChange | PluginEvent::Ready => {}
    }
    Ok(t)
}

impl Plugin for LuaPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, api: &PluginApi) {
        let plugin_table: Table = match self.lua.registry_value(&self.table_key) {
            Ok(t) => t,
            Err(_) => return,
        };

        let setup_fn = match plugin_table.get::<_, Function>("setup") {
            Ok(f) => f,
            Err(_) => return,
        };

        let api_table = self.lua.create_table().expect("failed to create Lua table");

        // ── Command registry ─────────────────────────────────────
        let cmd_reg = self.lua.create_table().expect("failed to create cmd table");
        let _ = self
            .lua
            .set_named_registry_value("ijevim_cmd_registry", cmd_reg.clone());

        api_table
            .set(
                "addCommand",
                self.lua
                    .create_function(move |lua, (name, func): (String, Function)| {
                        let reg: Table = lua
                            .named_registry_value("ijevim_cmd_registry")
                            .unwrap_or_else(|_| lua.create_table().unwrap());
                        reg.set(name, func).ok();
                        lua.set_named_registry_value("ijevim_cmd_registry", reg)
                            .ok();
                        Ok(())
                    })
                    .expect("failed to create addCommand"),
            )
            .expect("failed to set addCommand");

        // ── Event registry ───────────────────────────────────────
        let ev_reg = self.lua.create_table().expect("failed to create ev table");
        let _ = self
            .lua
            .set_named_registry_value("ijevim_event_registry", ev_reg.clone());

        api_table
            .set(
                "on",
                self.lua
                    .create_function(move |lua, (event_name, handler): (String, Function)| {
                        let reg: Table = lua
                            .named_registry_value("ijevim_event_registry")
                            .unwrap_or_else(|_| lua.create_table().unwrap());
                        // Store as list: reg[event_name] = [handler, ...]
                        let list: Table = reg
                            .get::<_, Option<Table>>(event_name.as_str())
                            .unwrap_or(None)
                            .unwrap_or_else(|| lua.create_table().unwrap());
                        list.set(list.len().unwrap_or(0) + 1, handler).ok();
                        reg.set(event_name, list).ok();
                        lua.set_named_registry_value("ijevim_event_registry", reg)
                            .ok();
                        Ok(())
                    })
                    .expect("failed to create on"),
            )
            .expect("failed to set on");

        // ── Log ──────────────────────────────────────────────────
        let log_fn = api.log_fn.clone();
        api_table
            .set(
                "log",
                self.lua
                    .create_function(move |_lua, msg: String| {
                        (log_fn)(&msg);
                        Ok(())
                    })
                    .expect("failed to create log"),
            )
            .expect("failed to set log");

        let _ = setup_fn.call::<_, ()>(api_table);
    }

    fn handle_event(&mut self, event: &PluginEvent) {
        let name = match plugin_event_name(event) {
            Some(n) => n,
            None => return,
        };

        let reg: Table = match self.lua.named_registry_value("ijevim_event_registry") {
            Ok(t) => t,
            Err(_) => return,
        };

        let list: Table = match reg.get::<_, Option<Table>>(name) {
            Ok(Some(t)) => t,
            _ => return,
        };

        // Build event data table
        let data = match event_data_table(&self.lua, event) {
            Ok(t) => t,
            Err(_) => return,
        };

        // Iterate over all handlers and pass event data
        for (_idx, handler) in list.pairs::<usize, Function>().flatten() {
            let _ = handler.call::<_, ()>(data.clone());
        }
    }

    fn execute_command(&mut self, cmd: &str, args: Vec<String>) -> bool {
        let reg: Table = match self.lua.named_registry_value("ijevim_cmd_registry") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let f: Function = match reg.get(cmd) {
            Ok(f) => f,
            Err(_) => return false,
        };
        // Convert args to Lua values
        let lua_args: Vec<Value> = args
            .iter()
            .filter_map(|a| self.lua.create_string(a).ok().map(Value::String))
            .collect();
        f.call::<_, ()>(lua_args).is_ok()
    }
}

pub struct LuaLoader;

impl super::Loader for LuaLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["lua"]
    }

    fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, super::LoaderError> {
        let code = std::fs::read_to_string(path).map_err(|e| {
            super::LoaderError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let lua = Lua::new();

        // Create the `ijevim` global table with log function
        let api_outer = api.clone();
        let ijevim_table = lua
            .create_table()
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;

        ijevim_table
            .set(
                "log",
                lua.create_function(move |_lua, msg: String| {
                    api_outer.log(&msg);
                    Ok(())
                })
                .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?,
            )
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;

        lua.globals()
            .set("ijevim", ijevim_table)
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;

        // Execute the plugin script; it should return a table with `name`, `setup`, etc.
        let plugin_table: Table = {
            let value: mlua::Value = lua
                .load(&code)
                .eval()
                .map_err(|e| super::LoaderError::Parse(format!("Lua eval error: {}", e)))?;
            match value {
                mlua::Value::Table(t) => t,
                _ => {
                    return Err(super::LoaderError::Parse(
                        "Lua plugin must return a table".to_string(),
                    ));
                }
            }
        };

        let table_key = lua
            .create_registry_value(plugin_table)
            .map_err(|e| super::LoaderError::Parse(format!("Registry error: {}", e)))?;

        Ok(Box::new(LuaPlugin {
            name,
            lua,
            table_key,
        }))
    }
}

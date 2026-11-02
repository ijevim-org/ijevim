#![cfg(feature = "plugin-rust")]

use crate::api::{CommandFn, EventFn};
use crate::{Plugin, PluginApi};
use ijevim_core::types::PluginEvent;
use libloading::{Library, Symbol};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::path::Path;
use std::rc::Rc;

const PLUGIN_VERSION: u32 = super::API_VERSION;

// ── FFI-safe types ───────────────────────────────────────────────

/// Callback: a command handler provided by the plugin.
type PluginCmdCb = unsafe extern "C" fn(cmd: *const u8, cmd_len: usize) -> u8;

/// Callback: an event handler provided by the plugin.
type PluginEventCb = unsafe extern "C" fn(event_type: u32, data: usize);

/// FFI-safe PluginApi passed to the plugin's `setup`.
#[repr(C)]
struct PluginApiTable {
    /// Opaque handle (points to the RustPlugin instance).
    ctx: *mut std::ffi::c_void,
    /// Log a message.
    log: unsafe extern "C" fn(ctx: *mut std::ffi::c_void, msg: *const u8, len: usize),
    /// Register a command. `cmd_fn` is called when the command is invoked.
    add_command: unsafe extern "C" fn(
        ctx: *mut std::ffi::c_void,
        name: *const u8,
        name_len: usize,
        cmd_fn: Option<PluginCmdCb>,
    ),
    /// Register an event handler.
    on: unsafe extern "C" fn(
        ctx: *mut std::ffi::c_void,
        event: *const u8,
        event_len: usize,
        handler: Option<PluginEventCb>,
    ),
}

// ── Vtable exported by the plugin .so ────────────────────────────

#[repr(C)]
struct PluginVtable {
    /// Return a human-readable name (null-terminated UTF-8).
    get_name: unsafe extern "C" fn() -> *const std::ffi::c_char,
    /// Run the plugin's setup routine with the API table.
    setup: unsafe extern "C" fn(api: PluginApiTable),
    /// Handle an editor event.
    handle_event: unsafe extern "C" fn(event_type: u32, data: usize),
    /// Execute a named command. Returns 1 if handled.
    execute_command: unsafe extern "C" fn(cmd: *const u8, cmd_len: usize) -> u8,
}

type GetApiVersionFn = unsafe extern "C" fn() -> u32;
type GetVtableFn = unsafe extern "C" fn() -> PluginVtable;

// ── Host-side plugin wrapper ─────────────────────────────────────

pub struct RustPlugin {
    _library: Library,
    vtable: PluginVtable,
    name: String,
    /// Commands registered by the plugin during `setup()`.
    cmd_cbs: RefCell<HashMap<String, PluginCmdCb>>,
    /// Event handlers registered by the plugin during `setup()`.
    event_cbs: RefCell<HashMap<String, Vec<PluginEventCb>>>,
}

impl Plugin for RustPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, api: &PluginApi) {
        // Build the PluginApiTable and pass it to the plugin's setup vtable entry.
        let ctx = self as *const RustPlugin as *mut std::ffi::c_void;
        let table = PluginApiTable {
            ctx,
            log: Self::log_cb,
            add_command: Self::add_command_cb,
            on: Self::on_cb,
        };
        unsafe {
            (self.vtable.setup)(table);
        }

        // Register commands stored in cmd_cbs into the global PluginApi.
        let cmd_map: HashMap<String, PluginCmdCb> = std::mem::take(&mut *self.cmd_cbs.borrow_mut());
        for (name, cb) in cmd_map {
            let name_clone = name.clone();
            let command_cb: CommandFn = Box::new(move |_args| {
                let name_bytes = name_clone.as_bytes();
                let _ = unsafe { cb(name_bytes.as_ptr(), name_bytes.len()) };
            });
            let api_cmds = api.commands().clone();
            api_cmds.borrow_mut().insert(name, command_cb);
        }

        // Register event handlers stored in event_cbs into the global PluginApi.
        let ev_map: HashMap<String, Vec<PluginEventCb>> =
            std::mem::take(&mut *self.event_cbs.borrow_mut());
        for (event_name, handlers) in ev_map {
            for handler in handlers {
                let event_cb: EventFn = Box::new(move |_event| {
                    // For now, pass generic event data; extend later
                    unsafe { handler(0, 0) };
                });
                let api_events = api.event_handlers().clone();
                api_events
                    .borrow_mut()
                    .entry(event_name.clone())
                    .or_default()
                    .push(event_cb);
            }
        }
    }

    fn handle_event(&mut self, event: &PluginEvent) {
        // Map PluginEvent to a numeric event type for the native plugin.
        let event_type = match event {
            PluginEvent::ModeChange { .. } => 1,
            PluginEvent::BufferChange => 2,
            PluginEvent::Key { .. } => 3,
            PluginEvent::BufferSave { .. } => 4,
            PluginEvent::Ready => 5,
        };
        unsafe {
            (self.vtable.handle_event)(event_type, 0);
        }
    }

    fn execute_command(&mut self, cmd: &str, args: Vec<String>) -> bool {
        // TODO: pass `args` through the FFI vtable once the interface supports it.
        let _ = args;
        let cmd_bytes = cmd.as_bytes();
        unsafe { (self.vtable.execute_command)(cmd_bytes.as_ptr(), cmd_bytes.len()) != 0 }
    }
}

// ── Extern "C" callbacks for PluginApiTable ──────────────────────

impl RustPlugin {
    unsafe extern "C" fn log_cb(ctx: *mut std::ffi::c_void, msg: *const u8, len: usize) {
        let plugin = &*(ctx as *const RustPlugin);
        let s = std::str::from_utf8(std::slice::from_raw_parts(msg, len)).unwrap_or("");
        eprintln!("[plugin:{}] {}", plugin.name, s);
    }

    unsafe extern "C" fn add_command_cb(
        ctx: *mut std::ffi::c_void,
        name: *const u8,
        name_len: usize,
        cmd_fn: Option<PluginCmdCb>,
    ) {
        let plugin = &*(ctx as *const RustPlugin);
        let name_str = std::str::from_utf8(std::slice::from_raw_parts(name, name_len))
            .unwrap_or("")
            .to_string();
        if let Some(f) = cmd_fn {
            plugin.cmd_cbs.borrow_mut().insert(name_str, f);
        }
    }

    unsafe extern "C" fn on_cb(
        ctx: *mut std::ffi::c_void,
        event: *const u8,
        event_len: usize,
        handler: Option<PluginEventCb>,
    ) {
        let plugin = &*(ctx as *const RustPlugin);
        let event_str = std::str::from_utf8(std::slice::from_raw_parts(event, event_len))
            .unwrap_or("")
            .to_string();
        if let Some(f) = handler {
            plugin
                .event_cbs
                .borrow_mut()
                .entry(event_str)
                .or_default()
                .push(f);
        }
    }
}

// ── Loader ───────────────────────────────────────────────────────

pub struct RustLoader;

impl super::Loader for RustLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["so", "dylib", "dll"]
    }

    fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, super::LoaderError> {
        let library = unsafe {
            Library::new(path)
                .map_err(|e| super::LoaderError::Io(format!("Failed to load library: {}", e)))?
        };

        unsafe {
            let version: Symbol<GetApiVersionFn> =
                library.get(b"ijevim_plugin_api_version").map_err(|e| {
                    super::LoaderError::Io(format!("Failed to get version symbol: {}", e))
                })?;

            let actual_version = version();
            if actual_version != PLUGIN_VERSION {
                return Err(super::LoaderError::ApiVersionMismatch {
                    expected: PLUGIN_VERSION,
                    actual: actual_version,
                });
            }

            let get_vtable: Symbol<GetVtableFn> =
                library.get(b"ijevim_plugin_vtable").map_err(|e| {
                    super::LoaderError::Io(format!("Failed to get vtable symbol: {}", e))
                })?;

            let vtable = get_vtable();
            let name = {
                let ptr = (vtable.get_name)();
                if ptr.is_null() {
                    return Err(super::LoaderError::Parse("Plugin name is null".to_string()));
                }
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            };

            api.log(&format!("Loaded Rust plugin: {}", name));

            Ok(Box::new(RustPlugin {
                _library: library,
                vtable,
                name,
                cmd_cbs: RefCell::new(HashMap::new()),
                event_cbs: RefCell::new(HashMap::new()),
            }))
        }
    }
}

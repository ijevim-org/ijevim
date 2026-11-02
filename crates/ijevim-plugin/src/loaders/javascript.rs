#![cfg(feature = "plugin-js")]

use crate::{Plugin, PluginApi};
use ijevim_core::types::PluginEvent;
use quickjs_rusty::Context;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

type JsCtx = RefCell<Option<Rc<PluginApi>>>;

thread_local! {
    /// Thread-local API reference for JS callbacks.
    /// Closures capture nothing (access via thread-local),
    /// so they are `RefUnwindSafe` and `'static`.
    static JS_API: JsCtx = const { RefCell::new(None) };
}

pub struct JavaScriptPlugin {
    name: String,
    ctx: Context,
}

/// Escape a string for safe embedding in a JS string literal (single-quoted).
fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

impl Plugin for JavaScriptPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, _api: &PluginApi) {
        // Setup is handled during load (the JS code has already run)
    }

    fn handle_event(&mut self, event: &PluginEvent) {
        let event_name = event_name_str(event);
        // Build event data object
        let data_expr = event_data_expr(event);
        let code = format!(
            r#"var _list = __events['{}'];
if (_list) _list.forEach(function(f) {{ f({}); }});
undefined"#,
            js_escape(event_name),
            data_expr,
        );
        let _ = self.ctx.eval(&code, false);
    }

    fn execute_command(&mut self, cmd: &str, args: Vec<String>) -> bool {
        let escaped = js_escape(cmd);
        // Build args array literal
        let args_js: Vec<String> = args.iter().map(|a| format!("'{}'", js_escape(a))).collect();
        let args_str = if args_js.is_empty() {
            String::new()
        } else {
            args_js.join(", ")
        };
        let code = format!(
            r#"(typeof __commands['{}'] === 'function') ? (__commands['{}']({}), true) : false"#,
            escaped, escaped, args_str,
        );
        self.ctx.eval_as::<bool>(&code).unwrap_or_default()
    }
}

fn event_name_str(event: &PluginEvent) -> &'static str {
    match event {
        PluginEvent::Ready => "ready",
        PluginEvent::Key { .. } => "key",
        PluginEvent::BufferChange => "buffer_change",
        PluginEvent::BufferSave { .. } => "buffer_save",
        PluginEvent::ModeChange { .. } => "mode_change",
    }
}

/// Build a JS object expression for event data.
fn event_data_expr(event: &PluginEvent) -> String {
    match event {
        PluginEvent::ModeChange { from, to } => {
            format!(
                "{{ from: '{}', to: '{}' }}",
                js_escape(&format!("{:?}", from)),
                js_escape(&format!("{:?}", to)),
            )
        }
        PluginEvent::Key { mode, key } => {
            format!(
                "{{ mode: '{}', key: '{}' }}",
                js_escape(&format!("{:?}", mode)),
                js_escape(key),
            )
        }
        PluginEvent::BufferSave { file_path } => {
            if let Some(p) = file_path {
                format!("{{ file: '{}' }}", js_escape(&p.to_string_lossy()))
            } else {
                "{}".to_string()
            }
        }
        PluginEvent::BufferChange | PluginEvent::Ready => "{}".to_string(),
    }
}

pub struct JavaScriptLoader;

impl super::Loader for JavaScriptLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["js"]
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

        let ctx = Context::builder().build().map_err(|e| {
            super::LoaderError::Parse(format!("Failed to create JS context: {}", e))
        })?;

        // Store API in thread-local for callback access.
        // Closures capture nothing, so they are `RefUnwindSafe` and `'static`.
        JS_API.with(|slot| {
            *slot.borrow_mut() = Some(api.clone());
        });

        // Register a Rust callback for logging (captures nothing via thread-local).
        ctx.add_callback("__ijevim_log", |msg: String| -> String {
            JS_API.with(|slot| {
                if let Some(ref api) = *slot.borrow() {
                    api.log(&msg);
                }
            });
            String::new()
        })
        .map_err(|e| super::LoaderError::Parse(format!("Failed to register log: {}", e)))?;

        // Set up the JS-side infrastructure: __commands, __events, and the ijevim namespace.
        let preamble = r#"
var __commands = {};
var __events = {};

var ijevim = {
    addCommand: function(name, func) {
        if (typeof func !== 'function') {
            __ijevim_log("addCommand: second argument must be a function");
            return;
        }
        __commands[name] = func;
    },
    on: function(event, handler) {
        if (typeof handler !== 'function') {
            __ijevim_log("on: second argument must be a function");
            return;
        }
        if (!__events[event]) __events[event] = [];
        __events[event].push(handler);
    },
    log: function(msg) {
        __ijevim_log(String(msg));
    }
};
"#;
        ctx.eval(preamble, false)
            .map_err(|e| super::LoaderError::Parse(format!("JS preamble error: {}", e)))?;

        // Evaluate the plugin code
        ctx.eval(&code, false)
            .map_err(|e| super::LoaderError::Parse(format!("JS eval error: {}", e)))?;

        Ok(Box::new(JavaScriptPlugin { name, ctx }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::Loader;
    use crate::PluginApi;

    #[test]
    fn test_js_loader_basic() {
        let dir = std::env::temp_dir().join("ijevim-test-js");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.js");
        std::fs::write(&path, "ijevim.log(\"hello\");").unwrap();

        let loader = JavaScriptLoader;
        let api = Rc::new(PluginApi::new());
        let plugin = loader.load(&path, api).unwrap();
        assert_eq!(plugin.name(), "test");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_js_loader_register_command() {
        let dir = std::env::temp_dir().join("ijevim-test-js-cmd");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cmdtest.js");
        std::fs::write(
            &path,
            "ijevim.addCommand(\"hello-js\", function() { return 42; });",
        )
        .unwrap();

        let loader = JavaScriptLoader;
        let api = Rc::new(PluginApi::new());
        let mut plugin = loader.load(&path, api.clone()).unwrap();

        assert!(plugin.execute_command("hello-js", vec![]));
        assert!(!plugin.execute_command("unknown", vec![]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_js_loader_register_event() {
        let dir = std::env::temp_dir().join("ijevim-test-js-evt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("evttest.js");
        std::fs::write(&path, "ijevim.on(\"ready\", function() { /* noop */ });").unwrap();

        let loader = JavaScriptLoader;
        let api = Rc::new(PluginApi::new());
        let mut plugin = loader.load(&path, api).unwrap();
        plugin.handle_event(&PluginEvent::Ready);
    }
}

#![cfg(feature = "plugin-lisp")]

use crate::api::CommandFn;
use crate::{Plugin, PluginApi};
use ijevim_core::types::PluginEvent;
use rust_lisp::interpreter::eval_block;
use rust_lisp::model::{Env, RuntimeError, Symbol, Value};
use rust_lisp::parser::parse;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Shared mutable state between Lisp native closures and the LispPlugin
struct LispPluginState {
    commands: Vec<String>,
    events: Vec<String>,
}

impl LispPluginState {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
            events: Vec::new(),
        }
    }
}

pub struct LispPlugin {
    name: String,
    // State kept for loader registration; PluginApi handles dispatch
    #[allow(dead_code)]
    state: Rc<RefCell<LispPluginState>>,
}

impl Plugin for LispPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, _api: &PluginApi) {
        // Setup is handled during loading since native closures capture the API
    }

    fn handle_event(&mut self, _event: &PluginEvent) {
        // Event dispatch happens via PluginApi's global event_handlers
        // (registered at load time by the `on` Lisp binding)
    }

    fn execute_command(&mut self, _cmd: &str, _args: Vec<String>) -> bool {
        // Command dispatch happens via PluginApi's global commands
        // (registered at load time by the `add-command` Lisp binding).
        // Return false so PluginManager falls through to PluginApi.
        false
    }
}

pub struct LispLoader;

impl super::Loader for LispLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["lisp"]
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

        // Create shared state
        let state = Rc::new(RefCell::new(LispPluginState::new()));

        // Create a Lisp environment extending the default one
        let env = default_env_with_bindings(state.clone(), api);

        // Parse and evaluate the Lisp code
        let parsed: Vec<Value> = parse(&code).filter_map(|r| r.ok()).collect();

        eval_block(env, parsed.into_iter())
            .map_err(|e| super::LoaderError::Parse(format!("Lisp eval error: {}", e)))?;

        Ok(Box::new(LispPlugin { name, state }))
    }
}

/// Create a Lisp environment with `add-command`, `on`, and `log` bindings.
fn default_env_with_bindings(
    state: Rc<RefCell<LispPluginState>>,
    api: Rc<PluginApi>,
) -> Rc<RefCell<Env>> {
    let env = Rc::new(RefCell::new(rust_lisp::default_env()));

    // define `add-command` as a native closure
    {
        let state_clone = state.clone();
        let api_clone = api.clone();
        let add_command = Value::NativeClosure(Rc::new(RefCell::new(
            move |_env: Rc<RefCell<Env>>, args: Vec<Value>| -> Result<Value, RuntimeError> {
                if args.is_empty() {
                    return Err(RuntimeError {
                        msg: "add-command requires at least 1 argument: (name)".to_string(),
                    });
                }
                let cmd_name = extract_string(&args[0]);
                state_clone.borrow_mut().commands.push(cmd_name.clone());

                let cmd_name_inner = cmd_name.clone();
                api_clone._add_command(
                    cmd_name.clone(),
                    Box::new(move |_args| {
                        let _ = &cmd_name_inner;
                    }) as CommandFn,
                );

                Ok(Value::String(format!("Command '{}' registered", cmd_name)))
            },
        )));
        env.borrow_mut()
            .define(Symbol::from("add-command"), add_command);
    }

    // define `on` as a native closure
    {
        let state_clone = state.clone();
        let api_clone = api.clone();
        let on = Value::NativeClosure(Rc::new(RefCell::new(
            move |_env: Rc<RefCell<Env>>, args: Vec<Value>| -> Result<Value, RuntimeError> {
                if args.is_empty() {
                    return Err(RuntimeError {
                        msg: "on requires at least 1 argument: (event-name)".to_string(),
                    });
                }
                let event_name = extract_string(&args[0]);
                state_clone.borrow_mut().events.push(event_name.clone());

                let event_name_inner = event_name.clone();
                api_clone._on(
                    event_name.clone(),
                    Box::new(move |_event| {
                        let _ = &event_name_inner;
                    }),
                );

                Ok(Value::String(format!(
                    "Handler for '{}' registered",
                    event_name
                )))
            },
        )));
        env.borrow_mut().define(Symbol::from("on"), on);
    }

    // define `log` as a native closure
    {
        let api_clone = api.clone();
        let log = Value::NativeClosure(Rc::new(RefCell::new(
            move |_env: Rc<RefCell<Env>>, args: Vec<Value>| -> Result<Value, RuntimeError> {
                let msg: Vec<String> = args.iter().map(extract_string).collect();
                api_clone.log(&msg.join(" "));
                Ok(Value::NIL)
            },
        )));
        env.borrow_mut().define(Symbol::from("log"), log);
    }

    env
}

/// Extract the inner string from a Lisp Value, handling String values properly
/// (without the quotes that `Display` adds).
fn extract_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::Loader;
    use crate::PluginApi;

    #[test]
    fn test_lisp_loader_basic() {
        let dir = std::env::temp_dir().join("ijevim-test-lisp");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.lisp");
        std::fs::write(&path, "(log \"hello\")").unwrap();

        let loader = LispLoader;
        let api = Rc::new(PluginApi::new());
        let plugin = loader.load(&path, api).unwrap();
        assert_eq!(plugin.name(), "test");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_lisp_loader_register_command() {
        let dir = std::env::temp_dir().join("ijevim-test-lisp-cmd");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cmdtest.lisp");
        std::fs::write(&path, "(add-command \"hello\")").unwrap();

        let loader = LispLoader;
        let api = Rc::new(PluginApi::new());
        let mut _plugin = loader.load(&path, api.clone()).unwrap();

        // LispPlugin::execute_command returns false (delegates to PluginApi).
        // The command should be registered in PluginApi instead.
        assert!(!_plugin.execute_command("hello", vec![]));
        assert!(!_plugin.execute_command("unknown", vec![]));

        // Verify command is registered in global PluginApi
        let cmds = api.commands();
        assert!(cmds.borrow().contains_key("hello"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_lisp_loader_register_event() {
        let dir = std::env::temp_dir().join("ijevim-test-lisp-evt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("evttest.lisp");
        std::fs::write(&path, "(on \"Ready\")").unwrap();

        let loader = LispLoader;
        let api = Rc::new(PluginApi::new());
        let _plugin = loader.load(&path, api).unwrap();
        // Event registration succeeds without error
    }
}

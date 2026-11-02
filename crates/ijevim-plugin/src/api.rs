use ijevim_core::types::PluginEvent;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type CommandFn = Box<dyn Fn(Vec<String>)>;
pub type EventFn = Box<dyn Fn(&PluginEvent)>;
pub type LogFn = fn(&str);

#[derive(Clone)]
pub struct PluginApi {
    commands: Rc<RefCell<HashMap<String, CommandFn>>>,
    event_handlers: Rc<RefCell<HashMap<String, Vec<EventFn>>>>,
    pub log_fn: Rc<LogFn>,
}

fn default_log(msg: &str) {
    eprintln!("[plugin] {}", msg);
}

impl PluginApi {
    pub fn new() -> Self {
        Self {
            commands: Rc::new(RefCell::new(HashMap::new())),
            event_handlers: Rc::new(RefCell::new(HashMap::new())),
            log_fn: Rc::new(default_log),
        }
    }

    pub fn _add_command(&self, name: String, f: CommandFn) {
        self.commands.borrow_mut().insert(name, f);
    }

    pub fn _on(&self, event: String, f: EventFn) {
        self.event_handlers
            .borrow_mut()
            .entry(event)
            .or_default()
            .push(f);
    }

    pub fn log(&self, msg: &str) {
        (self.log_fn)(msg);
    }

    pub fn _log_fn(&self) -> Rc<LogFn> {
        self.log_fn.clone()
    }

    pub fn commands(&self) -> &Rc<RefCell<HashMap<String, CommandFn>>> {
        &self.commands
    }

    pub fn event_handlers(&self) -> &Rc<RefCell<HashMap<String, Vec<EventFn>>>> {
        &self.event_handlers
    }
}

impl Default for PluginApi {
    fn default() -> Self {
        Self::new()
    }
}

pub trait Plugin {
    fn name(&self) -> &str;
    fn setup(&mut self, api: &PluginApi);
    fn handle_event(&mut self, event: &PluginEvent);
    fn execute_command(&mut self, cmd: &str, args: Vec<String>) -> bool;
}

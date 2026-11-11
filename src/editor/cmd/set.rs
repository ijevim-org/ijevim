use crate::editor::Editor;
use crate::types::MessageLevel;

impl Editor {
    pub(super) fn handle_set_command(&mut self, cmd: &str) {
        let args = cmd.trim_start_matches("set ").trim();

        match args {
            "number" => {
                self.engine.state.show_line_numbers = true;
            }
            "nonumber" | "nonumber!" => {
                self.engine.state.show_line_numbers = false;
            }
            "number!" => {
                self.engine.state.show_line_numbers = !self.engine.state.show_line_numbers;
            }
            "relativenumber" => {
                self.engine.state.show_relative_numbers = true;
            }
            "norelativenumber" | "norelativenumber!" => {
                self.engine.state.show_relative_numbers = false;
            }
            "relativenumber!" => {
                self.engine.state.show_relative_numbers = !self.engine.state.show_relative_numbers;
            }
            "hlsearch" => {
                self.engine.state.hlsearch = true;
            }
            "nohlsearch" | "nohlsearch!" => {
                self.engine.state.hlsearch = false;
            }
            "hlsearch!" => {
                self.engine.state.hlsearch = !self.engine.state.hlsearch;
            }
            "expandtab" | "et" => {
                self.engine.state.expandtab = true;
            }
            "noexpandtab" | "noet" => {
                self.engine.state.expandtab = false;
            }
            "expandtab!" | "et!" => {
                self.engine.state.expandtab = !self.engine.state.expandtab;
            }
            args if args.starts_with("tabstop=") || args.starts_with("ts=") => {
                let val = args.split_once('=').map(|x| x.1).unwrap_or("8");
                if let Ok(n) = val.parse::<usize>() {
                    self.engine.state.tabstop = n;
                } else {
                    self.engine
                        .state
                        .push_message(MessageLevel::Warning, format!("Invalid value: {}", val));
                }
            }
            args if args.starts_with("shiftwidth=") || args.starts_with("sw=") => {
                let val = args.split_once('=').map(|x| x.1).unwrap_or("4");
                if let Ok(n) = val.parse::<usize>() {
                    self.engine.state.shiftwidth = n;
                } else {
                    self.engine
                        .state
                        .push_message(MessageLevel::Warning, format!("Invalid value: {}", val));
                }
            }
            args if args.starts_with("scrolloff=") || args.starts_with("so=") => {
                let val = args.split_once('=').map(|x| x.1).unwrap_or("0");
                if let Ok(n) = val.parse::<usize>() {
                    self.engine.state.scrolloff = n;
                } else {
                    self.engine
                        .state
                        .push_message(MessageLevel::Warning, format!("Invalid value: {}", val));
                }
            }
            _ => {
                self.engine.state.push_message(
                    MessageLevel::Warning,
                    format!("Unknown set option: {}", args),
                );
            }
        }
    }
}

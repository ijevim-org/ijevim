use crate::editor::Editor;
use crate::types::{Operator, PendingOperator};
use crossterm::event::KeyCode;

impl Editor {
    pub(super) async fn handle_ctrl_w_prefix(&mut self, key: KeyCode) -> bool {
        if !self.engine.operator_state.ctrl_w_prefix {
            return false;
        }
        self.engine.operator_state.ctrl_w_prefix = false;
        self.handle_ctrl_w(key).await;
        true
    }

    /// Consume the second key of a `"x` register prefix.
    pub(super) fn handle_register_prefix(&mut self, key: KeyCode) -> bool {
        // The sentinel `'\0'` means "waiting for the register name".
        if self.engine.operator_state.register != Some('\0') {
            return false;
        }
        self.engine.operator_state.register = None;
        if let KeyCode::Char(c) = key {
            if c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '_' {
                self.engine.operator_state.register = Some(c);
                self.needs_render = true;
                return true;
            }
            if c == '"' {
                // `""` selects the unnamed register, which is the default.
                self.needs_render = true;
                return true;
            }
        }
        false
    }

    /// Consume the second key of an `m`/`` ` ``/`'` mark prefix.
    pub(super) fn handle_mark_prefix(&mut self, key: KeyCode) -> bool {
        let Some(pending) = self.engine.operator_state.mark else {
            return false;
        };
        self.engine.operator_state.mark = None;
        if let KeyCode::Char(c) = key {
            if (pending == 'm' && c.is_ascii_lowercase()) || (pending != 'm' && !c.is_whitespace())
            {
                self.handle_mark(pending, c);
                self.needs_render = true;
                return true;
            }
        }
        false
    }

    /// Accumulate a Vim count (`1`-`9`, plus `0` once a count is in progress).
    pub(super) fn handle_count_prefix(&mut self, key: KeyCode) -> bool {
        let KeyCode::Char(c) = key else {
            return false;
        };
        if !c.is_ascii_digit() || (c == '0' && self.engine.operator_state.count.is_none()) {
            return false;
        }
        let digit = c.to_digit(10).unwrap() as usize;
        self.engine.operator_state.count =
            Some(self.engine.operator_state.count.unwrap_or(0) * 10 + digit);
        self.needs_render = true;
        true
    }

    /// Handle the file-browser keys that override normal editing.
    pub(super) fn handle_directory_prefix(&mut self, key: KeyCode) -> bool {
        if !self.engine.buffer.is_directory_listing() {
            return false;
        }
        match key {
            KeyCode::Enter => {
                self.open_selected_entry();
                true
            }
            KeyCode::Char('-') => {
                self.go_up_directory();
                true
            }
            KeyCode::Char('R') => {
                self.refresh_directory();
                true
            }
            _ => false,
        }
    }

    /// Resolve a pending operator/text-object before dispatching a new key.
    pub(super) async fn handle_pending_operator(&mut self, key: KeyCode) -> bool {
        let state = std::mem::replace(
            &mut self.engine.operator_state.pending,
            PendingOperator::Idle,
        );
        match state {
            PendingOperator::AwaitingTextObject {
                op,
                register,
                inner,
            } => {
                match op {
                    Operator::Yank | Operator::Delete => {
                        self.handle_text_object(key, register, inner).await;
                    }
                    Operator::Change => {
                        self.handle_text_object_change(key, register, inner).await;
                    }
                    _ => {}
                }
                true
            }
            PendingOperator::AwaitingOperator { op, register } => {
                // `g` starts a two-key motion (`dgg`). Only the operators
                // that take a motion wait for it; `gg` itself is a complete
                // motion and must pass through.
                if let KeyCode::Char('g') = key {
                    if matches!(op, Operator::Delete | Operator::Yank | Operator::Change)
                        && !self.engine.operator_state.pending_g
                    {
                        self.engine.operator_state.pending_g = true;
                        self.engine.operator_state.pending =
                            PendingOperator::AwaitingOperator { op, register };
                        self.needs_render = true;
                        return true;
                    }
                }
                self.engine.operator_state.register = register;
                let count = self.take_count();
                self.handle_operator(op, key, count).await;
                true
            }
            PendingOperator::Idle => false,
        }
    }

    /// Consume the macro name after `@`.
    pub(super) async fn handle_macro_prefix(&mut self, key: KeyCode) -> bool {
        // Check BEFORE the outer match because the macro name register
        // (a-z) overlaps with Vim normal-mode commands like `a` (append).
        if self.engine.operator_state.macro_play.is_none() {
            return false;
        }
        if let KeyCode::Char(c) = key {
            if c.is_ascii_lowercase() {
                // Clear macro_play BEFORE dispatching so re-entrant
                // handle_normal calls don't try to play another macro.
                self.engine.operator_state.macro_play = None;
                self.play_macro(c).await;
                self.needs_render = true;
                return true;
            }
        }
        self.engine.operator_state.macro_play = None;
        false
    }
}

use crate::types::{PendingOperator, Position, SearchDirection, SearchResult};

/// Tracks cursor state related to yank operations.
///
/// Separated from the main cursor position (`EditorState.cursor`) because
/// these fields are only relevant during Emacs yank-pop cycles.
#[derive(Debug, Clone, Default)]
pub struct CursorState {
    /// Cursor position before the last yank (for yank-pop cycle).
    pub yank_start: Option<Position>,
    /// Length of text inserted by the last yank (for yank-pop undo).
    pub yank_length: usize,
}

/// Tracks pending Vim operator state machine.
///
/// After pressing an operator key (d, y, c, >, <, etc.), the editor waits
/// for a motion/text-object/second key to complete the operation. This struct
/// holds all the transient state accumulated during that sequence.
#[derive(Debug, Clone, Default)]
pub struct OperatorState {
    /// Pending operator state machine.
    pub pending: PendingOperator,
    /// Numeric count prefix (e.g., `3` in `3j`). Accumulated digit by digit.
    pub count: Option<usize>,
    /// Register name selected via `"` prefix (e.g., `"a` → Some('a')).
    pub register: Option<char>,
    /// Pending mark operation: `Some('m')` waiting for mark name,
    /// `Some('`')` or `Some('\'')` waiting for jump target.
    pub mark: Option<char>,
    /// Pending macro playback: `Some('@')` waiting for register name.
    pub macro_play: Option<char>,
    /// Character to replace with in Replace mode; None means normal Replace.
    pub replace_char: Option<char>,
    /// Ctrl-w prefix: waiting for second key (w, j, k, h, l, c, =, _, |, etc.)
    pub ctrl_w_prefix: bool,
    /// Emacs C-x prefix: waiting for second key (s, c, h, d, u, /, etc.)
    pub emacs_cx_prefix: bool,
}

impl OperatorState {
    /// Reset all pending operator state (used when switching keymaps).
    pub fn reset(&mut self) {
        self.pending = PendingOperator::Idle;
        self.count = None;
        self.register = None;
        self.mark = None;
        self.macro_play = None;
        self.replace_char = None;
        self.ctrl_w_prefix = false;
        self.emacs_cx_prefix = false;
    }

    /// Take the accumulated count, defaulting to 1 if absent.
    pub fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }
}

/// Tracks search state for Vim `*`, `/`, `?`, `n`, `N`, and `f`/`F`/`t`/`T`.
#[derive(Debug, Clone)]
pub struct SearchState {
    /// Current search query string (set by `/`, `?`, `*`, etc.).
    pub query: String,
    /// Current search direction.
    pub direction: SearchDirection,
    /// Cached search results.
    pub results: Vec<SearchResult>,
    /// Index into `results` for the current match.
    pub current_idx: usize,
    /// Last character searched with `f`/`F`/`t`/`T`.
    pub last_fchar: Option<char>,
    /// Whether the last `f`/`F` was a `t`/`T` (till) operation.
    pub last_fchar_till: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            direction: SearchDirection::Forward,
            results: Vec::new(),
            current_idx: 0,
            last_fchar: None,
            last_fchar_till: false,
        }
    }
}

/// Tracks accumulated insert-mode state for undo grouping.
///
/// When the user types in insert mode, characters are accumulated and pushed
/// as a single undo entry when leaving insert mode.
#[derive(Debug, Clone, Default)]
pub struct InsertState {
    /// Accumulated characters typed during the current insert-mode session.
    pub accum: String,
    /// Cursor position when insert mode was entered (for undo cursor_before).
    pub start_pos: Option<Position>,
    /// Set by Ctrl-r in insert mode; the next non-modifier key selects a register.
    pub waiting_register: bool,
}

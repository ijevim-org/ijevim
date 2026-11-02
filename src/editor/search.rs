use crate::editor::Editor;
use crate::types::SearchDirection;

impl Editor {
    pub(crate) fn search_word_under_cursor(&mut self) {
        let (word, _, _) = self
            .engine
            .buffer
            .get_word_range(self.engine.state.cursor.line, self.engine.state.cursor.col);
        if !word.is_empty() {
            let query = word.clone();
            self.do_search(&query, SearchDirection::Forward);
        }
    }

    pub(crate) fn search_next(&mut self) {
        if !self.engine.search_state.query.is_empty() {
            let query = self.engine.search_state.query.clone();
            let dir = self.engine.search_state.direction;
            self.do_search(&query, dir);
        }
    }

    pub(crate) fn search_prev(&mut self) {
        if !self.engine.search_state.query.is_empty() {
            let query = self.engine.search_state.query.clone();
            let dir = match self.engine.search_state.direction {
                SearchDirection::Forward => SearchDirection::Backward,
                SearchDirection::Backward => SearchDirection::Forward,
            };
            self.do_search(&query, dir);
        }
    }

    pub(crate) fn do_search(&mut self, query: &str, direction: SearchDirection) {
        self.engine.search_state.query = query.to_string();
        self.engine.search_state.direction = direction;
        self.engine.state.search_pattern = Some(query.to_string());
        self.engine.search_state.results = self.engine.buffer.search(query);

        if self.engine.search_state.results.is_empty() {
            return;
        }

        if direction == SearchDirection::Forward {
            self.engine.search_state.current_idx = self
                .engine
                .search_state
                .results
                .iter()
                .position(|r| {
                    r.line > self.engine.state.cursor.line
                        || (r.line == self.engine.state.cursor.line
                            && r.start_col > self.engine.state.cursor.col)
                })
                .unwrap_or(0);
        } else {
            self.engine.search_state.current_idx = self
                .engine
                .search_state
                .results
                .iter()
                .rposition(|r| {
                    r.line < self.engine.state.cursor.line
                        || (r.line == self.engine.state.cursor.line
                            && r.start_col < self.engine.state.cursor.col)
                })
                .unwrap_or(self.engine.search_state.results.len() - 1);
        }

        if let Some(result) = self
            .engine
            .search_state
            .results
            .get(self.engine.search_state.current_idx)
        {
            self.engine.state.cursor.line = result.line;
            self.engine.state.cursor.col = result.start_col;
        }
    }
}

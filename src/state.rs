use std::cmp::Ordering;

use bimap::BiMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorState {
    Ok,
    Error,
}

#[derive(PartialEq, Eq, Ord, Hash, Debug, Clone, Copy)]
pub struct CodeSpan {
    begin_line: u64,
    begin_column: u64,
    end_line: u64,
    end_column: u64,
}

impl CodeSpan {
    pub fn new(begin_line: u64, begin_column: u64, end_line: u64, end_column: u64) -> Self {
        Self {
            begin_line,
            begin_column,
            end_line,
            end_column,
        }
    }
}

impl CodeSpan {
    pub fn extend(self, other: &CodeSpan) -> Self {
        Self {
            begin_line: if other.begin_line <= self.begin_line {
                other.begin_line
            } else {
                self.begin_line
            },
            begin_column: if other.begin_line < self.begin_line {
                other.begin_column
            } else if other.begin_line == self.begin_line {
                if other.begin_column <= self.begin_column {
                    other.begin_column
                } else {
                    self.begin_column
                }
            } else {
                self.begin_column
            },
            end_line: if other.end_line >= self.end_line {
                other.end_line
            } else {
                self.end_line
            },
            end_column: if other.end_line > self.end_line {
                other.end_column
            } else if other.end_line == self.end_line {
                if other.end_column >= self.end_column {
                    other.end_column
                } else {
                    self.end_column
                }
            } else {
                self.end_column
            },
        }
    }
}

impl Default for CodeSpan {
    fn default() -> Self {
        Self {
            begin_line: 1,
            begin_column: 1,
            end_line: 1,
            end_column: 1,
        }
    }
}

impl PartialOrd for CodeSpan {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.begin_line < other.begin_line {
            Some(Ordering::Less)
        } else if self.begin_line == other.begin_line {
            if self.begin_column < other.begin_column {
                Some(Ordering::Less)
            } else if self.begin_column == other.begin_column {
                Some(Ordering::Equal)
            } else {
                Some(Ordering::Equal)
            }
        } else {
            Some(Ordering::Greater)
        }
    }
}

impl std::fmt::Display for CodeSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{},{}.{}",
            self.begin_line, self.begin_column, self.end_line, self.end_column
        )
    }
}

///////////////////////////////////::

pub struct CoqState {
    error_state: ErrorState,
    state_id_to_range: BiMap<i64, CodeSpan>,
    root_id: i64,
    current_id: i64,
    last_processed: bool,
}

impl CoqState {
    pub fn new() -> Self {
        Self {
            error_state: ErrorState::Ok,
            state_id_to_range: BiMap::new(),
            root_id: 0,
            current_id: 0,
            last_processed: false,
        }
    }

    /// Move into an error state.
    pub fn error(&mut self) {
        self.error_state = ErrorState::Error;
    }

    /// Go back to an ok state.
    pub fn ok(&mut self) {
        self.error_state = ErrorState::Ok;
    }

    pub fn get_error_state(&self) -> ErrorState {
        self.error_state
    }

    pub fn reset_last_processed(&mut self) {
        self.last_processed = false;
    }

    pub fn set_current_state_id(&mut self, state_id: i64) {
        if self.root_id == 0 {
            self.root_id = state_id;
        }
        self.current_id = state_id;
    }

    pub fn get_current_state_id(&self) -> i64 {
        if self.current_id == 0 {
            self.root_id
        } else {
            self.current_id
        }
    }

    pub fn get_current_range(&self) -> CodeSpan {
        self.state_id_to_range
            .get_by_left(&self.get_current_state_id())
            .cloned()
            .unwrap_or_else(|| CodeSpan::default())
    }

    pub fn push_range(&mut self, state_id: i64, range: CodeSpan) {
        self.state_id_to_range.insert(state_id, range);
        self.last_processed = true;
    }

    pub fn backtrack_to_id(&mut self, state_id: i64) {
        self.state_id_to_range.retain(|id, _| id <= &state_id);
        self.last_processed = false;

        self.set_current_state_id(
            *self
                .state_id_to_range
                .left_values()
                .max()
                .unwrap_or(&self.root_id),
        );
    }

    pub fn backtrack_to_position(&mut self, line: u64, col: u64) {
        let is_before = |span: &CodeSpan| -> bool {
            span.end_line < line || (span.end_line == line && span.end_column < col)
        };

        self.state_id_to_range.retain(|_, span| is_before(span));
        self.last_processed = false;

        self.set_current_state_id(
            *self
                .state_id_to_range
                .left_values()
                .max()
                .unwrap_or(&self.root_id),
        );
    }

    pub fn processed_range(&self) -> CodeSpan {
        self.state_id_to_range
            .right_values()
            .fold(CodeSpan::default(), CodeSpan::extend)
    }

    pub fn backtrack_last_processed(&mut self) {
        if self.last_processed {
            let state_id = self.get_current_state_id();
            self.state_id_to_range.remove_by_left(&state_id);

            self.set_current_state_id(
                *self
                    .state_id_to_range
                    .left_values()
                    .max()
                    .unwrap_or(&self.root_id),
            );

            self.last_processed = false;
        }
    }
}

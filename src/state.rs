use std::cmp::Ordering;

use bimap::BiMap;

/// The current state of the daemon:
/// - [`ErrorState::Ok`] means that everything is fine and we can continue.
/// - [`ErrorState::Error`] means that an error occured somewhere in your Coq code (e.g. a syntax error)
///   therefore the daemon is unable to continue its work.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorState {
    Ok,
    Error,
}

/// A very simply code span structure containing beginning and ending positions.
#[derive(PartialEq, Eq, Ord, Hash, Debug, Clone, Copy)]
pub struct CodeSpan {
    begin_line: u64,
    begin_column: u64,
    end_line: u64,
    end_column: u64,
}

impl CodeSpan {
    /// Creates a new code span.
    pub fn new(begin_line: u64, begin_column: u64, end_line: u64, end_column: u64) -> Self {
        Self {
            begin_line,
            begin_column,
            end_line,
            end_column,
        }
    }

    /// Extends a range using another one, only if it grows.
    /// For example, given a span `1.1,5.2`:
    /// - if it is extended with `1.1,6.3`, then the resulting range is `1.1,6.3`.
    /// - if it is extended with `1.1,3.5`, the resulting range is `1.1,5.2`.
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
    /// The default range is `1.1,1.1`.
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
    /// Outputs a range using the format `<begin_line>.<begin_column>,<end_line>.<end_column>`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{},{}.{}",
            self.begin_line, self.begin_column, self.end_line, self.end_column
        )
    }
}

///////////////////////////////////::

/// The current daemon state, holding
/// - if the daemon has errored out or not
/// - mappings from state IDs to processed ranges
/// - the root state ID returned when calling `Init`
/// - the current state ID
/// - whether the last processed statements changed the processing range or not.
pub struct CoqState {
    error_state: ErrorState,
    state_id_to_range: BiMap<i64, CodeSpan>,
    root_id: i64,
    current_id: i64,
    last_processed: Option<i64>,
}

impl CoqState {
    /// Creates a new daemon state with the following attributes:
    /// - `error state = Ok`
    /// - no mappings
    /// - `root ID = 1`
    /// - `current ID = 0`
    /// - no statements processed
    pub fn new() -> Self {
        Self {
            error_state: ErrorState::Ok,
            state_id_to_range: BiMap::new(),
            root_id: 1,
            current_id: 0,
            last_processed: None,
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

    /// Returns the current error state.
    pub fn get_error_state(&self) -> ErrorState {
        self.error_state
    }

    /// Nothing was processed anymore.
    pub fn reset_last_processed(&mut self) {
        self.last_processed = None;
    }

    /// Set the last processed state ID
    pub fn set_last_processed(&mut self, state_id: i64) {
        self.last_processed = Some(state_id);
    }

    /// Sets the current state ID (and the root ID if it wasn't set already).
    pub fn set_current_state_id(&mut self, state_id: i64) {
        // NOTE: do not backtrack
        //if self.current_id > state_id {
        //    return;
        //}

        if self.root_id == 0 {
            self.root_id = state_id;
        }
        self.current_id = state_id;
    }

    /// Retrieves the current state ID (which may be the root ID).
    pub fn get_current_state_id(&self) -> i64 {
        if self.current_id == 0 {
            self.root_id
        } else {
            self.current_id
        }
    }

    /// Gets the range associated with the current state ID, or `1.1,1.1` if none can be found.
    pub fn get_current_range(&self) -> CodeSpan {
        let max_id = *self
            .state_id_to_range
            .left_values()
            .max()
            .unwrap_or(&self.root_id);

        self.state_id_to_range
            .get_by_left(&max_id)
            .cloned()
            .unwrap_or_else(|| CodeSpan::default())
    }

    /// Adds a new processed range associated with a state ID to the state.
    pub fn push_range(&mut self, state_id: i64, range: CodeSpan) {
        self.state_id_to_range.insert(state_id, range);
    }

    /// Backtrack to the last state before the indicated position.
    ///
    /// The last state is defined as the farthest range not containing the given `line.col` coordinates.
    pub fn backtrack_before_position(&mut self, line: u64, col: u64) {
        let is_before = |span: &CodeSpan| -> bool {
            span.end_line < line || (span.end_line == line && span.end_column < col)
        };

        self.state_id_to_range.retain(|_, span| is_before(span));
        self.last_processed = None;

        self.current_id = *self
            .state_id_to_range
            .left_values()
            .max()
            .unwrap_or(&self.root_id);
    }

    /// Retrieves the entire processed range in the daemon state.
    pub fn processed_range(&self) -> CodeSpan {
        self.state_id_to_range
            .right_values()
            .fold(CodeSpan::default(), CodeSpan::extend)
    }

    /// Removes the last processed range from the daemon state.
    pub fn backtrack_last_processed(&mut self) {
        if let Some(state_id) = self.last_processed {
            self.state_id_to_range.remove_by_left(&state_id);

            self.current_id = *self
                .state_id_to_range
                .left_values()
                .max()
                .unwrap_or(&self.root_id);

            self.last_processed = None;
        }
    }

    /// Go back one state.
    pub fn backtrack_one_state(&mut self) -> i64 {
        let state_id = *self
            .state_id_to_range
            .left_values()
            .max()
            .unwrap_or(&self.root_id);
          
        self.state_id_to_range.remove_by_left(&state_id);
        self.current_id = *self
            .state_id_to_range
            .left_values()
            .max()
            .unwrap_or(&self.root_id);

        state_id
    }
}

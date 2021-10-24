use bimap::BiMap;

#[derive(Clone, Copy)]
pub enum ErrorState {
    Ok,
    Error,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl std::fmt::Debug for CodeSpan {
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
    all_processed_state_ids: Vec<i64>,
    state_id_to_range: BiMap<i64, CodeSpan>,
}

impl CoqState {
    pub fn new() -> Self {
        Self {
            error_state: ErrorState::Ok,
            all_processed_state_ids: Vec::new(),
            state_id_to_range: BiMap::new(),
        }
    }

    /// Move into an error state.
    pub fn error(&mut self) {
        self.error_state = ErrorState::Error;
    }

    /// Go back to an ok state.
    pub fn ok(&mut self) {
        self.error_state =  ErrorState::Ok;
    }

    pub fn get_error_state(&self) -> ErrorState {
        self.error_state
    }
}

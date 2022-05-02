use std::collections::VecDeque;

use crate::range::Range;

#[derive(Clone, Debug)]
pub struct Operation {
    pub state_id: i64,
    pub range: Range,
}

impl Default for Operation {
    fn default() -> Self {
        Self {
            state_id: 1,
            range: Range::default(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorState {
    Ok,
    Error,
    Interrupted,
}

pub struct State {
    pub operations: VecDeque<Operation>,
    pub last_error_range: Option<Range>,
    pub error_state: ErrorState,
}

impl State {
    pub fn new() -> Self {
        Self {
            operations: VecDeque::new(),
            last_error_range: None,
            error_state: ErrorState::Ok,
        }
    }
}

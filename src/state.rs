use std::{
    collections::VecDeque,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{client::commands::types::ClientCommand, range::Range};

#[derive(Clone, Debug)]
pub struct Operation {
    pub state_id: i64,
    pub range: Range,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorState {
    Ok,
    Error,
    ClearQueue,
}

pub struct State {
    pub operations: VecDeque<Operation>,
    pub waiting: VecDeque<ClientCommand>,
    pub last_error_range: Option<Range>,
    pub error_state: ErrorState,
    go_further: AtomicBool,
}

impl State {
    pub fn new() -> Self {
        Self {
            operations: VecDeque::new(),
            waiting: VecDeque::new(),
            last_error_range: None,
            error_state: ErrorState::Ok,
            go_further: AtomicBool::new(true),
        }
    }

    pub fn can_go_further(&self) -> bool {
        self.go_further.load(Ordering::Relaxed)
    }

    pub fn stop_processing(&mut self) {
        log::debug!("Stopping processing of incoming messages");
        self.go_further.store(false, Ordering::Relaxed);
    }

    pub fn continue_processing(&mut self) {
        log::debug!("Continuing processing incoming messages");
        self.go_further.store(true, Ordering::Relaxed);
    }
}

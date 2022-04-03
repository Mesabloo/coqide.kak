use std::{
    io,
    sync::{Arc, Mutex},
};

use itertools::{enumerate, partition};
use tokio::sync::{
    broadcast::{self, error::TryRecvError},
    mpsc, watch,
};

use crate::{
    client::commands::types::{ClientCommand, DisplayCommand},
    coqtop::xml_protocol::types::{FeedbackContent, MessageType},
    range::Range,
    session::Session,
    state::{ErrorState, Operation, State},
};

use super::xml_protocol::types::{ProtocolResult, ProtocolRichPP, ProtocolValue};

pub struct ResponseProcessor {
    command_rx: broadcast::Receiver<ClientCommand>,
    command_tx: broadcast::Sender<ClientCommand>,
    coqtop_response_rx: mpsc::UnboundedReceiver<ProtocolResult>,
    command_display_tx: mpsc::UnboundedSender<DisplayCommand>,
    state: Arc<Mutex<State>>,
}

impl ResponseProcessor {
    pub fn new(
        _session: Arc<Session>,
        command_rx: broadcast::Receiver<ClientCommand>,
        command_tx: broadcast::Sender<ClientCommand>,
        coqtop_response_rx: mpsc::UnboundedReceiver<ProtocolResult>,
        command_display_tx: mpsc::UnboundedSender<DisplayCommand>,
        state: Arc<Mutex<State>>,
    ) -> Self {
        Self {
            command_rx,
            command_tx,
            coqtop_response_rx,
            command_display_tx,
            state,
        }
    }

    pub async fn process_until(&mut self, mut stop: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop.changed() => break Ok(()),
                Some(resp) = self.coqtop_response_rx.recv() => {
                    self.process_response(resp).await?;
                }
            }
        }
    }

    // ----------------------------------------------------------------------------

    async fn process_response(&mut self, resp: ProtocolResult) -> io::Result<()> {
        use ProtocolResult::*;
        use ProtocolValue::*;

        let error_state = {
            let state = self.state.lock().unwrap();
            state.error_state.clone()
        };

        match resp {
            Good(value) => {
                let last_command = {
                    let mut state = self.state.lock().unwrap();
                    state.waiting.pop_back()
                };

                match (value, last_command) {
                    (StateId(state_id), Some(ClientCommand::Init)) => {
                        {
                            let mut state = self.state.lock().unwrap();
                            state.operations.push_front(Operation {
                                state_id,
                                range: Range::default(),
                            });
                            // state.continue_processing();
                        }

                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;
                        log::info!("Init at state ID {}", state_id);
                    }
                    (_, Some(ClientCommand::Quit)) => {}
                    (_, Some(ClientCommand::Previous)) => {
                        let old_op = {
                            let mut state = self.state.lock().unwrap();
                            let old_op = state.operations.pop_front();
                            state.last_error_range = None;
                            state.error_state = ErrorState::Ok;
                            old_op
                        };
                        match old_op {
                            Some(Operation { range, .. }) => {
                                self.send_command(DisplayCommand::RemoveProcessed(range))
                                    .await?;
                                self.send_command(DisplayCommand::RemoveToBeProcessed(range))
                                    .await?;
                            }
                            _ => {}
                        }
                        self.send_command(DisplayCommand::RefreshErrorRange(None))
                            .await?;
                        self.send_command(DisplayCommand::GotoTip).await?;
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;

                        log::info!("Popped last state from processed ones");
                    }
                    (_, Some(ClientCommand::BackTo(Operation { state_id, .. }))) => {
                        let to_remove = {
                            let mut state = self.state.lock().unwrap();
                            let mut ops_to_remove = Vec::new();

                            loop {
                                if let Some(op) = state.operations.front() {
                                    if op.state_id == state_id {
                                        break;
                                    } else {
                                        ops_to_remove.push(state.operations.pop_front().unwrap());
                                    }
                                } else {
                                    break;
                                }
                            }

                            if error_state == ErrorState::Ok {
                                state.last_error_range = None;
                            }
                            // state.continue_processing();

                            ops_to_remove
                        };

                        if error_state == ErrorState::Ok {
                            self.send_command(DisplayCommand::RefreshErrorRange(None))
                                .await?;
                            self.send_command(DisplayCommand::ColorResult(
                                ProtocolRichPP::RichPP(vec![]),
                                false,
                            ))
                            .await?;
                        }
                        for op in to_remove {
                            self.send_command(DisplayCommand::RemoveProcessed(op.range.clone()))
                                .await?;
                            self.send_command(DisplayCommand::RemoveToBeProcessed(op.range))
                                .await?;
                        }
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;
                    }
                    (Optional(None), Some(ClientCommand::ShowGoals(_))) => {
                        {
                            let mut state = self.state.lock().unwrap();
                            // state.continue_processing();
                        }
                        self.send_command(DisplayCommand::OutputGoals(vec![], vec![], vec![]))
                            .await?;
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;
                    }
                    (
                        Optional(Some(box Goals(fg, bg, _, gg))),
                        Some(ClientCommand::ShowGoals(_)),
                    ) => {
                        {
                            let mut state = self.state.lock().unwrap();
                            // state.continue_processing();
                        }
                        self.send_command(DisplayCommand::OutputGoals(fg, bg, gg))
                            .await?;
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;
                    }

                    _ if error_state != ErrorState::Ok => {
                        // let mut state = self.state.lock().unwrap();
                        // state.continue_processing();
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;
                    }
                    (
                        Pair(box StateId(state_id), box Pair(box union, _)),
                        Some(ClientCommand::Next(range, _)),
                    ) => {
                        self.send_command(DisplayCommand::ColorResult(
                            ProtocolRichPP::RichPP(vec![]),
                            false,
                        ))
                        .await?;
                        let new_state_id = match union {
                            Inl(box Unit) => state_id,
                            Inr(box StateId(state_id)) => state_id,
                            _ => unreachable!(),
                        };
                        {
                            let mut state = self.state.lock().unwrap();
                            state.operations.push_front(Operation {
                                state_id: new_state_id,
                                range: range.clone(),
                            });
                            // state.continue_processing();
                        }
                        self.send_command(DisplayCommand::RefreshErrorRange(None))
                            .await?;
                        self.send_command(DisplayCommand::AddToProcessed(range))
                            .await?;
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;

                        log::debug!("Added code to processed operations");
                    }
                    _ => todo!(),
                }
            }
            Fail(_, _, StateId(safe_state_id), message) => {
                let last_command = {
                    let mut state = self.state.lock().unwrap();
                    state.waiting.pop_back()
                };

                log::debug!("Failed with last command being {:?}", last_command);

                match last_command {
                    _ if error_state != ErrorState::Ok => {
                        // let mut state = self.state.lock().unwrap();
                        // state.continue_processing();
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;
                    }
                    Some(ClientCommand::Init) => {
                        // TODO: failed to init
                    }
                    Some(ClientCommand::Query(_)) => {
                        // TODO: show error
                    }
                    Some(ClientCommand::Next(range, _) | ClientCommand::ShowGoals(range)) => {
                        self.discard_states_until(safe_state_id).await?;
                        self.handle_error(Some(range.clone()), message, false)
                            .await?;

                        {
                            // let mut state = self.state.lock().unwrap();
                            // state.continue_processing();
                        }
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;
                    }
                    Some(ClientCommand::BackTo(op)) => {
                        self.discard_states_until(safe_state_id).await?;
                        self.handle_error(Some(op.range), message, false).await?;
                        {
                            // let mut state = self.state.lock().unwrap();
                            // state.continue_processing();
                        }
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;
                    }
                    c => {
                        {
                            // let mut state = self.state.lock().unwrap();
                            // state.continue_processing();
                        }
                        self.discard_states_until(safe_state_id).await?;
                        self.send_command(DisplayCommand::ColorResult(message.error(), true))
                            .await?;
                        self.send_command(DisplayCommand::ContinueProcessing)
                            .await?;

                        log::error!("Command {:?} caused a failure", c);
                    }
                }
            }
            Feedback(_, _, StateId(state_id), content) => {
                let last_op = {
                    let state = self.state.lock().unwrap();
                    state.operations.front().cloned()
                };

                match last_op {
                    Some(Operation { state_id: id, .. }) if id != state_id => {}
                    // NOTE: ignore messages which are for previous states
                    _ => match content {
                        FeedbackContent::Message(message_type, message) => match message_type {
                            MessageType::Error => {
                                //self.discard_states_until(state_id).await?;
                                //self.handle_error(None, message, true).await?;
                                self.send_command(DisplayCommand::ColorResult(
                                    message.error(),
                                    true,
                                ))
                                .await?;
                            }
                            _ if error_state != ErrorState::Ok => {}
                            MessageType::Notice | MessageType::Info => {
                                self.send_command(DisplayCommand::ColorResult(message, true))
                                    .await?;
                            }
                            MessageType::Debug => log::debug!("@{}: {}", state_id, message.strip()),
                            MessageType::Warning => {
                                self.send_command(DisplayCommand::ColorResult(
                                    message.warning(),
                                    true,
                                ))
                                .await?;
                            }
                        },
                        _ if error_state != ErrorState::Ok => {}
                        _ => log::debug!("Received feedback object @{}: {:?}", state_id, content),
                    },
                }
            }
            _ => {}
        }

        Ok(())
    }

    // ---------------------------------

    async fn handle_error(
        &mut self,
        error_range: Option<Range>,
        message: ProtocolRichPP,
        append: bool,
    ) -> io::Result<()> {
        {
            let mut state = self.state.lock().unwrap();
            state.error_state = ErrorState::ClearQueue;
            state.last_error_range = error_range.clone();
        }

        log::debug!("Handling error with range begin {:?}", error_range);

        loop {
            match self.command_rx.try_recv() {
                Ok(_) => {}
                Err(TryRecvError::Empty) => {
                    log::debug!("Command channel is now empty. Moving towards error state.");

                    let mut state = self.state.lock().unwrap();
                    state.error_state = ErrorState::Error;

                    let safe_state = state.operations.front();
                    if let Some(op) = safe_state {
                        self.command_tx
                            .send(ClientCommand::BackTo(op.clone()))
                            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                        // self.command_tx
                        //     .send(ClientCommand::ShowGoals(op.range))
                        //     .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                    }

                    break Ok(());
                }
                Err(e) => break Err(e),
            }
        }
        .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;

        self.send_command(DisplayCommand::ColorResult(message.error(), append))
            .await?;
        if let Some(range) = error_range.clone() {
            log::debug!("Removing spurious range");

            self.send_command(DisplayCommand::RemoveToBeProcessed(range.clone()))
                .await?;
            self.send_command(DisplayCommand::RemoveProcessed(range))
                .await?;
            self.send_command(DisplayCommand::RefreshErrorRange(error_range))
                .await?;
        }

        Ok(())
    }

    async fn discard_states_until(&mut self, state_id: i64) -> io::Result<()> {
        let mut operations = {
            let mut state = self.state.lock().unwrap();
            let operations = state.operations.clone();

            state.operations.retain(|op| op.state_id <= state_id);

            operations
        };

        log::debug!("Rewinding to state ID {}", state_id);

        let split_index = partition(&mut operations, |op: &Operation| op.state_id > state_id);
        for (i, op) in enumerate(operations) {
            if i >= split_index {
                break;
            }

            log::debug!("Removing operation on state ID {}", op.state_id);

            self.send_command(DisplayCommand::RemoveProcessed(op.range))
                .await?;
            self.send_command(DisplayCommand::RemoveToBeProcessed(op.range))
                .await?;
        }

        Ok(())
    }

    fn find_range(&self, state_id: i64) -> Option<Range> {
        let state = self.state.lock().unwrap();
        state
            .operations
            .iter()
            .find(|op| op.state_id == state_id)
            .map(|op| op.range.clone())
    }

    async fn send_command(&mut self, command: DisplayCommand) -> io::Result<()> {
        log::debug!("Sending UI command {:?}", command);

        self.command_display_tx
            .send(command)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }
}

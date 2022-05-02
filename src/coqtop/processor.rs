use std::{
    collections::VecDeque,
    io,
    sync::{Arc, RwLock},
};

use itertools::{enumerate, partition};
use tokio::sync::broadcast;

use crate::{
    client::commands::types::{ClientCommand, DisplayCommand},
    coqtop::xml_protocol::types::{
        FeedbackContent, MessageType, ProtocolRichPP, ProtocolRichPPPart,
    },
    range::Range,
    session::Session,
    state::{ErrorState, Operation, State},
};

use super::xml_protocol::types::{ProtocolResult, ProtocolValue};

pub struct CoqIdeTopProcessor {
    /// The global session to communicate with Kakoune.
    _session: Arc<Session>,
    /// The global state of the application.
    state: Arc<RwLock<State>>,
    /// Internal channel sender to send more commands.
    command_tx: broadcast::Sender<ClientCommand>,
}

impl CoqIdeTopProcessor {
    pub fn new(
        session: Arc<Session>,
        state: Arc<RwLock<State>>,
        command_tx: broadcast::Sender<ClientCommand>,
    ) -> io::Result<Self> {
        Ok(Self {
            _session: session,
            state,
            command_tx,
        })
    }

    pub async fn process_feedback(
        &mut self,
        feedback: VecDeque<ProtocolResult>,
    ) -> io::Result<VecDeque<DisplayCommand>> {
        use ProtocolValue::*;

        let mut commands = VecDeque::new();

        let (last_op, error_state) = {
            let state = self.state.read().unwrap();
            (state.operations.front().cloned(), state.error_state)
        };

        for feedback in feedback {
            match feedback {
                ProtocolResult::Good(_) => unreachable!(),
                ProtocolResult::Fail(_, _, _, _) => unreachable!(),
                ProtocolResult::Feedback(_, _, StateId(state_id), content) => {
                    match last_op {
                        Some(Operation { state_id: id, .. }) if id > state_id => {}
                        // NOTE: ignore messages which are for previous states
                        _ => match content {
                            FeedbackContent::Message(message_type, message) => match message_type {
                                MessageType::Error => {
                                    log::error!("@{}: {}", state_id, message.strip());
                                }
                                _ if error_state != ErrorState::Ok => {}
                                MessageType::Notice | MessageType::Info => {
                                    commands.push_back(DisplayCommand::ColorResult(message, true));
                                }
                                MessageType::Debug => {
                                    log::debug!("@{}: {}", state_id, message.strip())
                                }
                                MessageType::Warning => {
                                    commands.push_back(DisplayCommand::ColorResult(
                                        message.warning(),
                                        true,
                                    ));
                                }
                            },
                            _ if error_state != ErrorState::Ok => {}
                            FeedbackContent::FileLoaded(Str(name), Str(path)) => {
                                use ProtocolRichPPPart::*;

                                commands.push_back(DisplayCommand::ColorResult(
                                    ProtocolRichPP::RichPP(vec![
                                        Raw("module \"".to_string()),
                                        Reference(name),
                                        Raw("\" (".to_string()),
                                        Path(path),
                                        Raw(") imported.".to_string()),
                                    ]),
                                    true,
                                ));
                            }
                            FeedbackContent::AddedAxiom => {
                                let state_range = self.find_range(state_id);
                                match state_range {
                                    Some(range) => {
                                        commands.push_back(DisplayCommand::AddAxiom(range))
                                    }
                                    None => {}
                                }
                            }
                            FeedbackContent::Processed => {
                                if let Some(range) = self.find_range(state_id) {
                                    commands.push_back(DisplayCommand::AddToProcessed(range));
                                }
                            }
                            _ => {
                                log::debug!("Received feedback object @{}: {:?}", state_id, content)
                            }
                        },
                    }
                }
                ProtocolResult::Feedback(_, _, _, _) => unreachable!(),
            }
        }

        Ok(commands)
    }

    pub async fn process_response(
        &mut self,
        response: ProtocolResult,
        command: ClientCommand,
    ) -> io::Result<VecDeque<DisplayCommand>> {
        use ProtocolValue::*;

        let mut commands = VecDeque::new();

        let error_state = self.state.read().unwrap().error_state;

        match response {
            ProtocolResult::Good(value) => match (value, command) {
                (StateId(state_id), ClientCommand::Init) => {
                    {
                        let mut state = self.state.write().unwrap();
                        state.operations.push_front(Operation {
                            state_id,
                            range: Range::default(),
                        });
                    }

                    log::debug!("Init at state ID {}", state_id);
                }
                (_, ClientCommand::Quit) => {}
                (_, ClientCommand::Previous) => {
                    let old_op = {
                        let mut state = self.state.write().unwrap();
                        let old_op = state.operations.pop_front();
                        state.last_error_range = None;
                        state.error_state = ErrorState::Ok;
                        old_op
                    };
                    match old_op {
                        Some(Operation { range, .. }) => {
                            commands.push_back(DisplayCommand::RemoveProcessed(range));
                            commands.push_back(DisplayCommand::RemoveToBeProcessed(range));
                            commands.push_back(DisplayCommand::RemoveAxiom(range));
                        }
                        _ => {}
                    }
                    commands.push_back(DisplayCommand::RefreshErrorRange(None, true));
                    commands.push_back(DisplayCommand::GotoTip);

                    log::info!("Popped last state from processed ones");
                }
                (_, ClientCommand::BackTo(Operation { state_id, .. })) => {
                    let to_remove = {
                        let mut state = self.state.write().unwrap();
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
                        commands.push_back(DisplayCommand::RefreshErrorRange(None, false));
                        commands.push_back(DisplayCommand::ColorResult(
                            ProtocolRichPP::RichPP(vec![]),
                            false,
                        ));
                    }
                    for op in to_remove {
                        commands.push_back(DisplayCommand::RemoveProcessed(op.range));
                        commands.push_back(DisplayCommand::RemoveToBeProcessed(op.range));
                        commands.push_back(DisplayCommand::RemoveAxiom(op.range));
                    }
                }
                (_, _) if error_state != ErrorState::Ok => {}
                (
                    Pair(box StateId(state_id), box Pair(box union, _)),
                    ClientCommand::Next(append, range, _),
                ) => {
                    let new_state_id = match union {
                        Inl(box Unit) => state_id,
                        Inr(box StateId(state_id)) => state_id,
                        _ => unreachable!(),
                    };

                    {
                        let mut state = self.state.write().unwrap();
                        state.operations.push_front(Operation {
                            state_id: new_state_id,
                            range,
                        });
                    }

                    commands.push_back(DisplayCommand::ColorResult(
                        ProtocolRichPP::RichPP(vec![]),
                        append,
                    ));
                    commands.push_back(DisplayCommand::RefreshErrorRange(None, false));
                    // commands.push_back(DisplayCommand::AddToProcessed(range));
                }
                (Optional(None), ClientCommand::ShowGoals(_)) => {
                    commands.push_back(DisplayCommand::OutputGoals(vec![], vec![], vec![], vec![]));
                }
                (Optional(Some(box Goals(fg, bg, sg, gg))), ClientCommand::ShowGoals(_)) => {
                    commands.push_back(DisplayCommand::OutputGoals(fg, bg, gg, sg));
                }
                (Status(box List(path), box Optional(proof), _, _), ClientCommand::Status) => {
                    let path = path
                        .into_iter()
                        .map(|v| match v {
                            Str(str) => str,
                            _ => panic!(),
                        })
                        .collect::<Vec<_>>();

                    let proof_name = match proof {
                        Some(box Str(proof_name)) => proof_name,
                        None => "".to_string(),
                        _ => "?".to_string(),
                    };

                    commands.push_back(DisplayCommand::ShowStatus(path.join("."), proof_name));
                }
                (_, _) => {}
            },
            ProtocolResult::Fail(_, _, StateId(safe_state_id), message) => match command {
                ClientCommand::Init => todo!(),
                ClientCommand::Query(_) => todo!(),
                ClientCommand::Next(_, range, _)
                | ClientCommand::ShowGoals(range)
                | ClientCommand::BackTo(Operation { range, .. }) => {
                    if safe_state_id > 0 {
                        self.discard_states_until(safe_state_id, &mut commands)
                            .await?;
                    }
                    self.handle_error(Some(range), message, false, &mut commands)
                        .await?;
                }
                c => {
                    log::error!("Unknown failing command {:?}", c);
                }
            },
            ProtocolResult::Fail(_, _, _, _) => unreachable!(),
            ProtocolResult::Feedback(_, _, _, _) => unreachable!(),
        }

        Ok(commands)
    }

    // ----------------------------

    async fn handle_error(
        &mut self,
        error_range: Option<Range>,
        message: ProtocolRichPP,
        append: bool,
        commands: &mut VecDeque<DisplayCommand>,
    ) -> io::Result<()> {
        let safe_state = {
            let mut state = self.state.write().unwrap();
            state.error_state = ErrorState::Error;
            state.last_error_range = error_range.clone();

            state.operations.front().cloned()
        };

        log::debug!("Handling error with range begin {:?}", error_range);

        if let Some(op) = safe_state {
            self.command_tx
                .send(ClientCommand::BackTo(op.clone()))
                .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
        }

        commands.push_back(DisplayCommand::ColorResult(message.error(), append));

        if let Some(range) = error_range.clone() {
            log::debug!("Removing spurious range");

            commands.push_back(DisplayCommand::RemoveToBeProcessed(range));
            commands.push_back(DisplayCommand::RemoveProcessed(range));
            commands.push_back(DisplayCommand::RemoveAxiom(range));
            commands.push_back(DisplayCommand::RefreshErrorRange(error_range, true));
        }

        Ok(())
    }

    async fn discard_states_until(
        &mut self,
        state_id: i64,
        commands: &mut VecDeque<DisplayCommand>,
    ) -> io::Result<()> {
        let mut operations = {
            let mut state = self.state.write().unwrap();
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

            commands.push_back(DisplayCommand::RemoveProcessed(op.range));
            commands.push_back(DisplayCommand::RemoveToBeProcessed(op.range));
            commands.push_back(DisplayCommand::RemoveAxiom(op.range));
        }

        Ok(())
    }

    fn find_range(&mut self, state_id: i64) -> Option<Range> {
        let state = self.state.read().unwrap();
        let len = state.operations.len();
        let mut i = 0usize;

        while i < len {
            if let Some(op) = state.operations.get(i) {
                if op.state_id == state_id {
                    return Some(op.range);
                }
            }
            i += 1;
        }

        None
    }
}

use std::{
    io,
    path::Path,
    sync::{Arc, Mutex},
};

use tokio::fs::{File, OpenOptions};
use tokio::sync::{mpsc, watch};

use crate::{
    client::commands::types::{ClientCommand, DisplayCommand},
    coqtop::xml_protocol::types::{FeedbackContent, MessageType},
    files::result_file,
    range::Range,
    session::{temporary_folder, Session},
    state::{Operation, State},
};

use super::xml_protocol::types::{ProtocolResult, ProtocolRichPP, ProtocolValue};

pub struct ResponseProcessor {
    coqtop_response_rx: mpsc::UnboundedReceiver<ProtocolResult>,
    command_display_tx: mpsc::UnboundedSender<DisplayCommand>,
    state: Arc<Mutex<State>>,
}

impl ResponseProcessor {
    pub fn new(
        _session: Arc<Session>,
        coqtop_response_rx: mpsc::UnboundedReceiver<ProtocolResult>,
        command_display_tx: mpsc::UnboundedSender<DisplayCommand>,
        state: Arc<Mutex<State>>,
    ) -> Self {
        Self {
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

        match resp {
            Good(value) => {
                let (last_command, error_state) = {
                    let mut state = self.state.lock().unwrap();
                    (state.waiting.pop_back(), state.last_error.clone())
                };

                match (value, last_command) {
                    (StateId(state_id), Some(ClientCommand::Init)) => {
                        let mut state = self.state.lock().unwrap();
                        state.operations.push_front(Operation {
                            state_id,
                            range: Range::default(),
                        });
                        state.continue_processing();

                        log::info!("Init at state ID {}", state_id);
                    }
                    (_, Some(ClientCommand::Quit)) => {}
                    (_, Some(ClientCommand::Previous)) => {
                        {
                            let mut state = self.state.lock().unwrap();
                            state.operations.pop_front();
                            state.last_error = None;
                            state.continue_processing();
                        }

                        log::info!("Popped last state from processed ones");

                        // TODO: refresh processed range
                        // TODO: remove error range
                    }
                    _ if error_state.is_some() => {}
                    (
                        Pair(box StateId(state_id), box Pair(box union, _)),
                        Some(ClientCommand::Next(range, code)),
                    ) => {
                        let new_state_id = match union {
                            Inl(box Unit) => state_id,
                            Inr(box StateId(state_id)) => state_id,
                            _ => unreachable!(),
                        };
                        {
                            let mut state = self.state.lock().unwrap();
                            state.operations.push_front(Operation {
                                state_id: new_state_id,
                                range,
                            });
                            state.continue_processing();
                        }

                        log::debug!("Added code to processed operations");

                        // TODO: refresh processed range at some point, maybe on the feedback message
                    }
                    _ => todo!(),
                }
            }
            Fail(_, _, _, _) => todo!(),
            Feedback(_, _, StateId(state_id), content) => match content {
                FeedbackContent::Message(message_type, message) => match message_type {
                    MessageType::Notice | MessageType::Info => {
                        self.send_command(DisplayCommand::ColorResult(message))
                            .await?;
                    }
                    _ => log::error!("[{}] {}", state_id, message.strip()),
                },
                _ => log::debug!("Received feedback object [{}] {:?}", state_id, content),
            },
            _ => {}
        }

        Ok(())
    }

    // ---------------------------------

    async fn send_command(&mut self, command: DisplayCommand) -> io::Result<()> {
        self.command_display_tx
            .send(command)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }
}

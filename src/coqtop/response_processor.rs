use std::{
    io,
    path::Path,
    sync::{Arc, Mutex},
};

use combine::parser::choice::Optional;
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
                    _ if error_state.is_some() => {
                        // TODO: empty incoming queue (need a reference to it)
                        // TODO: report error range
                        // TODO: continue processing
                    }
                    (
                        Pair(box StateId(state_id), box Pair(box union, _)),
                        Some(ClientCommand::Next(range, code)),
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
                            state.continue_processing();
                        }
                        self.send_command(DisplayCommand::AddToProcessed(range))
                            .await?;

                        log::debug!("Added code to processed operations");
                    }
                    (Optional(None), Some(ClientCommand::ShowGoals)) => {
                        {
                            let mut state = self.state.lock().unwrap();
                            state.continue_processing();
                        }
                        self.send_command(DisplayCommand::OutputGoals(vec![], vec![], vec![]))
                            .await?;
                    }
                    (Optional(Some(box Goals(fg, bg, _, gg))), Some(ClientCommand::ShowGoals)) => {
                        {
                            let mut state = self.state.lock().unwrap();
                            state.continue_processing();
                        }
                        self.send_command(DisplayCommand::OutputGoals(fg, bg, gg))
                            .await?;
                    }
                    _ => todo!(),
                }
            }
            Fail(_, _, _, _) => todo!(),
            Feedback(_, _, StateId(state_id), content) => match content {
                FeedbackContent::Message(message_type, message) => match message_type {
                    MessageType::Notice | MessageType::Info => {
                        self.send_command(DisplayCommand::ColorResult(message, true))
                            .await?;
                    }
                    MessageType::Debug => log::debug!("@{}: {}", state_id, message.strip()),
                    _ => log::error!("@{}: {}", state_id, message.strip()),
                },
                FeedbackContent::Processed => {}
                _ => log::debug!("Received feedback object [{}] {:?}", state_id, content),
            },
            _ => {}
        }

        Ok(())
    }

    // ---------------------------------

    fn find_range(&self, state_id: i64) -> Option<Range> {
        let state = self.state.lock().unwrap();
        state
            .operations
            .iter()
            .find(|op| op.state_id == state_id)
            .map(|op| op.range.clone())
    }

    async fn send_command(&mut self, command: DisplayCommand) -> io::Result<()> {
        self.command_display_tx
            .send(command)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }
}

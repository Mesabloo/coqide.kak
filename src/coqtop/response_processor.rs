use std::{
    io,
    sync::{Arc, Mutex},
};

use tokio::sync::{mpsc, watch};

use crate::{
    client::commands::types::{ClientCommand, DisplayCommand},
    range::Range,
    session::Session,
    state::{Operation, State},
};

use super::xml_protocol::types::{ProtocolResult, ProtocolValue};

pub struct ResponseProcessor {
    coqtop_response_rx: mpsc::UnboundedReceiver<ProtocolResult>,
    command_display_tx: mpsc::UnboundedSender<DisplayCommand>,
    state: Arc<Mutex<State>>,
}

impl ResponseProcessor {
    pub fn new(
        session: Arc<Session>,
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
                let last_command = self.state.lock().unwrap().waiting.pop_front();

                match (value, last_command) {
                    (StateId(state_id), Some(ClientCommand::Init)) => {
                        let mut state = self.state.lock().unwrap();
                        state.operations.push_back(Operation {
                            state_id,
                            range: Range::default(),
                        });
                        state.continue_processing();

                        log::info!("Init at state ID {}", state_id);
                    }
                    _ => todo!(),
                }
            }
            Fail(_, _, _, _) => todo!(),
            _ => {}
        }

        Ok(())
    }
}

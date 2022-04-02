use std::io;
use std::sync::{Arc, Mutex};

use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_util::codec::FramedRead;

use crate::coqtop::xml_protocol::types::{ProtocolCall, ProtocolValue};
use crate::kakoune::command_line::kak;
use crate::range::Range;
use crate::session::{edited_file, input_fifo, session_id, Session};
use crate::state::{ErrorState, Operation, State};

use super::commands::decode::{command_decoder, CommandDecoder};
use super::commands::types::{ClientCommand, DisplayCommand};

pub struct ClientInput {
    cmd_disp_tx: mpsc::UnboundedSender<DisplayCommand>,
    coqtop_call_tx: mpsc::UnboundedSender<ProtocolCall>,
    stop_tx: watch::Sender<()>,
    reader: FramedRead<UnixStream, CommandDecoder>,
    pub command_tx: broadcast::Sender<ClientCommand>,
    command_rx: broadcast::Receiver<ClientCommand>,
    state: Arc<Mutex<State>>,
}

impl ClientInput {
    pub async fn new(
        session: Arc<Session>,
        cmd_disp_tx: mpsc::UnboundedSender<DisplayCommand>,
        coqtop_call_tx: mpsc::UnboundedSender<ProtocolCall>,
        stop_tx: watch::Sender<()>,
        state: Arc<Mutex<State>>,
    ) -> io::Result<Self> {
        let sess_id = session_id(session.clone());

        let pipe_listener = UnixListener::bind(input_fifo(session.clone()))?;
        let init_kakoune = kak(
            &sess_id,
            format!(
                r#"evaluate-commands -buffer '{}' %{{ coqide-init }}"#,
                edited_file(session.clone())
            ),
        );
        let (res1, res2) = tokio::join!(init_kakoune, pipe_listener.accept());
        res1?;
        let (pipe, _) = res2?;

        log::debug!(
            "Connected to UNIX socket at path {}",
            input_fifo(session.clone())
        );

        let (command_tx, command_rx) = broadcast::channel(25);

        Ok(Self {
            cmd_disp_tx,
            coqtop_call_tx,
            stop_tx,
            reader: command_decoder(pipe),
            state,
            command_tx,
            command_rx,
        })
    }

    pub async fn handle_commands_until(&mut self, mut stop: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop.changed() => break Ok(()),
                Ok(cmd) = self.command_rx.recv() => {
                    let (go_further, error_state) = {
                        let state = self.state.lock().unwrap();
                        (state.can_go_further(), state.error_state)
                    };

                    match cmd {
                        ClientCommand::Quit => self.process_command(ClientCommand::Quit, error_state).await?,
                        cmd =>  if go_further && error_state != ErrorState::ClearQueue {
                            self.state.lock().unwrap().stop_processing();
                            self.process_command(cmd, error_state).await?;
                        }
                    }
                }
                cmd = ClientCommand::decode_stream(&mut self.reader) => {
                    match cmd? {
                        None => {
                            log::warn!("Junk byte ignored in stream");
                        },
                        Some(cmd) => {
                            self.command_tx
                                .send(cmd)
                                .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                        }
                    }
                }
                else => {}
            }
        }
    }

    // ----------------------------------------

    async fn process_command(
        &mut self,
        command: ClientCommand,
        error_state: ErrorState,
    ) -> io::Result<()> {
        self.state
            .lock()
            .unwrap()
            .waiting
            .push_back(command.clone());

        match command {
            ClientCommand::Init => self.process_init().await,
            ClientCommand::Quit => self.process_quit().await,
            ClientCommand::Previous => self.process_previous().await,
            ClientCommand::RewindTo(line, col) => self.process_rewind_to(line, col).await,
            ClientCommand::Query(_) => todo!(),
            ClientCommand::MoveTo(_) if error_state == ErrorState::Ok => todo!(),
            ClientCommand::Next(range, code) if error_state == ErrorState::Ok => {
                self.process_next(range, code).await
            }
            ClientCommand::IgnoreError if error_state == ErrorState::Error => {
                self.process_ignore_error().await
            }
            ClientCommand::Hints => todo!(),
            ClientCommand::ShowGoals(_) => self.process_show_goals().await,
            ClientCommand::Next(range, _) => {
                self.state.lock().unwrap().continue_processing();

                self.cmd_disp_tx
                    .send(DisplayCommand::RemoveToBeProcessed(range))
                    .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
            }
            ClientCommand::IgnoreError => {
                self.state.lock().unwrap().continue_processing();
                Ok(())
            }
            ClientCommand::BackTo(state_id) => self.process_back_to(state_id).await,
            _ => todo!(),
        }
    }

    async fn process_init(&mut self) -> io::Result<()> {
        self.send_call(ProtocolCall::Init(ProtocolValue::Optional(None)))
            .await
    }

    async fn process_quit(&mut self) -> io::Result<()> {
        log::info!("Initiating quitting");

        self.send_call(ProtocolCall::Quit).await?;
        self.stop_tx.send(()).unwrap();
        Ok(())
    }

    async fn process_next(&mut self, _range: Range, code: String) -> io::Result<()> {
        let call = match self.state.lock().unwrap().operations.front() {
            None => panic!(),
            Some(Operation { state_id, .. }) => ProtocolCall::Add(code, *state_id),
        };
        self.send_call(call).await?;

        Ok(())
    }

    async fn process_previous(&mut self) -> io::Result<()> {
        let call = match self.state.lock().unwrap().operations.get(1) {
            Some(Operation { state_id, .. }) => Some(ProtocolCall::EditAt(*state_id)),
            None => None,
        };
        match call {
            Some(call) => self.send_call(call).await?,
            None => {
                log::error!(
                    "No earlier operations to rewind to (queue length: {})",
                    self.state.lock().unwrap().operations.len()
                );
            }
        }

        Ok(())
    }

    async fn process_show_goals(&mut self) -> io::Result<()> {
        self.send_call(ProtocolCall::Goal).await
    }

    async fn process_ignore_error(&mut self) -> io::Result<()> {
        {
            let mut state = self.state.lock().unwrap();
            state.last_error_range = None;
            state.error_state = ErrorState::Ok;
            state.continue_processing();
        }

        self.cmd_disp_tx
            .send(DisplayCommand::RefreshErrorRange(None))
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }

    async fn process_back_to(&mut self, op: Operation) -> io::Result<()> {
        self.send_call(ProtocolCall::EditAt(op.state_id)).await
    }

    async fn process_rewind_to(&mut self, line: u64, column: u64) -> io::Result<()> {
        let (op, tip) = {
            let state = self.state.lock().unwrap();
            let op = state
                .operations
                .iter()
                .find(|op| {
                    let range = op.range;
                    range.end.0 < line || (range.end.0 == line && range.end.1 < column)
                })
                .cloned();
            (op, state.operations.front().cloned())
        };
        let (new_state_id, tip_id) = {
            let mut state = self.state.lock().unwrap();

            let op = op.unwrap_or_else(Operation::default);
            let new_id = op.state_id;

            state.waiting.pop_back();
            state.waiting.push_back(ClientCommand::BackTo(op));

            (new_id, tip.map(|op| op.state_id).unwrap_or(1))
        };

        if new_state_id < tip_id {
            self.send_call(ProtocolCall::EditAt(new_state_id)).await
        } else {
            let mut state = self.state.lock().unwrap();
            state.continue_processing();
            Ok(())
        }
    }

    // --------------------------------------

    async fn send_call(&mut self, call: ProtocolCall) -> io::Result<()> {
        log::debug!("Sending Coq call {:?}", call);

        self.coqtop_call_tx
            .send(call)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }
}

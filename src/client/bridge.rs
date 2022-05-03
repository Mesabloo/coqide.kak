use std::{
    io,
    sync::{Arc, RwLock},
};

use tokio::{
    net::{UnixListener, UnixStream},
    sync::{broadcast, watch},
};
use tokio_util::codec::FramedRead;

use crate::{
    client::commands::decode::command_decoder,
    coqtop::xml_protocol::types::{ProtocolCall, ProtocolValue},
    kakoune::command_line::kak,
    range::Range,
    session::{edited_file, input_fifo, session_id, Session},
    state::{ErrorState, Operation, State},
};

use super::commands::{
    decode::CommandDecoder,
    types::{ClientCommand, DisplayCommand},
};

pub struct ClientBridge {
    /// All information required to communicate with the session.
    _session: Arc<Session>,
    /// The global application state.
    state: Arc<RwLock<State>>,
    /// The backdoor for sending [`ClientCommand`]s from inside the daemon.
    pub command_tx: broadcast::Sender<ClientCommand>,
    /// The receiver of the backdoor.
    command_rx: broadcast::Receiver<ClientCommand>,
    /// A reader to decode the incoming stream of commands.
    reader: FramedRead<UnixStream, CommandDecoder>,
    /// Manual ending of the daemon.
    stop_tx: watch::Sender<()>,
}

impl ClientBridge {
    pub async fn new<const SIZE: usize>(
        session: Arc<Session>,
        state: Arc<RwLock<State>>,
        stop_tx: watch::Sender<()>,
    ) -> io::Result<Self> {
        let session_id = session_id(session.clone());
        let unix_listener = UnixListener::bind(input_fifo(session.clone()))?;
        let init_kakoune = kak(
            &session_id,
            format!(
                r#"evaluate-commands -buffer '{}' %{{ coqide-init }}"#,
                edited_file(session.clone())
            ),
        );

        let (r1, r2) = tokio::join!(init_kakoune, unix_listener.accept());
        let (_, (pipe, _)) = (r1?, r2?);

        log::debug!("Connected to FIFO at path {}", input_fifo(session.clone()));

        let (command_tx, command_rx) = broadcast::channel(SIZE);

        Ok(Self {
            _session: session,
            state,
            command_tx,
            command_rx,
            reader: command_decoder(pipe),
            stop_tx,
        })
    }

    /// Wait until a command is received, either through the unix socket or the backdoor channel.
    pub async fn recv(&mut self, _stop: watch::Receiver<()>) -> io::Result<ClientCommand> {
        loop {
            tokio::select! {
                biased; // We want to always favor receiving from the internal channel.

                Ok(cmd) = self.command_rx.recv() => break Ok(cmd),
                Ok(cmd) = ClientCommand::decode_stream(&mut self.reader) => {
                    match cmd {
                        None => log::warn!("Junk found in stream"),
                        Some(cmd) => break Ok(cmd),
                    }
                }
            }
        }
    }

    pub async fn process(
        &mut self,
        command: ClientCommand,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        let error_state = self.state.read().unwrap().error_state;

        match command {
            ClientCommand::StopInterrupt => self.process_stop_interrupt(),
            ClientCommand::Init => self.process_init(),
            ClientCommand::Quit => self.process_quit(),
            ClientCommand::Previous if error_state != ErrorState::Interrupted => {
                self.process_previous()
            }
            ClientCommand::RewindTo(line, column) if error_state != ErrorState::Interrupted => {
                self.process_rewind_to(line, column)
            }
            ClientCommand::Query(_) => todo!(),
            ClientCommand::MoveTo(ranges) if error_state == ErrorState::Ok => {
                self.process_move_to(ranges)
            }
            ClientCommand::Next(append, range, code) if error_state == ErrorState::Ok => {
                self.process_next(append, range, code)
            }
            ClientCommand::IgnoreError if error_state == ErrorState::Error => {
                self.process_ignore_error()
            }
            ClientCommand::Hints => todo!(),
            ClientCommand::ShowGoals(range) if error_state != ErrorState::Interrupted => {
                self.process_show_goals(range)
            }
            ClientCommand::BackTo(op) => self.process_back_to(op),
            ClientCommand::Next(append, range, code) => Ok((
                None,
                ClientCommand::Next(append, range, code),
                vec![DisplayCommand::RemoveToBeProcessed(range)],
            )),
            ClientCommand::Status => self.process_status(),
            c => Ok((None, c, vec![])),
        }
    }

    // -----------------

    fn process_init(
        &self,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        Ok((
            Some(ProtocolCall::Init(ProtocolValue::Optional(None))),
            ClientCommand::Init,
            vec![],
        ))
    }

    fn process_quit(
        &mut self,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        log::info!("Beginning quitting");

        self.stop_tx.send(()).unwrap();
        Ok((Some(ProtocolCall::Quit), ClientCommand::Quit, vec![]))
    }

    fn process_next(
        &mut self,
        append: bool,
        range: Range,
        code: String,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        let call = match self.state.read().unwrap().operations.front() {
            Some(Operation { state_id, .. }) => Some(ProtocolCall::Add(code.clone(), *state_id)),
            None => None,
        };
        Ok((call, ClientCommand::Next(append, range, code), vec![]))
    }

    fn process_show_goals(
        &mut self,
        range: Range,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        Ok((
            Some(ProtocolCall::Goal),
            ClientCommand::ShowGoals(range),
            vec![],
        ))
    }

    fn process_previous(
        &mut self,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        let call = match self.state.read().unwrap().operations.get(1) {
            Some(Operation { state_id, .. }) => Some(ProtocolCall::EditAt(*state_id)),
            None => {
                log::warn!("No earlier operation to go back to (this might be a case where you are trying to rollback on buffer start)");
                None
            }
        };
        Ok((call, ClientCommand::Previous, vec![]))
    }

    fn process_rewind_to(
        &mut self,
        line: u64,
        column: u64,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        let (op, tip) = {
            let state = self.state.read().unwrap();
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
        let (new_state_id, tip_id, op) = {
            let op = op.unwrap_or_else(Operation::default);
            let new_id = op.state_id;

            (new_id, tip.map(|op| op.state_id).unwrap_or(1), op)
        };

        Ok((
            if new_state_id < tip_id {
                Some(ProtocolCall::EditAt(new_state_id))
            } else {
                None
            },
            ClientCommand::BackTo(op),
            vec![],
        ))
    }

    fn process_back_to(
        &mut self,
        op: Operation,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        Ok((
            Some(ProtocolCall::EditAt(op.state_id)),
            ClientCommand::BackTo(op),
            vec![],
        ))
    }

    fn process_ignore_error(
        &mut self,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        {
            let mut state = self.state.write().unwrap();
            state.last_error_range = None;
            state.error_state = ErrorState::Ok;
        }

        Ok((
            None,
            ClientCommand::IgnoreError,
            vec![DisplayCommand::RefreshErrorRange(None, true)],
        ))
    }

    fn process_status(
        &mut self,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        Ok((
            Some(ProtocolCall::Status(ProtocolValue::Boolean(false))),
            ClientCommand::Status,
            vec![],
        ))
    }

    fn process_move_to(
        &mut self,
        ranges: Vec<(Range, String)>,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        let mut should_append = false;
        for (range, code) in ranges.iter() {
            self.command_tx
                .send(ClientCommand::Next(
                    should_append,
                    range.clone(),
                    code.clone(),
                ))
                .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
            should_append = true;
        }

        Ok((None, ClientCommand::MoveTo(ranges), vec![]))
    }

    fn process_stop_interrupt(
        &mut self,
    ) -> io::Result<(Option<ProtocolCall>, ClientCommand, Vec<DisplayCommand>)> {
        self.state.write().unwrap().error_state = ErrorState::Ok;

        Ok((None, ClientCommand::StopInterrupt, vec![]))
    }
}

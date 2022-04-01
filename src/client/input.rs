use std::io;
use std::sync::{Arc, Mutex};

use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, watch};
use tokio_util::codec::FramedRead;

use crate::coqtop::xml_protocol::types::{ProtocolCall, ProtocolValue};
use crate::kakoune::command_line::kak;
use crate::range::Range;
use crate::session::{edited_file, input_fifo, session_id, Session};
use crate::state::{Operation, State};

use super::commands::decode::{command_decoder, CommandDecoder};
use super::commands::types::{ClientCommand, DisplayCommand};

pub struct ClientInput {
    cmd_disp_tx: mpsc::UnboundedSender<DisplayCommand>,
    coqtop_call_tx: mpsc::UnboundedSender<ProtocolCall>,
    stop_tx: watch::Sender<()>,
    reader: FramedRead<UnixStream, CommandDecoder>,
    command_tx: mpsc::UnboundedSender<ClientCommand>,
    command_rx: mpsc::UnboundedReceiver<ClientCommand>,
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

        let (command_tx, command_rx) = mpsc::unbounded_channel();

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
                            Some(cmd) = self.command_rx.recv(), if self.state.lock().unwrap().can_go_further() => {
                                self.state.lock().unwrap().stop_processing();
                                self.process_command(cmd).await?;
                            }
                            cmd = ClientCommand::decode_stream(&mut self.reader) => {
                                match cmd? {
                                    None => {
                                        log::warn!("Junk byte ignored in stream");
                                    },
                                    Some(cmd) => {
            /*                            match &cmd {
                                            ClientCommand::Next(range, _) => {
                                                self.cmd_disp_tx
                                                    .send(DisplayCommand::PushToBeProcessedRange(range.clone()))
                                                    .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                                            }
                                            _ => {}
                                        }
            */
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

    async fn process_command(&mut self, command: ClientCommand) -> io::Result<()> {
        self.state
            .lock()
            .unwrap()
            .waiting
            .push_back(command.clone());

        match command {
            ClientCommand::Init => self.process_init().await,
            ClientCommand::Quit => self.process_quit().await,
            ClientCommand::Previous => self.process_previous().await,
            ClientCommand::RewindTo(_, _) => todo!(),
            ClientCommand::Query(_) => todo!(),
            ClientCommand::MoveTo(_) => todo!(),
            ClientCommand::Next(range, code) => self.process_next(range, code).await,
            ClientCommand::IgnoreError => todo!(),
            ClientCommand::Hints => todo!(),
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

    async fn process_next(&mut self, range: Range, code: String) -> io::Result<()> {
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
            None => {}
        }

        Ok(())
    }

    // --------------------------------------

    async fn send_call(&mut self, call: ProtocolCall) -> io::Result<()> {
        self.coqtop_call_tx
            .send(call)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }
}

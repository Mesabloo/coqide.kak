use std::io;
use std::sync::{Arc, Mutex};

use tokio::fs::File;
use tokio::io::{stdin, Stdin};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;

use crate::coqtop::xml_protocol::types::{ProtocolCall, ProtocolValue};
use crate::kakoune::command_line::kak;
use crate::session::{edited_file, input_fifo, session_id, Session};
use crate::state::State;

use super::commands::decode::{command_decoder, CommandDecoder};
use super::commands::types::ClientCommand;

pub struct ClientInput {
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
                cmd = self.reader.next() => {
                    log::debug!("{:?}", self.reader.read_buffer());

                    if let Some(cmd) = cmd {
                        match cmd? {
                            None => {
                                log::warn!("Junk byte ignored in stream");
                            },
                            Some(cmd) => {
                                self.command_tx.send(cmd).map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                            }
                        }
                    }
                }
                Some(cmd) = self.command_rx.recv(), if self.state.lock().unwrap().can_go_further() => {
                    self.state.lock().unwrap().stop_processing();
                    self.process_command(cmd).await?;
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
            ClientCommand::Previous => todo!(),
            ClientCommand::RewindTo(_, _) => todo!(),
            ClientCommand::Query(_) => todo!(),
            ClientCommand::MoveTo(_) => todo!(),
            ClientCommand::Next(_, _) => todo!(),
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

    // --------------------------------------

    async fn send_call(&mut self, call: ProtocolCall) -> io::Result<()> {
        self.coqtop_call_tx
            .send(call)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }
}

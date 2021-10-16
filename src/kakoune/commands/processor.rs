use std::{
    borrow::{Borrow, BorrowMut},
    cell::RefCell,
    future::Future,
    io,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::{
        atomic::{self, AtomicBool},
        Arc, Mutex,
    },
};

use async_signals::Signals;
use tokio::{
    fs::File,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};
use tokio_stream::StreamExt;

use crate::{
    coqtop::{
        slave::IdeSlave,
        xml_protocol::types::{ProtocolCall, ProtocolResult, ProtocolRichPP, ProtocolValue},
    },
    kakoune::{
        session::SessionWrapper,
        slave::{command_file, KakSlave},
    },
};

use super::types::Command;

pub struct CommandProcessor {
    session: Arc<SessionWrapper>,
    kakslave: Arc<KakSlave>,

    todo_rx: mpsc::UnboundedReceiver<Command>,
    todo_tx_handle: JoinHandle<io::Result<()>>,
}

async fn receive_from_pipe(
    session: Arc<SessionWrapper>,
    todo_tx: mpsc::UnboundedSender<Command>,
    mut must_run_rx: watch::Receiver<()>,
) -> io::Result<()> {
    let mut signals = Signals::new(vec![libc::SIGUSR1]).unwrap();
    let mut pipe = File::from(unix_named_pipe::open_read(command_file(session.clone()))?);
    log::debug!("Command pipe opened");

    'global_handler: loop {
        tokio::select! {
            Ok(_) = must_run_rx.changed() => return Ok(()),
            Some(_) = signals.next() => {
                log::debug!("Received a SIGUSR1. Trying to process the next command");

                match receive_commands(todo_tx.clone(), &mut pipe).await? {
                    None => break 'global_handler Ok(()),
                    Some(_) => {}
                }
            }
        }
    }
}

async fn receive_commands(
    todo_tx: mpsc::UnboundedSender<Command>,
    pipe: &mut File,
) -> io::Result<Option<()>> {
    loop {
        match Command::parse_from(pipe).await? {
            None => break Ok(None),
            Some(None) => {}
            Some(Some(cmd)) => {
                log::debug!("Command '{:?}' sent through internal channel", cmd);

                todo_tx.send(cmd).map_err(|err| {
                    io::Error::new(
                        io::ErrorKind::NotConnected,
                        format!("Could not send command to channel: {:?}", err),
                    )
                })?;

                break Ok(Some(()));
            }
        }
    }
}

impl CommandProcessor {
    pub fn new(
        session: Arc<SessionWrapper>,
        kakslave: Arc<KakSlave>,
        must_run_rx: watch::Receiver<()>,
    ) -> io::Result<Self> {
        let (todo_tx, todo_rx) = mpsc::unbounded_channel();
        let todo_tx_handle = tokio::spawn(receive_from_pipe(
            session.clone(),
            todo_tx,
            must_run_rx,
        ));

        Ok(Self {
            session: session.clone(),
            kakslave,
            todo_rx,
            todo_tx_handle,
        })
    }

    pub async fn start(&mut self, mut must_run_rx: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = must_run_rx.changed() => return Ok(()),
                cmd = self.todo_rx.recv() => {
                    match cmd {
                        None => {
                            log::warn!("Channel closed: no more commands can be received");
                            return Ok(());
                        }
                        Some(cmd) => {
                            log::debug!("Received command `{:?}` from internal channel", cmd);

                            self.try_process_command(cmd).await?;
                        }
                    }
                }
            }
        }
    }

    pub async fn stop(&mut self) -> io::Result<()> {
        self.todo_rx.close();

        Ok(())
    }

    // pub async fn process_next_command(&mut self) -> io::Result<()> {
    //     loop {
    //         match Command::parse_from(&mut self.command_file).await? {
    //             None => break Err(io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe")),
    //             Some(None) => {}
    //             Some(Some(cmd)) => break self.try_process_command(cmd).await,
    //         }
    //     }
    // }

    async fn try_process_command(&mut self, cmd: Command) -> io::Result<()> {
        log::debug!("Processing command `{:?}`", cmd);

        let response = self
            .kakslave
            .ideslave
            .write()
            .await
            .send(self.command_to_call(&cmd))
            .await?;

        match cmd {
            Command::Init => self.when_good(response, Self::handle_init).await?,
            _ => todo!(),
        }
        Ok(())
    }

    fn command_to_call(&self, cmd: &Command) -> ProtocolCall {
        match cmd {
            Command::Init => ProtocolCall::Init(ProtocolValue::Optional(None)),
            _ => todo!(),
        }
    }

    async fn when_good<'a, 'b: 'a, F, T>(&'b mut self, resp: ProtocolResult, f: F) -> io::Result<()>
    where
        F: Fn(&'a mut Self, ProtocolValue) -> T,
        T: Future<Output = io::Result<()>>,
    {
        match resp {
            ProtocolResult::Fail(line, col, msg) => self.fail(line, col, msg).await,
            ProtocolResult::Feedback(_, _, _, _) => self.unexpected_response(resp).await,
            ProtocolResult::Good(val) => f(self, val).await,
        }
    }

    async fn fail(
        &mut self,
        line: Option<i64>,
        col: Option<i64>,
        msg: ProtocolRichPP,
    ) -> io::Result<()> {
        todo!()
    }

    async fn unexpected_response(&mut self, resp: ProtocolResult) -> io::Result<()> {
        todo!()
    }

    async fn handle_init(&mut self, val: ProtocolValue) -> io::Result<()> {
        match val {
            ProtocolValue::StateId(state_id) => {
                {
                    let mut state = self.kakslave.ext_state.write().await;
                    state.set_root_id(state_id);
                    state.set_current_id(state_id);
                }
                Ok(())
            }
            val => {
                log::error!("Init: Unexpected good value '{:?}'. Ignoring command", val);
                Ok(())
            }
        }
    }
}

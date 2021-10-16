use std::{
    cell::RefCell,
    future::Future,
    io,
    rc::Rc,
    sync::{
        atomic::{self, AtomicBool},
        Arc,
    },
};

use signal_hook::{consts::SIGUSR1, iterator::Signals};
use tokio::{
    fs::File,
    sync::{mpsc, RwLock},
    task::JoinHandle,
};

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

    todo_rx: Arc<RwLock<mpsc::UnboundedReceiver<Command>>>,
    todo_tx_handle: Arc<JoinHandle<io::Result<()>>>,

    running: Arc<AtomicBool>,
}

impl CommandProcessor {
    pub fn new(session: Arc<SessionWrapper>, kakslave: Arc<KakSlave>) -> io::Result<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let running_ = running.clone();

        let session_ = session.clone();

        let (todo_tx, todo_rx) = mpsc::unbounded_channel();
        let todo_tx_handle = Arc::new(tokio::spawn(async move {
            let mut signals = Signals::new(&[SIGUSR1])?;
            let mut pipe = File::from(unix_named_pipe::open_read(command_file(session_.clone()))?);
            log::debug!("Command pipe opened");

            let handle = signals.handle();

            let mut res: io::Result<()> = Ok(());

            'global_handler: while running_.load(atomic::Ordering::Relaxed) {
                signals.wait().next(); // wait until a SIGUSR1 has been received
                log::debug!("Received a SIGUSR1. Trying to process the next command");

                while running_.load(atomic::Ordering::Relaxed) {
                    match Command::parse_from(&mut pipe).await? {
                        None => {
                            res = Ok(()); // Err(io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe")),
                            break 'global_handler;
                        }
                        Some(None) => {}
                        Some(Some(cmd)) => {
                            todo_tx.send(cmd).map_err(|err| {
                                io::Error::new(
                                    io::ErrorKind::NotConnected,
                                    format!("Could nbot send command to channel: {:?}", err),
                                )
                            })?;

                            log::debug!("Command sent through internal channel");

                            break;
                        }
                    }
                }
            }

            handle.close();

            res
        }));

        Ok(Self {
            session: session.clone(),
            kakslave,
            todo_rx: Arc::new(RwLock::new(todo_rx)),
            todo_tx_handle,
            running,
        })
    }

    pub async fn start(&mut self) -> io::Result<()> {
        while self.running.load(atomic::Ordering::Relaxed) {
            log::debug!("Trying to receive commands from internal channel...");

            let cmd = { self.todo_rx.write().await.recv().await };
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

        Ok(())
    }

    pub async fn stop(&mut self) -> io::Result<()> {
        self.running.store(false, atomic::Ordering::Relaxed);

        self.todo_rx.write().await.close();
        (&self.todo_tx_handle).abort();

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

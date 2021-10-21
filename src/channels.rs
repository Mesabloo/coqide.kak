use tokio::{
    fs::File,
    io::{self, AsyncWriteExt},
    net::{TcpStream, UnixListener},
    sync::{mpsc, watch},
};

use crate::{
    coqtop::xml_protocol::types::{ProtocolCall, ProtocolResult, ProtocolValue},
    files::{command_file, goal_file, result_file, COQTOP},
    kakoune::{command_line::kak, commands::types::Command},
};

pub struct CommandReceiver {
    pipe_tx: mpsc::UnboundedSender<Command>,
    stop_rx: watch::Receiver<()>,
}

impl CommandReceiver {
    pub fn new(pipe_tx: mpsc::UnboundedSender<Command>, stop_rx: watch::Receiver<()>) -> Self {
        Self { pipe_tx, stop_rx }
    }

    pub async fn process(
        &mut self,
        kak_session: String,
        tmp_dir: String,
        coq_file: String,
    ) -> io::Result<()> {
        let pipe_listener = UnixListener::bind(command_file(&tmp_dir))?;

        log::debug!("Binding unix socket in /tmp directory");

        // Populate file descriptor 3 with connection to unix socket
        let populate_fd = kak(
            &kak_session,
            format!(
                r#"evaluate-commands -buffer '{0}' %{{ coqide-populate-fd4 }}
                evaluate-commands -buffer '{0}' %{{ edit! -scratch "%opt{{coqide_result_buffer}}" }}
                evaluate-commands -buffer '{0}' %{{ edit! -scratch "%opt{{coqide_goal_buffer}}" }}"#,
                coq_file
            ),
        );
        let (kak_res, pipe) = tokio::join!(populate_fd, pipe_listener.accept());
        kak_res?;
        let mut pipe = pipe?.0;

        log::debug!("Successfully opened unix socket");

        let res = loop {
            tokio::select! {
                Ok(_) = self.stop_rx.changed() => break Ok::<_, io::Error>(()),
                Ok(cmd) = Command::parse_from(&mut pipe) => {
                    if let Some(cmd) = cmd.flatten() {
                        log::debug!("Received kakoune command '{:?}'", cmd);

                        self.pipe_tx.send(cmd)
                            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                    }
                }
            }
        };

        drop(pipe);

        res
    }

    pub async fn stop(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct CommandProcessor {
    pipe_rx: mpsc::UnboundedReceiver<Command>,
    main_w: TcpStream,
    stop_rx: watch::Receiver<()>,
}

impl CommandProcessor {
    pub fn new(
        pipe_rx: mpsc::UnboundedReceiver<Command>,
        main_w: TcpStream,
        stop_rx: watch::Receiver<()>,
    ) -> Self {
        Self {
            pipe_rx,
            main_w,
            stop_rx,
        }
    }

    pub async fn process(&mut self) -> io::Result<()> {
        log::debug!("Initialising {}", COQTOP);
        let init = ProtocolCall::Init(ProtocolValue::Optional(None));
        self.main_w.write_all(init.encode().as_bytes()).await?;

        loop {
            tokio::select! {
                Ok(_) = self.stop_rx.changed() => break Ok::<_, io::Error>(()),
                Some(cmd) = self.pipe_rx.recv() => {
                    let call = tokio::task::block_in_place(|| {
                        match cmd {
                            Command::Init => ProtocolCall::Init(ProtocolValue::Optional(None)),
                            Command::Query(str) => ProtocolCall::Query(ProtocolValue::Str(str)), // FIXME: tests
                            _ => todo!(),
                        }
                    });

                    log::debug!("Sending call '{:?}' to {}", call, COQTOP);

                    self.main_w.write_all(call.encode().as_bytes()).await?;
                }
            }
        }
    }

    pub async fn stop(&mut self) -> io::Result<()> {
        self.main_w.shutdown().await?;

        Ok(())
    }
}

pub struct ResponseReceiver {
    result_tx: mpsc::UnboundedSender<ProtocolResult>,
    main_r: TcpStream,
    stop_rx: watch::Receiver<()>,
}

impl ResponseReceiver {
    pub fn new(
        result_tx: mpsc::UnboundedSender<ProtocolResult>,
        main_r: TcpStream,
        stop_rx: watch::Receiver<()>,
    ) -> Self {
        Self {
            result_tx,
            main_r,
            stop_rx,
        }
    }

    pub async fn process(&mut self) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = self.stop_rx.changed() => break Ok::<_, io::Error>(()),
                Ok(resp) = ProtocolResult::decode_stream(&mut self.main_r) => {
                    log::debug!("Received response '{:?}' from {}", resp, COQTOP);

                    self.result_tx.send(resp)
                        .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                }
            }
        }
    }

    pub async fn stop(&mut self) -> io::Result<()> {
        self.main_r.shutdown().await?;

        Ok(())
    }
}

pub struct ResponseProcessor {
    result_rx: mpsc::UnboundedReceiver<ProtocolResult>,
    stop_rx: watch::Receiver<()>,
    result_file: File,
    goal_file: File,
    tmp_dir: String,
    kak_session: String,
    coq_file: String,
}

impl ResponseProcessor {
    pub async fn new(
        result_rx: mpsc::UnboundedReceiver<ProtocolResult>,
        stop_rx: watch::Receiver<()>,
        tmp_dir: &String,
        kak_session: &String,
        coq_file: &String,
    ) -> io::Result<Self> {
        let result_file = File::open(result_file(tmp_dir)).await?;
        let goal_file = File::open(goal_file(tmp_dir)).await?;

        Ok(Self {
            result_rx,
            stop_rx,
            result_file,
            goal_file,
            tmp_dir: tmp_dir.clone(),
            kak_session: kak_session.clone(),
            coq_file: coq_file.clone(),
        })
    }

    pub async fn process(&mut self) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = self.stop_rx.changed() => break Ok::<_, io::Error>(()),
                Some(resp) = self.result_rx.recv() => match resp {
                    ProtocolResult::Fail(_, _, _) => {}
                    ProtocolResult::Good(state_id) => {
                        match state_id {
                            ProtocolValue::StateId(state_id) => {}
                            val => {
                                log::warn!("Unhandled response 'Good({:?})'", val);
                            }
                        }
                    }
                    ProtocolResult::Feedback(_, _, _, richpp) => {
                        self.result_file.set_len(0).await?;
                        self.result_file.write_all(format!(">>> {:?}", richpp).as_bytes()).await?;
                        self.update_result_buffer().await?;
                    }
                }
            }
        }
    }

    pub async fn stop(&mut self) -> io::Result<()> {
        self.result_file.shutdown().await?;
        self.goal_file.shutdown().await?;

        Ok(())
    }

    async fn update_result_buffer(&self) -> io::Result<()> {
        kak(
            &self.kak_session,
            format!(
                r#"execute-keys -buffer '{}' %{{
                  %|cat<space>{}<ret>
                }}"#,
                self.coq_file,
                result_file(&self.tmp_dir),
            ),
        )
        .await
    }

    async fn update_goal_buffer(&self) -> io::Result<()> {
        kak(
            &self.kak_session,
            format!(
                r#"execute-keys -buffer '{}' %{{
                  %|cat<space>{}<ret>
                }}"#,
                self.coq_file,
                goal_file(&self.tmp_dir),
            ),
        )
        .await
    }
}

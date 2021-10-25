use std::{
    io::{self, SeekFrom},
    sync::{Arc, RwLock},
};

use tokio::{
    fs::File,
    io::{AsyncSeekExt, AsyncWriteExt},
    join,
    net::{TcpListener, TcpStream},
    process::{Child, Command},
    sync::{mpsc, watch},
};

use crate::{coqtop::xml_protocol::types::{FeedbackContent, ProtocolRichPP}, files::{goal_file, result_file}, logger, state::{CoqState, ErrorState}};

use super::xml_protocol::types::{ProtocolCall, ProtocolResult, ProtocolValue};

/// The name of the process used for IDE interactions with Coq.
pub static COQTOP: &'static str = "coqidetop";

/// The structure encapsulating all communications with the underlying [`COQTOP`] process.
pub struct IdeSlave {
    /// The main channel where [`COQTOP`] sends its responses.
    main_r: TcpStream,
    /// The main channel to send commands (calls, see [`ProcotolCall`]) to [`COQTOP`].
    main_w: TcpStream,
    /// The [`COQTOP`] process itself.
    coqidetop: Child,
    /// The receiving end of a channel used to transmit protocol calls to send to [`COQTOP`].
    call_rx: mpsc::UnboundedReceiver<ProtocolCall>,
    /// The transmitting end of a channel to send commands to Kakoune.
    cmd_tx: mpsc::UnboundedSender<String>,
    /// The file where all results are written.
    result_file: File,
    /// The file where goals are written.
    goal_file: File,
}

impl IdeSlave {
    /// Creates a new [`ideSlave`] by spawning 4 TCP sockets as well as a [`COQTOP`] process.
    pub async fn new(
        call_rx: mpsc::UnboundedReceiver<ProtocolCall>,
        cmd_tx: mpsc::UnboundedSender<String>,
        tmp_dir: &String,
        topfile: String,
    ) -> io::Result<Self> {
        let (main_w_listen, main_w_port) = create_listener().await?;
        let (main_r_listen, main_r_port) = create_listener().await?;

        // NOTE: `async { X.await }` can also be written `X`. However, I find it less clear when types
        // are not inlined in my code (which rust-analyzer is able to do).
        // Please do not refactor this...
        let main_r = async { main_r_listen.accept().await };
        let main_w = async { main_w_listen.accept().await };

        let ports = [main_r_port, main_w_port];
        let flags = ["-async-proofs", "on", "-topfile", &topfile];

        let coqidetop = async { coqidetop(tmp_dir, ports, flags).await };

        let (main_r, main_w, coqidetop) = join!(main_r, main_w, coqidetop);
        // NOTE: because we are using TCP streams, we don't care about the second parameter returned by [`TcpListener::accept`]
        // hence all the `.0`s.
        let (main_r, main_w, coqidetop) = (main_r?.0, main_w?.0, coqidetop?);

        log::info!(
            "{} (process {}) is up and running!",
            COQTOP,
            coqidetop.id().unwrap_or(0)
        );

        let result_file = File::create(result_file(&tmp_dir)).await?;
        let goal_file = File::create(goal_file(&tmp_dir)).await?;

        Ok(Self {
            main_r,
            main_w,
            coqidetop,
            call_rx,
            cmd_tx,
            result_file,
            goal_file,
        })
    }

    /// Send a [`ProtocolCall`] to the underlying [`COQTOP`] process.
    async fn send(&mut self, call: ProtocolCall) -> io::Result<()> {
        let encoded = call.encode();
        log::debug!("Sending encoded command `{}` to {}", encoded, COQTOP);

        self.main_w.write_all(encoded.as_bytes()).await
    }

    async fn recv(&mut self) -> io::Result<ProtocolResult> {
        ProtocolResult::decode_stream(&mut self.main_r).await
    }

    /// Runs a join point which processes anything related to [`COQTOP`]:
    /// - until `stop_rx` receives a value (in which case it ends).
    /// - when a [`ProtocolCall`] is received through the `main_w` channel, it encodes it
    ///   and sends it directly to [`COQTOP`].
    /// - when a [`ProtocolResult`] can be decoded from [`COQTOP`], try to process the response
    ///   according to these rules:
    ///   - if the response is a [`ProtocolResult::Fail`], output the error to the result buffer,
    ///     change the current state to be non-processing and report the error to Kakoune.
    ///   - if the response is a [`ProtocolResult::Good`] and it contains a [`ProtocolValue::StateId`], update
    ///     the current state ID.
    ///   - if the response is a [`ProtocolResult::Feedback`] and its content as a `processed` tag, update
    ///     the processed range in Kakoune.
    ///   - if the response is a [`ProtocolResult::Feedback`] and its content as a `message` tag,
    ///     output the message to the result buffer.
    ///   - else no special treatment is reserved, therefore we can ignore
    pub async fn process(
        &mut self,
        coq_state: Arc<RwLock<CoqState>>,
        mut stop_rx: watch::Receiver<()>,
    ) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                Some(call) = self.call_rx.recv() => {
                    let encoded = call.encode();
                    log::debug!("Sending encoded command `{}` to {}", encoded, COQTOP);

                    self.main_w.write_all(encoded.as_bytes()).await?;
                }
                Ok(resp) = ProtocolResult::decode_stream(&mut self.main_r) => {
                    log::debug!("Received response `{:?}` from {}", resp, COQTOP);

                    let error_state = tokio::task::block_in_place(|| -> io::Result<ErrorState> {
                        let coq_state = coq_state.read()
                            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, format!("{:?}", err)))?;
                        Ok(coq_state.get_error_state())
                    })?;

                    if error_state == ErrorState::Ok {
                        self.process_response(coq_state.clone(), resp).await?;
                    }
                }
            }
        }
    }

    ///
    async fn process_response(
        &mut self,
        coq_state: Arc<RwLock<CoqState>>,
        resp: ProtocolResult,
    ) -> io::Result<()> {
        use FeedbackContent::*;
        use ProtocolValue::*;

        match resp {
            // Result of an Init or Add call
            ProtocolResult::Good(StateId(state_id))
            | ProtocolResult::Good(Pair(box StateId(state_id), box _)) => {
                tokio::task::block_in_place(|| -> io::Result<()> {
                    let mut coq_state = coq_state.write().map_err(|err| {
                        io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err))
                    })?;

                    coq_state.set_current_state_id(state_id);
                    Ok(())
                })?;

                self.refresh_processed(coq_state).await?;

                // TODO: refresh goals
            }
            ProtocolResult::Good(Optional(Some(box Pair(box List(_), box _)))) => {
                log::warn!("Unhandled response {:?}", resp);
                // TODO: output hints in all option/pair/list/pair/string/::text
            }
            // Any other good result only refreshes the processed range
            ProtocolResult::Good(_) => {
                self.refresh_processed(coq_state).await?;
            }
            // On fail, send the fail message to the result buffer
            ProtocolResult::Fail(_, _, ProtocolRichPP::Raw(msg)) => {
                tokio::task::block_in_place(|| -> io::Result<()> {
                    let mut coq_state = coq_state.write().map_err(|err| {
                        io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err))
                    })?;

                    coq_state.backtrack_last_processed();
                    coq_state.error();
                    Ok(())
                })?;

                self.output_to_result(msg).await?;
                self.send_command(String::new()).await?;
            }
            ProtocolResult::Feedback(_, _, _, Message(ProtocolRichPP::Raw(msg))) => {
                self.output_to_result(msg).await?;
                self.send_command(String::new()).await?;
            }
            ProtocolResult::Feedback(_, _, _, Processed) => {
                self.refresh_processed(coq_state).await?;
            }
            ProtocolResult::Feedback(_, _, _, _) => {
                log::warn!("Unhandled response {:?}", resp);
            }
        }

        Ok(())
    }

    ///
    async fn output_to_result(&mut self, msg: String) -> io::Result<()> {
        self.result_file.set_len(0).await?;
        self.result_file.seek(SeekFrom::Start(0)).await?;
        self.result_file.write_all(msg.as_bytes()).await
    }

    ///
    async fn send_command(&self, cmd: String) -> io::Result<()> {
        self.cmd_tx
            .send(cmd)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }

    async fn refresh_processed(&self, coq_state: Arc<RwLock<CoqState>>) -> io::Result<()> {
        let cmd = tokio::task::block_in_place(|| -> io::Result<String> {
            let coq_state = coq_state
                .read()
                .map_err(|err| io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err)))?;

            Ok(format!(
                r#"set-option buffer coqide_processed_range %val{{timestamp}} '{}|coqide_processed'"#,
                coq_state.processed_range()
            ))
        })?;

        self.send_command(cmd).await
    }

    /// Drops the TCP sockets as well as the [`COQTOP`] process.
    pub async fn quit(mut self) -> io::Result<()> {
        log::debug!("Shutting down communication channels");
        self.main_r.shutdown().await?;
        self.main_w.shutdown().await?;

        log::debug!("Stopping {}", COQTOP);
        self.coqidetop.kill().await?;

        Ok(())
    }
}

/// Creates a new [`TcpListener`] listening on `127.0.0.1:0`, and returns both the
/// listener and the port it is listening on.
async fn create_listener() -> io::Result<(TcpListener, u16)> {
    let listen = TcpListener::bind("127.0.0.1:0").await?;
    let port = listen.local_addr()?.port();

    Ok((listen, port))
}

/// Spawns a new [`COQTOP`] process given the 4 ports it should connect to
/// (in order: `[main_r, main_w, control_r, control_w]`) as well as some more flags
/// (e.g. `["-topfile", file]`).
async fn coqidetop<const N: usize>(
    tmp_dir: &String,
    ports: [u16; 2],
    flags: [&str; N],
) -> io::Result<Child> {
    Command::new(COQTOP)
        .arg("-main-channel")
        .arg(format!("127.0.0.1:{}:{}", ports[0], ports[1]))
        .args(flags)
        .stdout(std::fs::File::create(logger::log_file(&tmp_dir))?)
        .kill_on_drop(true)
        .spawn()
}

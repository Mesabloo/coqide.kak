use crate::{
    coqtop,
    xml_protocol::types::{ProtocolCall, ProtocolResult, ProtocolValue},
};
use std::io;
use tokio::{io::AsyncWriteExt, join, process::Child};
use tokio::net::{TcpListener, TcpStream};

/// The state of the IDE slave (`coqidetop`)
pub struct IdeSlave {
    /// The main readable channel, where `coqidetop` sends its responses
    main_r: TcpStream,
    /// The main writable channel, used to send calls to `coqidetop`
    main_w: TcpStream,
    /// Some control channel
    control_r: TcpStream,
    /// Some control channel
    control_w: TcpStream,
    /// The `coqidetop` child process
    proc: Child,
    /// Current state of the connection
    state: SlaveState,
    /// The state ID returned as a response to an `Init` call
    root_id: i64,
    /// The current state ID which must be forwarded to some calls
    current_id: i64,
}

impl IdeSlave {
    pub async fn init(file: String) -> io::Result<Self> {
        let listener1 = TcpListener::bind("127.0.0.1:0").await?;
        let listener2 = TcpListener::bind("127.0.0.1:0").await?;
        let listener3 = TcpListener::bind("127.0.0.1:0").await?;
        let listener4 = TcpListener::bind("127.0.0.1:0").await?;

        let main_r = listener1.accept();
        let main_w = listener2.accept();
        let control_r = listener3.accept();
        let control_w = listener4.accept();

        let additional_flags = [
            "-async-proofs".to_string(),
            "on".to_string(),
            "-topfile".to_string(),
            file,
        ];
        let ports = [
            listener1.local_addr()?.port(),
            listener2.local_addr()?.port(),
            listener3.local_addr()?.port(),
            listener4.local_addr()?.port(),
        ];
        log::info!("Listening on ports {:?}", &ports);

        let proc = coqtop::spawn(ports, &additional_flags);

        let (main_r, main_w, control_r, control_w, proc) =
            join!(main_r, main_w, control_r, control_w, proc);
        let ((main_r, _), (main_w, _), (control_r, _), (control_w, _), proc) =
            (main_r?, main_w?, control_r?, control_w?, proc?);

        log::debug!(
            "`{}` (process {}) is up and running!",
            coqtop::COQTOP,
            proc.id().unwrap_or(0)
        );

        Ok(Self {
            main_r,
            main_w,
            control_r,
            control_w,
            proc,
            state: SlaveState::Connected,
            root_id: -1,
            current_id: -1,
        })
    }

    pub async fn send_call(&mut self, call: ProtocolCall) -> io::Result<()> {
        log::debug!("Sending call '{:?}' to `{}`", call, coqtop::COQTOP);
        self.main_w
            .write_all(call.clone().encode().as_bytes())
            .await?;

        let response = ProtocolResult::decode_stream(&mut self.main_r).await?;
        log::debug!("`{}` responsed with `{:?}`", coqtop::COQTOP, response);

        match response {
            ProtocolResult::Fail(line, col, v) => {
                self.state = SlaveState::Error;
            }
            ProtocolResult::Good(ProtocolValue::StateId(state_id)) => {
                if let ProtocolCall::Init(_) = call {
                    // set the root state
                    self.root_id = state_id;
                }
            }
            ProtocolResult::Good(_) => unreachable!(),
        }

        Ok(())
    }

    pub fn current_id(&self) -> i64 {
        if self.current_id != -1 {
            self.current_id
        } else {
            self.root_id
        }
    }

    pub async fn quit(mut self) -> io::Result<()> {
        log::info!("Closing communication channels");

        self.main_w
            .write_all(ProtocolCall::Quit.encode().as_bytes())
            .await?;

        self.main_r.shutdown().await?;
        self.main_w.shutdown().await?;
        self.control_r.shutdown().await?;
        self.control_w.shutdown().await?;

        log::info!("Stopping `{}` process", crate::coqtop::COQTOP);

        self.proc.kill().await?;
        self.state = SlaveState::Disconnected;

        Ok(())
    }
}

pub enum SlaveState {
    Connected,
    Disconnected,
    Error,
}

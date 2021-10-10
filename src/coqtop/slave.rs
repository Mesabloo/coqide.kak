use crate::{
    coqtop,
    xml_protocol::types::{ProtocolCall, ProtocolResult, ProtocolValue},
};
use bimap::BiMap;
use std::io;
use tokio::{
    fs::File,
    net::{TcpListener, TcpStream},
};
use tokio::{io::AsyncWriteExt, join, process::Child};

///
pub enum ConnectionState {
    Connected,
    Disconnected,
    Error,
}

impl Default for ConnectionState {
    fn default() -> Self {
        ConnectionState::Connected
    }
}

#[derive(Eq, PartialEq, PartialOrd, Ord, Hash)]
///
pub struct Range {
    /// The beginning of the range, encoded as `(line, column)`
    begin: (u64, u64),
    /// The end of the range, encoded as `(line, column)`
    end: (u64, u64),
}

/// The current state of the slave
pub struct SlaveState {
    /// Connection state to the slave
    connection: ConnectionState,
    /// The state ID returned as a response to an `Init` call
    root_id: i64,
    /// The current state ID forwarded to some calls
    current_id: i64,
    ///
    states: BiMap<Range, i64>,
}

impl Default for SlaveState {
    fn default() -> Self {
        SlaveState {
            connection: ConnectionState::Connected,
            root_id: -1,
            current_id: -1,
            states: BiMap::new(),
        }
    }
}

///
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

    ///
    goal: File,
    ///
    result: File,

    state: SlaveState,
}

impl IdeSlave {
    pub async fn init(
        file: String,
        goal_buffer: &String,
        result_buffer: &String,
    ) -> io::Result<Self> {
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
            goal: File::open(goal_buffer).await?,
            result: File::open(result_buffer).await?,
            state: Default::default(),
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
            ProtocolResult::Fail(_line, _col, _v) => {
                self.state.connection = ConnectionState::Error;


            }
            ProtocolResult::Good(ProtocolValue::StateId(state_id)) => {
                match call {
                    ProtocolCall::Init(_) => {
                        // set the root state
                        self.state.root_id = state_id;
                        self.state.current_id = state_id;
                    }
                    _ => {}
                }
            }
            ProtocolResult::Good(_) => unreachable!(),
        }

        Ok(())
    }

    pub fn current_state(&self) -> i64 {
        if self.state.states.len() == 0 {
            self.state.root_id
        } else {
            self.state.current_id
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

        log::debug!("Closing buffers");
        self.goal.shutdown().await?;
        self.result.shutdown().await?;

        self.state = Default::default();

        Ok(())
    }
}

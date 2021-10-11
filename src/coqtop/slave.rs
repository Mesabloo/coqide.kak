use crate::{
    coqtop,
    kak_protocol::kakoune::kakoune,
    xml_protocol::types::{ProtocolCall, ProtocolResult, ProtocolRichPP},
};
use bimap::BiMap;
use std::{fmt::Display, io};
use tokio::{
    fs::File,
    net::{TcpListener, TcpStream},
};
use tokio::{io::AsyncWriteExt, join, process::Child};

pub struct ExtState {
    pub session: String,
    pub goal_buffer: String,
    pub result_buffer: String,
    pub buffer: String,
}

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

#[derive(Eq, PartialEq, PartialOrd, Ord, Hash, Clone)]
///
pub struct Range {
    /// The beginning of the range, encoded as `(line, column)`
    pub begin: (u64, u64),
    /// The end of the range, encoded as `(line, column)`
    pub end: (u64, u64),
}

impl Default for Range {
    fn default() -> Self {
        Range {
            begin: (1, 1),
            end: (1, 1),
        }
    }
}

impl Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{},{}.{}",
            self.begin.0, self.begin.1, self.end.0, self.end.1
        )
    }
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

    pub ext_state: ExtState,
}

impl IdeSlave {
    pub async fn init(
        file: String,
        session: &String,
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
            file.clone(),
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
            ext_state: ExtState {
                session: session.clone(),
                goal_buffer: goal_buffer.clone(),
                result_buffer: result_buffer.clone(),
                buffer: file,
            },
        })
    }

    pub async fn send_call(&mut self, call: ProtocolCall) -> io::Result<ProtocolResult> {
        log::debug!("Sending call '{:?}' to `{}`", call, coqtop::COQTOP);
        self.main_w
            .write_all(call.clone().encode().as_bytes())
            .await?;

        let response = ProtocolResult::decode_stream(&mut self.main_r).await?;
        log::debug!("`{}` responsed with `{:?}`", coqtop::COQTOP, response);

        Ok(response)
    }

    pub fn set_root_id(&mut self, id: i64) {
        self.state.root_id = id;
    }

    pub fn set_current_id(&mut self, id: i64) {
        self.state.current_id = id;
    }

    pub fn get_root_id(&self) -> i64 {
        self.state.root_id
    }

    pub fn get_range(&self, id: i64) -> Range {
        self.state
            .states
            .get_by_right(&id)
            .cloned()
            .unwrap_or_else(|| Default::default())
    }

    pub async fn error(
        &mut self,
        line: Option<i64>,
        col: Option<i64>,
        richpp: ProtocolRichPP,
    ) -> io::Result<()> {
        self.state.connection = ConnectionState::Error;

        match richpp {
            ProtocolRichPP::Raw(str) => {
                self.result.write_all(str.as_bytes()).await?;
                kakoune(
                    self.ext_state.session.clone(),
                    format!(
                        r#"evaluate-commands -buffer "%opt{{coqide_result_buffer}}" %{{
                          execute-keys "%|cat {}<ret>"
                        }}"#,
                        self.ext_state.result_buffer
                    ),
                )
                .await
            }
        }
    }

    pub async fn back(&mut self, nb_steps: i64) -> io::Result<ProtocolResult> {
        let new_state_id = (self.state.states.len() as i64 - nb_steps).max(self.state.root_id);
        self.state
            .states
            .retain(|_, state_id| state_id < &new_state_id);

        self.send_call(ProtocolCall::EditAt(new_state_id)).await
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
        self.state.connection = ConnectionState::Disconnected;

        Ok(())
    }
}

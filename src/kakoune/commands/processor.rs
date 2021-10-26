use std::{
    process,
    sync::{Arc, RwLock},
};

use tokio::{
    fs::File,
    io,
    sync::{mpsc, watch},
};

use crate::{
    coqtop::xml_protocol::types::{ProtocolCall, ProtocolValue},
    state::{CodeSpan, CoqState},
};

use super::types::Command;

/// The command processor receives commands, processes them and modifies the current daemon state
/// and finally sends calls to the [`COQTOP`] process.
///
/// [`COQTOP`]: crate::coqtop::slave::COQTOP
pub struct CommandProcessor {
    /// The receiving end of the command pipe, where user commands flow towards.
    pipe_rx: mpsc::UnboundedReceiver<Command>,
    /// The transmitting end of the call channel, where [`ProtocolCall`]s are sent to the [`COQTOP`] process.
    ///
    /// [`COQTOP`]: crate::coqtop::slave::COQTOP
    call_tx: mpsc::UnboundedSender<ProtocolCall>,
    /// The current daemon state.
    coq_state: Arc<RwLock<CoqState>>,
    /// The file holding the current goals.
    goal_file: File,
    /// The file holding any feedback to the user.
    result_file: File,
}

impl CommandProcessor {
    /// Creates a new command processor.
    pub async fn new(
        pipe_rx: mpsc::UnboundedReceiver<Command>,
        call_tx: mpsc::UnboundedSender<ProtocolCall>,
        coq_state: Arc<RwLock<CoqState>>,
        goal_file: String,
        result_file: String,
    ) -> io::Result<Self> {
        let goal_file = File::create(goal_file).await?;
        let result_file = File::create(result_file).await?;

        Ok(Self {
            pipe_rx,
            call_tx,
            coq_state,
            goal_file,
            result_file,
        })
    }

    /// Runs the command processor continuously until a message is received through
    /// the `stop_rx` parameter.
    pub async fn process(&mut self, mut stop_rx: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                Some(cmd) = self.pipe_rx.recv() => self.process_command(cmd).await?,
            }
        }
    }

    /// Sends a [`ProtocolCall`] through the call channel.
    #[inline]
    async fn send(&mut self, call: ProtocolCall) -> io::Result<()> {
        self.call_tx
            .send(call)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }

    /// Tries to process a receive user command.
    ///
    /// The corresponding `self.process_XXX` (where `XXX` is the command) are called.
    async fn process_command(&mut self, cmd: Command) -> io::Result<()> {
        tokio::task::block_in_place(|| -> io::Result<()> {
            let mut coq_state = self
                .coq_state
                .write()
                .map_err(|err| io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err)))?;
            coq_state.reset_last_processed();
            coq_state.ok();

            Ok(())
        })?;

        match cmd {
            Command::Init => self.process_init().await?,
            Command::Quit => self.process_quit().await?,
            Command::Previous => self.process_previous().await?,
            Command::RewindTo(line, col) => self.process_rewind_to(line, col).await?,
            Command::Query(str) => self.process_query(str).await?,
            Command::MoveTo(_) => {
                log::error!("Unhandled command `{:?}`", cmd);
                todo!()
            }
            Command::Next(range, code) => self.process_next(range, code).await?,
        }

        Ok(())
    }

    /////////////////////////////////////////////////////////:

    /// Process a [`Command::Init`] call by simply sending an empty [`ProtocolCall::Init`].
    async fn process_init(&mut self) -> io::Result<()> {
        self.send(ProtocolCall::Init(ProtocolValue::Optional(None)))
            .await
    }

    /// Process a [`Command::Quit`] by killing ourselves with a [`libc::SIGINT`] signal.
    async fn process_quit(&mut self) -> io::Result<()> {
        tokio::task::spawn_blocking(|| {
            unsafe {
                libc::kill(process::id() as i32, libc::SIGINT);
            }
            Ok(())
        })
        .await?
    }

    /// Process a [`Command::RewindTo`] by backtracking to the correct state and sending a [`ProtocolCall::EditAt`] wrapping
    /// the new current state ID.
    async fn process_rewind_to(&mut self, line: u64, col: u64) -> io::Result<()> {
        let state_id = tokio::task::block_in_place(|| -> io::Result<i64> {
            let mut coq_state = self
                .coq_state
                .write()
                .map_err(|err| io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err)))?;

            coq_state.backtrack_before_position(line, col);
            Ok(coq_state.get_current_state_id())
        })?;

        self.send(ProtocolCall::EditAt(state_id)).await
    }

    /// Process a [`Command::Next`] by pushing the new code range to the daemon state, and sending a [`ProtocolCall::Add`].
    async fn process_next(&mut self, range: CodeSpan, code: String) -> io::Result<()> {
        let state_id = tokio::task::block_in_place(|| -> io::Result<i64> {
            let mut coq_state = self
                .coq_state
                .write()
                .map_err(|err| io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err)))?;

            let sid = coq_state.get_current_state_id();
            coq_state.push_range(sid, range);
            Ok(sid)
        })?;

        self.send(ProtocolCall::Add(code, state_id)).await
    }

    /// Process a [`Command::Query`] by sending the query string along with the current state ID inside a [`ProtocolCall::Query`].
    async fn process_query(&mut self, query: String) -> io::Result<()> {
        use ProtocolValue::*;

        let state_id = tokio::task::block_in_place(|| -> io::Result<i64> {
            let coq_state = self
                .coq_state
                .read()
                .map_err(|err| io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err)))?;

            Ok(coq_state.get_current_state_id())
        })?;

        self.send(ProtocolCall::Query(Pair(
            Box::new(RouteId(0)),
            Box::new(Pair(Box::new(Str(query)), Box::new(StateId(state_id)))),
        )))
        .await
    }

    /// Process a [`Command::Previous`] by backtracking the current state one state earlier.
    async fn process_previous(&mut self) -> io::Result<()> {
        let state_id = tokio::task::block_in_place(|| -> io::Result<i64> {
            let mut coq_state = self
                .coq_state
                .write()
                .map_err(|err| io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err)))?;

            coq_state.backtrack_one_state();
            Ok(coq_state.get_current_state_id())
        })?;

        self.send(ProtocolCall::EditAt(state_id)).await
    }
}

use std::{process, sync::{Arc, RwLock}};

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

pub struct CommandProcessor {
    pipe_rx: mpsc::UnboundedReceiver<Command>,
    call_tx: mpsc::UnboundedSender<ProtocolCall>,
    coq_state: Arc<RwLock<CoqState>>,
    goal_file: File,
    result_file: File,
}

impl CommandProcessor {
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

    pub async fn process(&mut self, mut stop_rx: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                Some(cmd) = self.pipe_rx.recv() => {
                    // TODO: process commands with the current state

                    // Reset the error state when receiving a new command
                    //tokio::task::block_in_place(|| self.ok())?;

                    self.process_command(cmd).await?;
                }
            }
        }
    }

    #[inline]
    async fn send(&mut self, call: ProtocolCall) -> io::Result<()> {
        self.call_tx
            .send(call)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }

    async fn process_command(&mut self, cmd: Command) -> io::Result<()> {
        tokio::task::block_in_place(|| -> io::Result<()> {
            let mut coq_state = self.coq_state
                .write()
                .map_err(|err| io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err)))?;
            coq_state.reset_last_processed();
            coq_state.ok();

            Ok(())
        })?;

        match cmd {
            Command::Init => self.process_init().await?,
            Command::Quit => self.process_quit().await?,
            Command::Previous => {
                log::error!("Unhandled command `{:?}`", cmd);
                todo!()
            }
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

    async fn process_init(&mut self) -> io::Result<()> {
        self.send(ProtocolCall::Init(ProtocolValue::Optional(None)))
            .await?;

        Ok(())
    }

    async fn process_quit(&mut self) -> io::Result<()> {
        tokio::task::spawn_blocking(|| {
            unsafe {
                libc::kill(process::id() as i32, libc::SIGINT);
            }
            Ok(())
        })
        .await?
    }

    async fn process_rewind_to(&mut self, line: u64, col: u64) -> io::Result<()> {
        let state_id = tokio::task::block_in_place(|| -> io::Result<i64> {
            let mut coq_state = self
                .coq_state
                .write()
                .map_err(|err| io::Error::new(io::ErrorKind::Deadlock, format!("{:?}", err)))?;

            coq_state.backtrack_to_position(line, col);
            Ok(coq_state.get_current_state_id())
        })?;

        self.send(ProtocolCall::EditAt(state_id)).await
    }

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
}

use std::{
    future::Future,
    process,
    sync::{Arc, RwLock},
};

use tokio::{
    io,
    sync::{mpsc, watch},
};

use crate::{
    coqtop::{
        slave::{IdeSlave, COQTOP},
        xml_protocol::types::{ProtocolCall, ProtocolResult, ProtocolRichPP, ProtocolValue},
    },
    state::{CoqState, ErrorState},
};

use super::types::Command;

pub struct CommandProcessor {
    pipe_rx: mpsc::UnboundedReceiver<Command>,
    cmd_tx: mpsc::UnboundedSender<String>,
    ideslave: IdeSlave,
    coq_state: Arc<RwLock<CoqState>>,
}

impl CommandProcessor {
    pub fn new(
        pipe_rx: mpsc::UnboundedReceiver<Command>,
        cmd_tx: mpsc::UnboundedSender<String>,
        ideslave: IdeSlave,
        coq_state: Arc<RwLock<CoqState>>,
    ) -> Self {
        Self {
            pipe_rx,
            cmd_tx,
            ideslave,
            coq_state,
        }
    }

    pub async fn process(&mut self, mut stop_rx: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                Some(cmd) = self.pipe_rx.recv() => {
                    // Reset the error state when receiving a new command
                    tokio::task::block_in_place(|| self.ok())?;

                    self.process_command(cmd).await?;
                }
            }
        }
    }

    pub async fn shutdown(self) -> io::Result<()> {
        self.ideslave.quit().await?;

        Ok(())
    }

    /////////////

    #[inline]
    async fn recv_1(&mut self) -> io::Result<ProtocolResult> {
        self.ideslave.recv().await
    }

    #[inline]
    async fn recv_2(&mut self) -> io::Result<(ProtocolResult, ProtocolResult)> {
        Ok((self.ideslave.recv().await?, self.ideslave.recv().await?))
    }

    fn error(
        &mut self,
        line: Option<i64>,
        col: Option<i64>,
        msg: ProtocolRichPP,
    ) -> io::Result<()> {
        {
            let mut coq_state = self
                .coq_state
                .write()
                .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{}", err)))?;
            coq_state.error();
        }

        Ok(())
    }

    fn ok(&mut self) -> io::Result<()> {
        {
            let mut coq_state = self
                .coq_state
                .write()
                .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{}", err)))?;
            coq_state.ok();
        }

        Ok(())
    }

    fn error_state(&self) -> io::Result<ErrorState> {
        {
            let mut coq_state = self
                .coq_state
                .read()
                .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{}", err)))?;
            Ok(coq_state.get_error_state())
        }
    }

    fn unexpected_response(&mut self, resp: ProtocolResult) {
        log::error!("Unexpected response '{:?}' from {}", resp, COQTOP);
    }

    #[inline]
    async fn when_good<'a, 'b: 'a, F, G>(&'b mut self, g: G) -> F::Output
    where
        F: Future<Output = io::Result<()>>,
        G: Fn(&'a mut Self, ProtocolValue) -> F,
    {
        loop {
            let resp = self.recv_1().await?;
            log::debug!("Received response '{:?}' from {}", resp, COQTOP);

            match resp {
                ProtocolResult::Fail(line, col, msg) => {
                    break tokio::task::block_in_place(|| self.error(line, col, msg));
                }
                ProtocolResult::Good(val) => break g(self, val).await,
                ProtocolResult::Feedback(_, _, _, _) => self.unexpected_response(resp),
            }
        }
    }

    ///////////////////////////////////////

    async fn process_command(&mut self, cmd: Command) -> io::Result<()> {
        // If we already errored out (which should not happen), do not process this command
        let error_state = tokio::task::block_in_place(|| self.error_state())?;
        if let ErrorState::Error = error_state {
            return Ok(());
        }

        match cmd {
            Command::Init => self.process_init().await,
            Command::Quit => self.process_quit().await,
            Command::Query(str) => self.process_query(str).await,
            Command::Previous => todo!(),
            Command::RewindTo(_, _) => todo!(),
            Command::MoveTo(_) => todo!(),
            Command::Next(_, _) => todo!(),
        }
    }

    async fn process_init(&mut self) -> io::Result<()> {
        self.ideslave
            .send(ProtocolCall::Init(ProtocolValue::Optional(None)))
            .await?;

        self.when_good(Self::handle_init).await
    }

    async fn process_quit(&mut self) -> io::Result<()> {
        self.ideslave.send(ProtocolCall::Quit).await?;

        self.handle_quit().await
    }

    async fn process_query(&mut self, query: String) -> io::Result<()> {
        Ok(())
    }

    /////////////////////////////////////////////

    async fn handle_init(&mut self, val: ProtocolValue) -> io::Result<()> {
        Ok(())
    }

    async fn handle_quit(&mut self) -> io::Result<()> {
        // When we receive `quit`, send a SIGINT to ourselves to gracefully exit
        unsafe {
            libc::kill(process::id() as i32, libc::SIGINT);
        }
        Ok(())
    }
}

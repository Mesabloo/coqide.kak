use std::{
    io, process,
    sync::atomic::{
        AtomicBool,
        Ordering::{self, *},
    },
};

use bimap::BiMap;
use tokio::sync::{mpsc, watch};

use crate::{
    codespan::CodeSpan,
    kakoune::{
        command_line::kak,
        commands::types::{DisplayCommand, KakouneCommand},
    },
};

use super::{
    feedback_queue::Feedback,
    xml_protocol::types::{ProtocolCall, ProtocolResult, ProtocolRichPP, ProtocolValue},
};

pub struct SynchronizedState {
    pub current_state_id: i64,
    pub route_id: i64,
    // all ranges
    error: Option<CodeSpan>,
    state_id_to_range: BiMap<i64, CodeSpan>,
    // ---------
    call_tx: mpsc::UnboundedSender<ProtocolCall>,
    response_rx: mpsc::UnboundedReceiver<ProtocolResult>,
    cmd_rx: mpsc::UnboundedReceiver<KakouneCommand>,
    feedback_tx: mpsc::UnboundedSender<Feedback>,
    disp_cmd_tx: mpsc::UnboundedSender<DisplayCommand>,
    fetch_next_command: AtomicBool,
    kak_session: String,
    coq_file: String,
}

impl SynchronizedState {
    pub fn new(
        call_tx: mpsc::UnboundedSender<ProtocolCall>,
        response_rx: mpsc::UnboundedReceiver<ProtocolResult>,
        feedback_tx: mpsc::UnboundedSender<Feedback>,
        cmd_rx: mpsc::UnboundedReceiver<KakouneCommand>,
        disp_cmd_tx: mpsc::UnboundedSender<DisplayCommand>,
        kak_session: String,
        coq_file: String,
    ) -> Self {
        Self {
            current_state_id: 1,
            route_id: 1,
            call_tx,
            response_rx,
            cmd_rx,
            feedback_tx,
            disp_cmd_tx,
            fetch_next_command: AtomicBool::new(true),
            error: None,
            state_id_to_range: BiMap::new(),
            kak_session,
            coq_file,
        }
    }

    pub async fn process(&mut self, mut stop_rx: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                Some(cmd) = self.cmd_rx.recv(), if self.fetch_next_command.load(Ordering::Relaxed) => {
                    self.process_command(cmd).await?;
                },
                Some(resp) = self.response_rx.recv() => {
                    self.process_response(resp).await?;
                },
            }
        }
    }

    // ----------------------------------------------

    async fn process_command(&mut self, cmd: KakouneCommand) -> io::Result<()> {
        use KakouneCommand::*;

        self.fetch_next_command.store(false, Ordering::Relaxed);

        match cmd {
            Init => self.process_init().await?,
            Quit => self.process_quit().await?,
            Previous => self.process_previous().await?,
            RewindTo(_, _) => todo!(),
            Query(_) => todo!(),
            MoveTo(_) => todo!(),
            Next(range, code) if self.error.is_none() => self.process_next(range, code).await?,
            IgnoreError => todo!(),
            Hints => todo!(),
            c => {
                self.allow_processing_commands().await?;
                log::info!(
                    "Ignoring command {:?} (maybe it is not time to process it?)",
                    c
                );
            }
        }

        Ok(())
    }

    async fn process_init(&mut self) -> io::Result<()> {
        self.send_call(ProtocolCall::Init(ProtocolValue::Optional(None)))
            .await
    }

    async fn process_quit(&mut self) -> io::Result<()> {
        tokio::task::spawn_blocking(|| unsafe {
            libc::kill(process::id() as i32, libc::SIGINT);
            Ok(())
        })
        .await
        .unwrap()
    }

    async fn process_next(&mut self, range: CodeSpan, code: String) -> io::Result<()> {
        if code.is_empty() {
            // we reached the end of the buffer
            //
            // coqidetop does not seem to like adding empty ranges, so we don't.
            return Ok(());
        }

        self.state_id_to_range.insert(self.current_state_id, range);
        self.send_call(ProtocolCall::Add(code, self.current_state_id))
            .await
    }

    async fn process_previous(&mut self) -> io::Result<()> {
        let old_state_id = self.current_state_id - 1;
        self.state_id_to_range.retain(|id, _| id < &old_state_id);
        self.current_state_id = std::cmp::max(old_state_id, 1);
        self.send_call(ProtocolCall::EditAt(self.current_state_id))
            .await
    }

    // ------------------------------------------------

    //https://coq.github.io/doc/V8.13.2/api/coqide-server/Interface/index.html

    async fn process_response(&mut self, resp: ProtocolResult) -> io::Result<()> {
        use ProtocolResult::*;

        match resp {
            Good(value) => self.process_response_good(value).await?,
            Fail(_, _, ProtocolValue::StateId(state_id), message) => {
                self.process_response_fail(state_id, message).await?
            }
            Feedback(_, _, state_id, content) => {
                self.feedback_tx
                    .send((state_id, content))
                    .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
            }
            r => log::warn!("Unknown response {:?}", r),
        }

        Ok(())
    }

    async fn process_response_good(&mut self, value: ProtocolValue) -> io::Result<()> {
        use ProtocolValue::*;

        match value {
            StateId(state_id) => {
                // response from an Init call
                self.current_state_id = state_id;
                self.allow_processing_commands().await?;
            }
            Pair(box StateId(state_id), box _) => {
                // response from an Add call
                self.send_disp(DisplayCommand::RefreshProcessedRange(self.current_range()))
                    .await?;

                self.current_state_id = state_id;
                self.send_call(ProtocolCall::Goal).await?;
            }
            Optional(None) => {
                // no goals found
                self.send_disp(DisplayCommand::OutputGoals(vec![], vec![], vec![]))
                    .await?;
                self.allow_processing_commands().await?;
            }
            Optional(Some(box Goals(fg, bg, _, gg))) => {
                // response from a Goals call
                self.send_disp(DisplayCommand::OutputGoals(fg, bg, gg))
                    .await?;
                self.allow_processing_commands().await?;
            }
            _ => {
                self.send_disp(DisplayCommand::RefreshProcessedRange(self.current_range()))
                    .await?;
                self.error = None;
                self.send_disp(DisplayCommand::RefreshErrorRange(None))
                    .await?;
                self.allow_processing_commands().await?;
            }
        }

        Ok(())
    }

    async fn process_response_fail(
        &mut self,
        state_id: i64,
        message: ProtocolRichPP,
    ) -> io::Result<()> {
        match self.state_id_to_range.remove_by_left(&state_id) {
            None => {
                log::warn!("Inconsistent state: missing state ID {} in map", state_id);
                self.send_disp(DisplayCommand::RefreshErrorRange(None))
                    .await?;
                self.error = Some(CodeSpan::default());
            }
            Some((_, range)) => {
                self.send_disp(DisplayCommand::RefreshErrorRange(Some(range)))
                    .await?;
                self.error = Some(range);
            }
        }

        if state_id > 0 {
            self.current_state_id = state_id;
        }
        self.allow_processing_commands().await?;
        self.send_disp(DisplayCommand::RefreshProcessedRange(self.current_range()))
            .await?;
        self.send_disp(DisplayCommand::ColorResult(message)).await
    }

    // -----------------------------------------------

    async fn send_call(&self, call: ProtocolCall) -> io::Result<()> {
        self.call_tx
            .send(call)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }

    async fn send_disp(&self, cmd: DisplayCommand) -> io::Result<()> {
        self.disp_cmd_tx
            .send(cmd)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))
    }

    fn current_range(&self) -> CodeSpan {
        if self.state_id_to_range.is_empty() {
            CodeSpan::default()
        } else {
            let state_id = self.current_state_id;
            self.state_id_to_range
                .iter()
                .filter(|(id, _)| *id <= &state_id)
                .map(|(_, range)| range)
                .fold(CodeSpan::default(), CodeSpan::extend)
        }
    }

    async fn allow_processing_commands(&mut self) -> io::Result<()> {
        self.fetch_next_command.store(true, Ordering::Relaxed);
        kak(
            &self.kak_session,
            format!(
                r#"evaluate-commands -buffer '{}' %{{
                    set-option buffer coqide_can_go_further {}
                }}"#,
                self.coq_file, true
            ),
        )
        .await
    }
}

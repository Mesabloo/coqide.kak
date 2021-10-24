use tokio::{
    io,
    net::UnixListener,
    sync::{mpsc, watch},
};

use crate::{files::command_file, kakoune::command_line::kak};

use super::types::Command;

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

        // Populate file descriptor 4 with connection to unix socket
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

        // Initialize the internal process
        self.pipe_tx
            .send(Command::Init)
            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;

        let res = loop {
            tokio::select! {
                Ok(_) = self.stop_rx.changed() => break Ok::<_, io::Error>(()),
                Ok(cmd) = Command::parse_from(&mut pipe) => {
                    match cmd {
                        None => break Ok::<_, io::Error>(()),
                        Some(None) => {
                            log::warn!("Junk byte ignored from stream");
                        }
                        Some(Some(cmd)) => {
                            log::debug!("Received kakoune command '{:?}'", cmd);

                            self.pipe_tx.send(cmd)
                                .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                        }
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

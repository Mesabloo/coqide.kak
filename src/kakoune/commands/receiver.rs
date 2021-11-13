use tokio::{
    io,
    net::UnixListener,
    sync::{mpsc, watch},
};

use crate::{files::command_file, kakoune::command_line::kak};

use super::types::Command;

/// A command receiver, streaming a Unix socket to an unbounded channel.
pub struct CommandReceiver {
    /// The transmitting end of the unbounded channel, used to send user commands to
    /// the command processor.
    pipe_tx: mpsc::UnboundedSender<Command>,
}

impl CommandReceiver {
    /// Constructs a new command receiver.
    pub fn new(pipe_tx: mpsc::UnboundedSender<Command>) -> Self {
        Self { pipe_tx }
    }

    /// Tries to receive user commands from a Unix socket.
    ///
    /// The Unix socket is initialized internally by using [`command_file`] to retrieve the path, and
    /// dropped at the end of this function.
    ///
    /// Additional calls are performed to correctly initialize Kakoune and connect it to the socket.
    pub async fn process(
        &mut self,
        kak_session: String,
        tmp_dir: String,
        coq_file: String,
        mut stop_rx: watch::Receiver<()>,
    ) -> io::Result<()> {
        let pipe_listener = UnixListener::bind(command_file(&tmp_dir))?;

        log::debug!("Binding unix socket in /tmp directory");

        // Populate file descriptor 4 with connection to unix socket
        let populate_fd = kak(
            &kak_session,
            format!(
                r#"evaluate-commands -buffer '{0}' %{{ coqide-populate-fd4 }}
                evaluate-commands -buffer '{0}' %{{
                  edit! -scratch "%opt{{coqide_result_buffer}}"
                  add-highlighter buffer/coqide_result ranges coqide_result_highlight 
                }}
                evaluate-commands -buffer '{0}' %{{
                  edit! -scratch "%opt{{coqide_goal_buffer}}"
                  add-highlighter buffer/coqide_goal ranges coqide_goal_highlight
                }}"#,
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
                Ok(_) = stop_rx.changed() => break Ok::<_, io::Error>(()),
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

    /// Stops the command receiver (currently does nothing).
    pub async fn stop(&mut self) -> io::Result<()> {
        Ok(())
    }
}

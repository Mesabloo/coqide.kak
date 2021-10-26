use tokio::{
    io,
    sync::{mpsc, watch},
};

use crate::{
    files::{goal_file, result_file},
    kakoune::command_line::kak,
};

/// A simple abstraction of Kakoune used to send commands to it.
pub struct KakSlave {
    /// The receiving end of the channel used to send commands to be sent to Kakoune.
    cmd_rx: mpsc::UnboundedReceiver<String>,
    /// The session identifier of the Kakoune session to connect to.
    kak_session: String,
    /// The path to the goal file output in the goal buffer.
    kak_goal: String,
    /// The path to the result file output in the result buffer.
    kak_result: String,
    /// The file currently being edited.
    coq_file: String,
}

impl KakSlave {
    /// Initialises a new Kakoune slave.
    ///
    /// The 4th argument is used to automatically deduce both goal and results files
    /// using [`goal_file`] and [`result_file`].
    pub fn new(
        cmd_rx: mpsc::UnboundedReceiver<String>,
        kak_session: String,
        coq_file: String,
        tmp_dir: &String,
    ) -> Self {
        let kak_goal = goal_file(&tmp_dir);
        let kak_result = result_file(&tmp_dir);

        Self {
            cmd_rx,
            kak_session,
            kak_goal,
            kak_result,
            coq_file,
        }
    }

    /// Runs the processing loop of the kakoune slave until a message is received
    /// on its second parameter.
    ///
    /// Commands to be sent are received asynchronously, and direcctly dispatched (with minor formatting
    /// to execute commands in the correct buffer).
    pub async fn process(&mut self, mut stop_rx: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                Some(cmd) = self.cmd_rx.recv() => {
                    log::debug!("Sending command `{}` to Kakoune", cmd);

                    kak(&self.kak_session, format!(r#"evaluate-commands -buffer '{}' %{{ {} }}"#, self.coq_file, cmd)).await?;

                    self.update_buffers().await?;
                }
            }
        }
    }

    /// Updates both Kakoune buffers to reflect any changes of the current state.
    async fn update_buffers(&self) -> io::Result<()> {
        self.update_goal_buffer().await?;
        self.update_result_buffer().await?;
        Ok(())
    }

    /// Updates the goal buffer by simply `cat`-ing the file to the buffer itself.
    async fn update_goal_buffer(&self) -> io::Result<()> {
        kak(
            &self.kak_session,
            format!(
                r#"evaluate-commands -buffer '{}' %{{
                  execute-keys -buffer "%opt{{coqide_goal_buffer}}" %{{
                    %|cat<space>{}<ret>
                  }}
                }}"#,
                self.coq_file, self.kak_goal,
            ),
        )
        .await
    }

    /// Updates the result buffer by simply `cat`-ing the file to the buffer.
    async fn update_result_buffer(&self) -> io::Result<()> {
        kak(
            &self.kak_session,
            format!(
                r#"evaluate-commands -buffer '{}' %{{
                  execute-keys -buffer "%opt{{coqide_result_buffer}}" %{{
                    %|cat<space>{}<ret>
                  }}
                }}"#,
                self.coq_file, self.kak_result,
            ),
        )
        .await
    }
}

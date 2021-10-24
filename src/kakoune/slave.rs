use tokio::{
    fs::File,
    io,
    sync::{mpsc, watch},
};

use crate::{
    files::{goal_file, result_file},
    kakoune::command_line::kak,
};

pub struct KakSlave {
    cmd_rx: mpsc::UnboundedReceiver<String>,
    kak_session: String,
    kak_goal: String,
    kak_result: String,
    goal_file: File,
    result_file: File,
    coq_file: String,
}

impl KakSlave {
    pub async fn new(
        cmd_rx: mpsc::UnboundedReceiver<String>,
        kak_session: String,
        coq_file: String,
        tmp_dir: &String,
    ) -> io::Result<Self> {
        let kak_goal = goal_file(&tmp_dir);
        let kak_result = result_file(&tmp_dir);

        let goal_file = File::create(&kak_goal).await?;
        let result_file = File::create(&kak_result).await?;

        Ok(Self {
            cmd_rx,
            kak_session,
            kak_goal,
            kak_result,
            goal_file,
            result_file,
            coq_file,
        })
    }

    pub async fn process(&mut self, mut stop_rx: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                Some(cmd) = self.cmd_rx.recv() => {
                    kak(&self.kak_session, cmd).await?;

                    self.update_buffers().await?;
                }
            }
        }
    }

    async fn update_buffers(&self) -> io::Result<()> {
        self.update_goal_buffer().await?;
        self.update_result_buffer().await?;
        Ok(())
    }

    async fn update_goal_buffer(&self) -> io::Result<()> {
        kak(
            &self.kak_session,
            format!(
                r#"evaluate-commands -buffer '{}' %{{
                  execute-keys -buffer "%opt{{coqide_goal_buffer}}" %{{
                    %|cat<space>{}<ret>
                  }}
                }}"#,
                self.coq_file,
                self.kak_goal,
            ),
        )
        .await?;

        Ok(())
    }

    async fn update_result_buffer(&self) -> io::Result<()> {
        kak(
            &self.kak_session,
            format!(
                r#"evaluate-commands -buffer '{}' %{{
                  execute-keys -buffer "%opt{{coqide_result_buffer}}" %{{
                    %|cat<space>{}<ret>
                  }}
                }}"#,
                self.coq_file,
                self.kak_result,
            ),
        )
        .await?;

      
        Ok(())
    }
}

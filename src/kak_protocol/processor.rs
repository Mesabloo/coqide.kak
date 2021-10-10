use super::command::{Command, CommandKind};
use crate::coqtop::slave::IdeSlave;
use std::io;
use tokio::{fs::File, io::AsyncWriteExt};
use unix_named_pipe as fifos;

pub struct CommandProcessor {
    pipe: File,
    session: String,
    ide_slave: IdeSlave,
}

impl CommandProcessor {
    pub async fn init(pipes_path: String, session: String, slave: IdeSlave) -> io::Result<Self> {
        log::debug!("Opening command pipe '{}'", pipes_path);
        let pipe = File::from(fifos::open_read(pipes_path)?);
        log::debug!("Pipe opened!");

        Ok(CommandProcessor {
            pipe,
            session,
            ide_slave: slave,
        })
    }

    pub async fn kill_session(mut self) -> io::Result<()> {
        self.ide_slave.quit().await?;
        self.pipe.shutdown().await?;

        Ok(())
    }

    pub async fn process_command<'a>(&'a mut self) -> io::Result<Option<Command<'a>>> {
        loop {
            let kind = CommandKind::parse_from(&mut self.pipe).await?;

            log::debug!("Received command '{:?}' through control pipe", kind);
            match kind {
                Some(CommandKind::Nop) => {}
                k => {
                    break Ok(k.map(|kind| Command {
                        session: &self.session,
                        slave: &mut self.ide_slave,
                        kind,
                    }))
                }
            }
        }
    }
}

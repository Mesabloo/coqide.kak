use super::command::{Command, CommandKind};
use crate::coqtop::slave::IdeSlave;
use std::io;
use tokio::fs::File;
use unix_named_pipe as fifos;

pub struct CommandProcessor {
    pipe: File,
    session: String,
    ide_slave: IdeSlave,
}

impl CommandProcessor {
    pub async fn init(pipes_path: String, session: String, slave: IdeSlave) -> io::Result<Self> {
        log::debug!("Opening command pipe '{}/cmd'", pipes_path);
        let pipe = File::from(fifos::open_read(format!("{}/cmd", pipes_path))?);
        log::debug!("Pipe opened!");

        Ok(CommandProcessor {
            pipe,
            session,
            ide_slave: slave,
        })
    }

    pub async fn kill_session(self) -> io::Result<()> {
        self.ide_slave.quit().await?;

        Ok(())
    }

    pub async fn process_command<'a>(&'a mut self) -> io::Result<Command<'a>> {
        let kind = loop {
            let kind = CommandKind::parse_from(&mut self.pipe).await?;

            log::debug!("Received command '{:?}' through control pipe", kind);
            match kind {
                Nop => {}
                k => break k,
            }
        };

        Ok(Command {
            session: &self.session,
            slave: &mut self.ide_slave,
            kind,
        })
    }
}

use super::command::{Command, CommandKind};
use crate::coqtop::slave::IdeSlave;
use async_std::fs::File;
use futures::io::BufReader;
use std::io;

pub struct CommandProcessor {
    pipe: BufReader<File>,
    session: String,
    ide_slave: IdeSlave,
}

impl CommandProcessor {
    pub async fn init(pipe_path: String, session: String, slave: IdeSlave) -> io::Result<Self> {
        Ok(CommandProcessor {
            pipe: BufReader::new(File::open(pipe_path).await?),
            session,
            ide_slave: slave,
        })
    }

    pub async fn kill_session(self) -> io::Result<()> {
        self.ide_slave.quit().await?;

        Ok(())
    }

    pub async fn process_command<'a>(&'a self) -> io::Result<Command<'a>> {
        Ok(Command {
            session: &self.session,
            slave: &self.ide_slave,
            kind: parse_command(&self.pipe).await,
        })
    }
}

use CommandKind::*;

async fn parse_command(pipe: &BufReader<File>) -> CommandKind {
  Init
}

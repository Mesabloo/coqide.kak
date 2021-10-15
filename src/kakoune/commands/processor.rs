use std::{future::Future, io, rc::Rc, sync::Arc};

use tokio::fs::File;

use crate::{
    coqtop::slave::IdeSlave,
    kakoune::{
        session::SessionWrapper,
        slave::{command_file, KakSlave},
    },
};

use super::types::Command;

pub struct CommandProcessor {
    session: Arc<SessionWrapper>,
    ideslave: Rc<IdeSlave>,

    command_file: File,
}

impl CommandProcessor {
    pub fn new(session: Arc<SessionWrapper>, ideslave: Rc<IdeSlave>) -> io::Result<Self> {
        Ok(Self {
            session: session.clone(),
            ideslave,
            command_file: File::from(unix_named_pipe::open_read(command_file(
                session.clone(),
            ))?),
        })
    }

    pub async fn process_next_command(&mut self, kak_slave: &mut KakSlave<'_>) -> io::Result<()> {
        match Command::parse_from(&mut self.command_file).await? {
            None => Err(io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe")),
            Some(None) => Ok(()),
            Some(Some(cmd)) => Ok(()),
            // TODO: process `cmd`
        }
    }
}

impl Drop for CommandProcessor {
    fn drop(&mut self) {
        drop(&mut self.command_file);
        drop(&mut self.session);
        drop(&mut self.ideslave);
    }
}

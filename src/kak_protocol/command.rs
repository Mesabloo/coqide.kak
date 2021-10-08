use std::io;
use crate::coqtop::slave::IdeSlave;

pub struct Command<'a> {
    pub session: &'a String,
    pub slave: &'a IdeSlave,
    pub kind: CommandKind,
}

pub enum CommandKind {
    Init,
}

impl<'a> Command<'a> {
    pub async fn execute(self) -> io::Result<()> {
        Ok(())
    }
}

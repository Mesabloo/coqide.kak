use std::{io, rc::Rc, sync::Arc};

use crate::{coqtop::slave::IdeSlave, daemon::DaemonState};

use super::session::SessionWrapper;

/// The Kakoune slave is used to bridge between the daemon and Kakoune through the [`command_file`] file.
pub struct KakSlave<'a> {
    session: Arc<SessionWrapper>,
    ideslave: Rc<IdeSlave>,
    ext_state: &'a mut DaemonState,
}

impl<'a> KakSlave<'a> {
    /// Creates a new Kakoune slave.
    pub fn new(
        session: Arc<SessionWrapper>,
        ideslave: Rc<IdeSlave>,
        ext_state: &'a mut DaemonState,
    ) -> io::Result<Self> {
        Ok(Self {
            session,
            ideslave,
            ext_state,
        })
    }
}

/// Retrieves the path to the command file from a session.
pub fn command_file(session: Arc<SessionWrapper>) -> String {
    format!("{}/cmd", session.tmp_dir())
}

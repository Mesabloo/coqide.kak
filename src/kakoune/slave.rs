use std::{cell::RefCell, io, rc::Rc, sync::Arc};

use tokio::sync::RwLock;

use crate::{coqtop::slave::IdeSlave, daemon::DaemonState};

use super::session::SessionWrapper;

/// The Kakoune slave is used to bridge between the daemon and Kakoune through the [`command_file`] file.
#[derive(Clone)]
pub struct KakSlave {
    session: Arc<SessionWrapper>,
    pub ideslave: Arc<RwLock<IdeSlave>>,
    pub ext_state: Arc<RwLock<DaemonState>>,
}

impl KakSlave {
    /// Creates a new Kakoune slave.
    pub fn new(
        session: Arc<SessionWrapper>,
        ideslave: Arc<RwLock<IdeSlave>>,
        ext_state: Arc<RwLock<DaemonState>>,
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

use std::sync::Arc;

use crate::kakoune::session::SessionWrapper;

pub fn result_file(session: Arc<SessionWrapper>) -> String {
    format!("{}/result", session.tmp_dir())
}

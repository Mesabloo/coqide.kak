use std::sync::Arc;

use crate::kakoune::session::SessionWrapper;

pub fn goal_file(session: Arc<SessionWrapper>) -> String {
    format!("{}/goal", session.tmp_dir())
}

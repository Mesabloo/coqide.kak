use std::sync::Arc;

pub struct Session {
    kak_client: String,
    kak_session: String,
    coq_file: String,
    tmp_dir: String,
    fifo_path: String,
}

impl Session {
    /// Creates a new atomically managed session containing the identifier of the kakoune client,
    /// the name of the Coq file being edited and the path to a temporary directory containing various utility files.
    pub fn new(
        kak_client: String,
        kak_session: String,
        coq_file: String,
        tmp_dir: String,
        fifo_path: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            kak_client,
            kak_session,
            coq_file,
            tmp_dir,
            fifo_path,
        })
    }
}

/// Retrieves the name of the Kakoune client the process has been started in.
pub fn client_name(s: Arc<Session>) -> String {
    s.kak_client.clone()
}

/// Retrieves the identifier of the Kakoune session.
pub fn session_id(s: Arc<Session>) -> String {
    s.kak_session.clone()
}

/// Gets the name of the edited Coq file.
pub fn edited_file(s: Arc<Session>) -> String {
    s.coq_file.clone()
}

/// Gets the path of the temporary folder.
pub fn temporary_folder(s: Arc<Session>) -> String {
    s.tmp_dir.clone()
}

pub fn input_fifo(s: Arc<Session>) -> String {
    s.fifo_path.clone()
}

/// A session wrapper contains the ID of the current kakoune session,
/// and the directory containing all necessary files to communicate with it.
pub struct SessionWrapper {
    session: String,
    tmp_dir: String,
}

impl SessionWrapper {
    /// Creates a new wrapper from the session ID and the directory which will contain
    /// all the communication files.
    pub fn new(session: String, tmp_dir: String) -> Self {
        SessionWrapper { session, tmp_dir }
    }

    /// Retrieves the session ID from the wrapper.
    pub fn id(&self) -> &String {
        &self.session
    }

    /// Retrieves the directory containing communication files.
    pub fn tmp_dir(&self) -> &String {
        &self.tmp_dir
    }
}

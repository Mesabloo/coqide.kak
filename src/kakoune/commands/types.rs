#[derive(Debug)]
/// A command received from Kakoune.
pub enum Command {
    /// Initialise the daemon.
    Init,
    /// Stop the daemon.
    Quit,
    /// Go to the last proven statement.
    Previous,
    /// Rewind back to the given line and column numbers.
    RewindTo(u64, u64),
}

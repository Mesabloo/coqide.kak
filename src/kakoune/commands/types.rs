use crate::state::CodeSpan;

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
    /// Send a query directly to [`COQTOP`] in a disposable environment.
    ///
    /// [`COQTOP`]: crate::coqtop::slave::COQTOP
    Query(String),
    /// Process all the given statements (which correspond to until where the cursor is).
    MoveTo(Vec<(CodeSpan, String)>),
    /// Try to process the next statement.
    Next(CodeSpan, String),
    /// Allow bypassing the last error range reported, without removing it from the UI.
    IgnoreError,
    /// Ask for hints for the current proof.
    Hints
}

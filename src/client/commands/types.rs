use crate::{
    coqtop::xml_protocol::types::{ProtocolRichPP, ProtocolValue},
    range::Range,
};

#[derive(Debug, Clone)]
pub enum ClientCommand {
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
    MoveTo(Vec<(Range, String)>),
    /// Try to process the next statement.
    Next(Range, String),
    /// Allow bypassing the last error range reported, without removing it from the UI.
    IgnoreError,
    /// Ask for hints for the current proof.
    Hints,
    /// Ask the [`COQTOP`] process to output the current goals.
    ShowGoals,
}

/// The type of commands that can be sent back to Kakoune.
#[derive(Debug, Clone)]
pub enum DisplayCommand {
    /// Refresh the error range.
    RefreshErrorRange(Option<Range>),
    /// Output the result with colors.
    ColorResult(ProtocolRichPP, bool),
    /// Show some goals.
    OutputGoals(
        /// Foreground (focused) goals.
        Vec<ProtocolValue>,
        /// Background tasks.
        Vec<(Vec<ProtocolValue>, Vec<ProtocolValue>)>,
        /// Given up goals.
        Vec<ProtocolValue>,
    ),
    /// Add a range to the processed range.
    AddToProcessed(Range),
    /// Remove a range from the processed code.
    RemoveProcessed(Range),
}

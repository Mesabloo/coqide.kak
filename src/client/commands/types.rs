use crate::{
    coqtop::xml_protocol::types::{ProtocolRichPP, ProtocolValue},
    range::Range,
    state::Operation,
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
    Next(bool, Range, String),
    /// Allow bypassing the last error range reported, without removing it from the UI.
    IgnoreError,
    /// Ask for hints for the current proof.
    Hints,
    /// Ask the [`COQTOP`] process to output the current goals.
    ShowGoals(Range),
    /// Internal use: go back to the given state ID.
    BackTo(Operation),
    /// Show the status of the worker.
    Status,
}

/// The type of commands that can be sent back to Kakoune.
#[derive(Debug, Clone)]
pub enum DisplayCommand {
    /// Refresh the error range.
    RefreshErrorRange(
        Option<Range>,
        /// Do we force updating?
        bool,
    ),
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
        /// Shelved goals (e.g. uninstanciated existentials)
        Vec<ProtocolValue>,
    ),
    /// Remove a range from the to be processed range.
    RemoveToBeProcessed(Range),
    /// Add a range to the processed range.
    AddToProcessed(Range),
    /// Remove a range from the processed code.
    RemoveProcessed(Range),
    /// Go to the tip of the processed code.
    GotoTip,
    /// Add a range to the axiom highlighter.
    AddAxiom(Range),
    /// Remove an axiom from the axiom highlighter.
    RemoveAxiom(Range),
    /// Show the status of the worker.
    ShowStatus(String, String),
}

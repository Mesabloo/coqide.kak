use crate::{
    coqtop::xml_protocol::types::{ProtocolRichPP, ProtocolValue},
    state::CodeSpan,
};

/// The type of commands that can be sent back to Kakoune.
#[derive(Debug, Clone)]
pub enum DisplayCommand {
    /// Refresh the processed range.
    RefreshProcessedRange(CodeSpan),
    /// Output the result with colors.
    ColorResult(ProtocolRichPP),
    /// Show some goals.
    OutputGoals(
        /// Foreground (focused) goals.
        Vec<ProtocolValue>,
        /// Background tasks.
        Vec<(Vec<ProtocolValue>, Vec<ProtocolValue>)>,
    ),
}

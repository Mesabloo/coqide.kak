use super::parser::XMLNode;

#[derive(Debug, Clone)]
pub enum ProtocolValue {
    /// `<unit/>` which holds no value
    Unit,
    /// `<list>...</list>` for a list value
    List(Vec<ProtocolValue>),
    /// `<string>...</string>` represents string literals
    Str(String),
    /// `<int>...</int>` represents a single integer
    Int(i64),
    /// `<bool val="..."/>` represents simple boolean (`true` | `false`) values
    Boolean(bool),
    /// `<pair>'v1''v2'</pair>` is a pair containing both values `v1` and `v2`
    Pair(Box<ProtocolValue>, Box<ProtocolValue>),
    /// `<option val="none"/>` and `<option val="some">v1</option>` represents optional values
    Optional(Option<Box<ProtocolValue>>),
    /// `<state_id val="..."/>` represents a number
    StateId(i64),
    /// `<route_id val="..."/>`
    RouteId(i64),

    /// `<status>'paths''proofName''allProofs''proofNumber'</status>`
    ///
    /// - `paths`: Module path of the current proof
    /// - `proofName`: Current proof name. `None` if no focused proof is in progress
    /// - `allProofs`: List of all pending proofs. Order is not significant
    /// - `proofNumber`: An id describing the state of the current proof
    Status(
        Box<ProtocolValue>,
        Box<ProtocolValue>,
        Box<ProtocolValue>,
        Box<ProtocolValue>,
    ),

    /// An unknown value has been decoded
    Unknown(XMLNode),
}

/// Result returned by `coqidetop` on query.
#[derive(Debug, Clone)]
pub enum ProtocolResult {
    /// Everything went well, and `coqidetop` responded with some value.
    Good(
        /// A value trasmitted with the good response.
        ProtocolValue,
    ),
    /// An error occured.
    Fail(
        /// The optional line number where the error occured.
        Option<i64>,
        /// The optional column number where the error occured.
        Option<i64>,
        /// An associated error message describing what has gone wrong.
        ProtocolRichPP,
    ),
    /// Feedback from the daemon.
    Feedback(
        /// **UNUSED**
        ///
        /// The object the feedback relates to.
        String,
        /// **UNUSED**
        ///
        /// The route ID of the feedback.
        String,
        /// The state ID the feedback relates to.
        ProtocolValue,
        /// The content of the feedback.
        FeedbackContent,
    ),
}

/// The content of a feedback [`ProtocolResult`].
#[derive(Debug, Clone)]
pub enum FeedbackContent {
    /// A simple RichPP message.
    Message(ProtocolRichPP),
    /// Some piece of code has been processed.
    Processed,
    /// Worker status
    WorkerStatus(XMLNode),
    /// Processing some call
    Processing(XMLNode),
}

/// The type of pretty-printed text.
#[derive(Debug, Clone)]
pub enum ProtocolRichPP {
    /// The raw text contained inside a `<richpp>` node, where child nodes are also rendered.
    Raw(String),
}

/// Commands that `coqidetop` can understand.
#[derive(Debug, Clone)]
pub enum ProtocolCall {
    /// Initialize the process.
    Init(
        /// This [`ProtocolValue`] must be of the form `Optional(_)`.
        ProtocolValue,
    ),
    /// Quit.
    Quit,
    /// Go back to a previous state.
    EditAt(
        /// Represents the state ID to go back to.
        i64,
    ),
    /// Query some Coq statements in a disposable context.
    Query(
        /// The value transported must be of the form `Pair(box RouteId(_), Pair(box Str(_), box StateId(_)))`.
        ProtocolValue,
    ),
    /// Fetch some hints.
    Hints,
    /// Get all current goals.
    Goal,
    /// Send some piece of code to [`COQTOP`].
    ///
    /// [`COQTOP`]: crate::coqtop::slave::COQTOP
    Add(
        /// The code to be sent for verification.
        String,
        /// The state ID it is tied to.
        i64,
    ),
}

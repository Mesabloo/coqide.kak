use super::parser::XMLNode;

/*
Protocol documentation: https://github.com/coq/coq/blob/master/ide/coqide/protocol/interface.ml
 */

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
    /// `<union val="in_l">v1</union>` represents a left value of a sumtype
    Inl(Box<ProtocolValue>),
    /// `<union val="in_r">v1</union>` represents a right value of a sumtype
    Inr(Box<ProtocolValue>),

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

    /// `<goals>'fg''bg''sg''gg'</goals>`
    ///
    /// - `fg`: focused goals
    /// - `bg`: unfocused (background) goals
    /// - `sg`: shelved (pending) goals
    /// - `gg`: given up goals
    Goals(
        /// The list of focused goals.
        Vec<ProtocolValue>,
        /// The list of unfocused (background) goals.
        Vec<(Vec<ProtocolValue>, Vec<ProtocolValue>)>,
        /// The list of all pending proofs.
        Vec<ProtocolValue>,
        /// The list of proofs which have been given up.
        Vec<ProtocolValue>,
    ),

    /// `<goal>'name''hyp''ccl''user-name'</goal>`
    ///
    /// - `name`: a `<string>` containing a unique identifier
    /// - `hyp`: a `<list>` of [`ProtocolRichPP`] seen as the hypotheses of the goal
    /// - `ccl`: a [`ProtocolRichPP`] representing the conclusion of the goal
    /// - `user-name`: an [`ProtocolValue::Optional`] `<string>` for a user-given name
    Goal(
        /// The name of the goal.
        Box<ProtocolValue>,
        /// A list of hypotheses.
        Vec<ProtocolRichPP>,
        /// The conclusion of the goal.
        ProtocolRichPP,
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
        /// The state ID to go back to on error.
        ProtocolValue,
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
///
/// See here for a complete list of feedback objects:
/// [https://github.com/coq/coq/blob/master/lib/feedback.ml]
#[derive(Debug, Clone)]
pub enum FeedbackContent {
    /// A simple RichPP message.
    Message(MessageType, ProtocolRichPP),
    /// Some piece of code has been processed.
    Processed,
    /// Worker status
    WorkerStatus(XMLNode),
    /// Processing some call
    Processing(XMLNode),
    /// Some proof has been temporarily admitted
    AddedAxiom,
}

/// The level of the message sent by [`COQTOP`].
#[derive(Debug, Clone)]
pub enum MessageType {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
}

/// The type of pretty-printed text.
#[derive(Debug, Clone)]
pub enum ProtocolRichPP {
    /// All parts in the RichPP message.
    RichPP(Vec<ProtocolRichPPPart>),
}

/// The type of parts of a RichPP message.
///
/// All construction classes are found here:
/// [https://github.com/coq/coq/blob/8cd67a3976050f20e77f5c033d09f8da70d5a49f/printing/ppconstr.ml#L28-L34]
#[derive(Debug, Clone)]
pub enum ProtocolRichPPPart {
    /// Some raw string in the richpp node.
    Raw(String),
    /// A Coq keyword.
    Keyword(String),
    /// ???
    Evar(String),
    /// A Coq type.
    Type(String),
    /// Some Coq notation.
    Notation(String),
    /// Some text which should be highlighted as a variable.
    Variable(String),
    /// ???
    Reference(String),
    /// ???
    Path(String),
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

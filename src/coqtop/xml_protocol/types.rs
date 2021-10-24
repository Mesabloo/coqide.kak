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
#[derive(Debug)]
pub enum ProtocolResult {
    /// Everything went well, and `coqidetop` responded with some value.
    Good(ProtocolValue),
    /// An error occured.
    Fail(Option<i64>, Option<i64>, ProtocolRichPP),
    /// Feedback from the daemon.
    Feedback(String, String, ProtocolValue, XMLNode),
}

/// The type of pretty-printed text.
#[derive(Debug)]
pub enum ProtocolRichPP {
    /// TMP
    Raw(String),
}

/// Commands that `coqidetop` can understand.
#[derive(Debug, Clone)]
pub enum ProtocolCall {
    /// Initialize the process.
    Init(ProtocolValue),
    /// Quit.
    Quit,
    /// Go back to a previous state.
    EditAt(i64),
    /// Query some Coq statements in a disposable context.
    ///
    /// The value transported must be of the form `Pair(RouteId(_), Pair(String(_), StateId(_)))`.
    Query(ProtocolValue),
    /// Fetch some hints.
    Hints,
    /// Get all current goals.
    Goal,
}

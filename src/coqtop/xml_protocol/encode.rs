use super::types::ProtocolCall::{self, *};
use super::types::ProtocolValue::{self, *};

impl ProtocolValue {
    /// Encode a protocol value as XML to be sent to the `coqidetop` process.
    pub fn encode(self) -> String {
        match self {
            Unit => "<unit/>".to_string(),
            List(vs) => format!(
                "<list>{}</list>",
                vs.into_iter()
                    .map(ProtocolValue::encode)
                    .collect::<Vec<_>>()
                    .join("")
            ),
            Str(s) => format!("<string>{}</string>", escape(s)),
            Int(i) => format!("<int>{}</int>", i),
            Boolean(b) => format!("<bool val=\"{}\"/>", b),
            Pair(box v1, box v2) => format!("<pair>{}{}</pair>", v1.encode(), v2.encode()),
            Optional(opt) => opt.map_or_else(
                || "<option val=\"none\"/>".to_string(),
                |val| format!("<option val=\"some\">{}</option>", val.encode()),
            ),
            StateId(id) => format!("<state_id val=\"{}\"/>", id),
            RouteId(id) => format!("<route_id val=\"{}\"/>", id),
            Status(box ps, box pn, box pa, box nb) => format!(
                "<status>{}{}{}{}</status>",
                ps.encode(),
                pn.encode(),
                pa.encode(),
                nb.encode()
            ),
            Unknown(_) => format!(""),
        }
    }
}

/// Escapes special characters like `&` or `<` to their XML equivalents `&amp;`, `&lt;`, etc.
///
/// Currently, this only escapes those characters:
/// - `&` → `&amp;`
/// - `<` → `&lt;`
/// - `>` → `&gt;`
fn escape(str: String) -> String {
    str.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
    // .replace(" ", "&nbsp;")
    // .replace("'", "&apos;")
}

impl ProtocolCall {
    /// Encode a protocol call as XML to be sent to the `coqidetop` process.
    pub fn encode(self) -> String {
        match self {
            Init(val) => format!("<call val=\"Init\">{}</call>", val.encode()),
            EditAt(state_id) => format!(
                "<call val=\"Edit_at\">{}</call>",
                StateId(state_id).encode()
            ),
            Quit => format!("<call val=\"Quit\">{}</call>", Unit.encode()),
            Query(val) => format!("<call val=\"Query\">{}</call>", val.encode()),
            Goal => format!("<call val=\"Goal\">{}</call>", Unit.encode()),
            Hints => format!("<call val=\"Hints\">{}</call>", Unit.encode()),
            Add(code, state_id) => format!(
                "<call val=\"Add\">{}</call>",
                ProtocolValue::Pair(
                    Box::new(ProtocolValue::Pair(
                        Box::new(ProtocolValue::Str(code)),
                        Box::new(ProtocolValue::Int(2))
                    )),
                    Box::new(ProtocolValue::Pair(
                        Box::new(ProtocolValue::StateId(state_id)),
                        Box::new(ProtocolValue::Boolean(true))
                    ))
                )
                .encode()
            ),
        }
    }
}

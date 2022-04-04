use super::types::ProtocolCall;
use super::types::ProtocolValue;

impl ProtocolValue {
    /// Encode a protocol value as XML to be sent to the `coqidetop` process.
    pub fn encode(self) -> String {
        use ProtocolValue::*;

        match self {
            Unit => "<unit/>".to_string(),
            List(vs) => format!(
                "<list>{}</list>",
                vs.into_iter()
                    .map(ProtocolValue::encode)
                    .collect::<Vec<_>>()
                    .join("")
            ),
            Inl(box val) => format!("<union val=\"in_l\">{}</union>", val.encode()),
            Inr(box val) => format!("<union val=\"in_r\">{}</union>", val.encode()),
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
            // We should never have to encode a goal, only decode them.
            Goals(_, _, _, _) => unreachable!(),
            Goal(_, _, _) => unreachable!(),
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
        use ProtocolCall::*;

        match self {
            Init(val) => format!("<call val=\"Init\">{}</call>", val.encode()),
            EditAt(state_id) => format!(
                "<call val=\"Edit_at\">{}</call>",
                ProtocolValue::StateId(state_id).encode()
            ),
            Quit => format!("<call val=\"Quit\">{}</call>", ProtocolValue::Unit.encode()),
            Query(val) => format!("<call val=\"Query\">{}</call>", val.encode()),
            Goal => format!("<call val=\"Goal\">{}</call>", ProtocolValue::Unit.encode()),
            Hints => format!(
                "<call val=\"Hints\">{}</call>",
                ProtocolValue::Unit.encode()
            ),
            Add(code, state_id) => format!(
                "<call val=\"Add\">{}</call>",
                ProtocolValue::Pair(
                    Box::new(ProtocolValue::Pair(
                        Box::new(ProtocolValue::Str(code)),
                        Box::new(ProtocolValue::Int(-1))
                    )),
                    Box::new(ProtocolValue::Pair(
                        Box::new(ProtocolValue::StateId(state_id)),
                        Box::new(ProtocolValue::Boolean(true))
                    ))
                )
                .encode()
            ),
            Status(force) => format!("<call val=\"Status\">{}</call>", force.encode()),
        }
    }
}

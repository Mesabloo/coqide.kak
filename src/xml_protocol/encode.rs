use super::types::ProtocolCall::{self, *};
use super::types::ProtocolValue::{self, *};

impl ProtocolValue {
    /// Encode a protocol value as XML to be sent to the `coqidetop` process
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
            Str(s) => format!("<string>{}</string>", s),
            Int(i) => format!("<int>{}</int>", i),
            Boolean(b) => format!("<bool val=\"{}\"/>", b),
            Pair(box v1, box v2) => format!("<pair>{}{}</pair>", v1.encode(), v2.encode()),
            Optional(opt) => opt.map_or_else(
                || "<option val=\"none\"/>".to_string(),
                |val| format!("<option val=\"some\">{}</option>", val.encode()),
            ),
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

impl ProtocolCall {
    pub fn encode(self) -> String {
        match self {
            Init(val) => format!("<call val=\"Init\">{}</call>", val.encode()),
            Quit => format!("<call val=\"Quit\">{}</call>", Unit.encode()),
        }
    }
}

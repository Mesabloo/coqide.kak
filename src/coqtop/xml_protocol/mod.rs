use std::fmt;

/// Decode results, values, etc from a XML string.
pub mod decode;
/// Encodes calls and values into XML.
pub mod encode;
/// An implementation of a very simple XML parser.
pub mod parser;
/// Various types used to communicate through the XML protocol.
pub mod types;

impl types::ProtocolRichPP {
    pub fn strip(self) -> Self {
        let types::ProtocolRichPP::RichPP(parts) = self;
        types::ProtocolRichPP::RichPP(
            parts
                .into_iter()
                .map(types::ProtocolRichPPPart::strip_colors)
                .collect::<Vec<_>>(),
        )
    }
}

impl types::ProtocolRichPPPart {
    pub fn strip_colors(self) -> Self {
        use types::ProtocolRichPPPart::*;

        match self {
            Keyword(kw) => Raw(kw),
            Evar(v) => Raw(v),
            Type(ty) => Raw(ty),
            Notation(ty) => Raw(ty),
            Variable(v) => Raw(v),
            Reference(r) => Raw(r),
            Path(p) => Raw(p),
            c => c,
        }
    }
}

impl fmt::Display for types::ProtocolRichPPPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use types::ProtocolRichPPPart::*;

        write!(
            f,
            "{}",
            match self {
                Keyword(kw) => kw,
                Evar(v) => v,
                Type(ty) => ty,
                Notation(n) => n,
                Variable(v) => v,
                Reference(r) => r,
                Path(p) => p,
                Raw(raw) => raw,
            }
        )
    }
}

impl fmt::Display for types::ProtocolRichPP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let types::ProtocolRichPP::RichPP(parts) = self;
        for p in parts {
            write!(f, "{}", p)?;
        }
        Ok(())
    }
}

use std::fmt;

use self::types::{ProtocolRichPP, ProtocolRichPPPart};

/// Decode results, values, etc from a XML string.
pub mod decode;
/// Encodes calls and values into XML.
pub mod encode;
/// An implementation of a very simple XML parser.
pub mod parser;
/// Various types used to communicate through the XML protocol.
pub mod types;

impl ProtocolRichPP {
    pub fn strip(self) -> Self {
        let ProtocolRichPP::RichPP(parts) = self;
        ProtocolRichPP::RichPP(
            parts
                .into_iter()
                .map(ProtocolRichPPPart::strip_colors)
                .collect::<Vec<_>>(),
        )
    }

    pub fn warning(self) -> Self {
        use ProtocolRichPPPart::*;

        let ProtocolRichPP::RichPP(parts) = self.strip();
        ProtocolRichPP::RichPP(
            parts
                .into_iter()
                .map(|part| match part {
                    Raw(str) => Warning(str),
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>(),
        )
    }

    pub fn error(self) -> Self {
        use ProtocolRichPPPart::*;

        let ProtocolRichPP::RichPP(parts) = self.strip();
        ProtocolRichPP::RichPP(
            parts
                .into_iter()
                .map(|part| match part {
                    Raw(str) => Error(str),
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>(),
        )
    }
}

impl ProtocolRichPPPart {
    pub fn strip_colors(self) -> Self {
        use ProtocolRichPPPart::*;

        match self {
            Keyword(kw) => Raw(kw),
            Evar(v) => Raw(v),
            Type(ty) => Raw(ty),
            Notation(ty) => Raw(ty),
            Variable(v) => Raw(v),
            Reference(r) => Raw(r),
            Path(p) => Raw(p),
            Error(e) => Raw(e),
            Warning(w) => Raw(w),
            c => c,
        }
    }
}

impl fmt::Display for ProtocolRichPPPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ProtocolRichPPPart::*;

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
                Warning(w) => w,
                Error(e) => e,
            }
        )
    }
}

impl fmt::Display for ProtocolRichPP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ProtocolRichPP::RichPP(parts) = self;
        for p in parts {
            write!(f, "{}", p)?;
        }
        Ok(())
    }
}

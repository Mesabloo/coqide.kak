use super::{
    parser::{XMLDecoder, XMLNode},
    types::{
        FeedbackContent,
        ProtocolResult::{self, *},
        ProtocolRichPP::{self, *},
        ProtocolValue::{self, *},
    },
};
use std::io;
use tokio::io::AsyncRead;

pub enum DecodeError {
    InvalidUnit,
    InvalidList,
    InvalidString,
    InvalidInteger,
    InvalidBoolean,
    InvalidPair,
    InvalidOption,
    InvalidStatus,
    InvalidValue,
    InvalidRichPP,
    InvalidStateId,
    InvalidFeedback,
    InvalidFeedbackContent,
    InvalidGoals,
    InvalidGoal,
}

use tokio_util::codec::FramedRead;
use DecodeError::*;

impl std::fmt::Debug for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidUnit => write!(f, "Invalid <unit/> tag"),
            InvalidList => write!(f, "Invalid <list/> tag"),
            InvalidString => write!(f, "Invalid <string/> tag"),
            InvalidInteger => write!(f, "Invalid <int/> tag"),
            InvalidBoolean => write!(f, "Invalid <bool/> tag"),
            InvalidPair => write!(f, "Invalid <pair/> tag"),
            InvalidOption => write!(f, "Invalid <option/> tag"),
            InvalidStatus => write!(f, "Invalid <status/> tag"),
            InvalidValue => write!(f, "Invalid <value/> tag"),
            InvalidRichPP => write!(f, "Invalid <richpp/> tag"),
            InvalidStateId => write!(f, "Invalid <state_id/> tag"),
            InvalidFeedback => write!(f, "Invalid <feedback/> tag"),
            InvalidFeedbackContent => write!(f, "Invalid <feedback_content/> tag"),
            InvalidGoals => write!(f, "Invalid <goals/> tag"),
            InvalidGoal => write!(f, "Invalid <goal> tag"),
        }
    }
}

impl ProtocolValue {
    /// Tries to decode a protocol value from an XML `Element`.
    ///
    /// May throw a `DecodeError` if the value is malformed.
    pub fn decode(xml: XMLNode) -> io::Result<Self> {
        match xml.name.as_str() {
            "unit" => {
                assert_decode_error(xml.attributes.is_empty(), || InvalidUnit)?;
                assert_decode_error(xml.children.is_empty(), || InvalidUnit)?;

                Ok(Unit)
            }
            "list" => {
                assert_decode_error(xml.attributes.is_empty(), || InvalidList)?;

                xml.children
                    .iter()
                    .filter_map(|el| el.as_node())
                    .map(|el| ProtocolValue::decode(el.clone()))
                    .collect::<Result<Vec<_>, _>>()
                    .map(List)
            }
            "string" => {
                assert_decode_error(xml.attributes.is_empty(), || InvalidString)?;

                Ok(Str(xml.get_text()))
            }
            "int" => {
                assert_decode_error(xml.attributes.is_empty(), || InvalidInteger)?;

                xml.get_text()
                    .parse::<i64>()
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", err)))
                    .map(Int)
            }
            "bool" => {
                assert_decode_error(xml.children.is_empty(), || InvalidBoolean)?;
                assert_decode_error(xml.attributes.get("val").is_some(), || InvalidBoolean)?;

                let val = xml.attributes.get("val").unwrap();
                val.parse::<bool>()
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", err)))
                    .map(Boolean)
            }
            "pair" => {
                assert_decode_error(xml.children.len() == 2, || InvalidPair)?;
                assert_decode_error(xml.attributes.is_empty(), || InvalidPair)?;

                let mut vals = xml
                    .children
                    .iter()
                    .filter_map(|el| el.as_node())
                    .map(|el| ProtocolValue::decode(el.clone()))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Pair(Box::new(vals.remove(0)), Box::new(vals.remove(0))))
            }
            "option" => {
                assert_decode_error(!xml.attributes.is_empty(), || InvalidOption)?;
                assert_decode_error(xml.attributes.get("val").is_some(), || InvalidOption)?;

                let val = xml.attributes.get("val").unwrap();
                match val.as_str() {
                    "some" => {
                        assert_decode_error(xml.children.len() == 1, || InvalidOption)?;

                        Ok(Optional(
                            xml.children[0]
                                .as_node()
                                .map(|el| ProtocolValue::decode(el.clone()).map(Box::new))
                                .transpose()?,
                        ))
                    }
                    "none" => {
                        assert_decode_error(xml.children.is_empty(), || InvalidOption)?;

                        Ok(Optional(None))
                    }
                    _ => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{:?}", InvalidOption),
                    )),
                }
            }
            "state_id" => {
                assert_decode_error(!xml.attributes.is_empty(), || InvalidStateId)?;
                assert_decode_error(xml.attributes.get("val").is_some(), || InvalidStateId)?;

                let val = xml
                    .attributes
                    .get("val")
                    .unwrap()
                    .parse::<i64>()
                    .map_err(|err| {
                        io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", err))
                    })?;
                Ok(StateId(val))
            }
            "route_id" => {
                assert_decode_error(!xml.attributes.is_empty(), || InvalidStateId)?;
                assert_decode_error(xml.attributes.get("val").is_some(), || InvalidStateId)?;

                let val = xml
                    .attributes
                    .get("val")
                    .unwrap()
                    .parse::<i64>()
                    .map_err(|err| {
                        io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", err))
                    })?;
                Ok(RouteId(val))
            }
            "status" => {
                assert_decode_error(xml.children.len() == 4, || InvalidStatus)?;

                let mut children = xml
                    .children
                    .iter()
                    .filter_map(|el| el.as_node())
                    .map(|el| ProtocolValue::decode(el.clone()))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(Status(
                    Box::new(children.remove(0)),
                    Box::new(children.remove(0)),
                    Box::new(children.remove(0)),
                    Box::new(children.remove(0)),
                ))
            }
            "goals" => {
                assert_decode_error(xml.children.len() == 4, || InvalidGoals)?;
                let fg =
                    ProtocolValue::decode(xml.children[0].clone().as_node().cloned().unwrap())?;
                // <list> of <goal>s
                let bg =
                    ProtocolValue::decode(xml.children[1].clone().as_node().cloned().unwrap())?;
                // <list> of <pair>s of <list>s of <goal>s
                let sg =
                    ProtocolValue::decode(xml.children[2].clone().as_node().cloned().unwrap())?;
                // <list> of <goal>s
                let gg =
                    ProtocolValue::decode(xml.children[2].clone().as_node().cloned().unwrap())?;
                // <list> of <goal>s

                let fg = match fg {
                    List(vs) => Ok(vs),
                    _ => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{:?}", InvalidGoals),
                    )),
                }?;
                let bg = match bg {
                    List(vs) => Ok(vs
                        .into_iter()
                        .filter_map(|v| match v {
                            Pair(box v1, box v2) => {
                                let v1 = match v1 {
                                    List(vs) => Some(vs),
                                    _ => None,
                                }?;
                                let v2 = match v2 {
                                    List(vs) => Some(vs),
                                    _ => None,
                                }?;
                                Some((v1, v2))
                            }
                            _ => None,
                        })
                        .collect()),
                    _ => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{:?}", InvalidGoals),
                    )),
                }?;
                let sg = match sg {
                    List(vs) => Ok(vs),
                    _ => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{:?}", InvalidGoals),
                    )),
                }?;
                let gg = match gg {
                    List(vs) => Ok(vs),
                    _ => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{:?}", InvalidGoals),
                    )),
                }?;

                Ok(Goals(fg, bg, sg, gg))
            }
            "goal" => {
                assert_decode_error(xml.children.len() == 3, || InvalidGoal)?;

                let name =
                    ProtocolValue::decode(xml.children[0].clone().as_node().cloned().unwrap())?;
                let hyps = {
                    let node = xml.children[1].as_node().cloned().unwrap();
                    match node.name.as_str() {
                        "list" => node
                            .children
                            .into_iter()
                            .filter_map(|child| ProtocolRichPP::decode(child.as_node().cloned().unwrap()).ok())
                            .collect(),
                        _ => Vec::new(),
                    }
                };
                let ccl = ProtocolRichPP::decode(xml.children[2].as_node().cloned().unwrap())?;

                Ok(Goal(Box::new(name), hyps, ccl))
            }
            _ => Ok(Unknown(xml)),
        }
    }
}

impl ProtocolResult {
    /// Tries to decode a [`ProtocolResult`] from a [`XMLNode`].
    pub fn decode(xml: XMLNode) -> io::Result<Self> {
        match xml.name.as_str() {
            "value" => {
                assert_decode_error(xml.attributes.len() >= 1, || InvalidValue)?;
                assert_decode_error(xml.attributes.get("val").is_some(), || InvalidValue)?;

                let val = xml.attributes.get("val").unwrap();
                match val.as_str() {
                    "good" => ProtocolValue::decode(
                        xml.children[0]
                            .as_node()
                            .ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("{:?}", InvalidValue),
                                )
                            })?
                            .clone(),
                    )
                    .map(Good),
                    "fail" => {
                        let loc_s = xml
                            .attributes
                            .get("loc_s")
                            .map(|str| str.parse::<i64>())
                            .transpose()
                            .map_err(|err| {
                                io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", err))
                            })?;
                        let loc_e = xml
                            .attributes
                            .get("loc_e")
                            .map(|str| str.parse::<i64>())
                            .transpose()
                            .map_err(|err| {
                                io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", err))
                            })?;

                        let richpp_elem = xml
                            .get_child("richpp".to_string())
                            .ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("{:?}", InvalidValue),
                                )
                            })?
                            .clone();
                        let richpp = ProtocolRichPP::decode(richpp_elem)?;

                        Ok(Fail(loc_s, loc_e, richpp))
                    }
                    _ => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{:?}", InvalidValue),
                    )),
                }
            }
            "feedback" => {
                assert_decode_error(!xml.attributes.is_empty(), || InvalidFeedback)?;
                assert_decode_error(!xml.children.is_empty(), || InvalidFeedback)?;
                assert_decode_error(xml.attributes.get("object").is_some(), || InvalidFeedback)?;
                assert_decode_error(xml.attributes.get("route").is_some(), || InvalidFeedback)?;

                let object = xml.attributes.get("object").unwrap().clone();
                let route = xml.attributes.get("route").unwrap().clone();

                let state_id = xml.children[0].as_node().cloned().unwrap();
                let feedback_content = xml.children[1].as_node().cloned().unwrap();
                let feedback_content = FeedbackContent::decode(feedback_content)?;

                ProtocolValue::decode(state_id)
                    .map(|val| ProtocolResult::Feedback(object, route, val, feedback_content))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{:?}", InvalidValue),
            )),
        }
    }

    /// Tries to decode a [`ProtocolResult`] from a streaming [`AsyncRead`] by first decoding a [`XMLNode`]
    /// and then using [`ProtocolResult::decode`] on it.
    pub async fn decode_stream<R>(reader: &mut FramedRead<R, XMLDecoder>) -> io::Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        tokio::select! {
            Ok(elem) = XMLNode::decode_stream(reader) => {
                ProtocolResult::decode(elem)
            }
            else => Err(io::Error::new(io::ErrorKind::BrokenPipe, "Cannot decode XML node")),
        }
    }
}

impl ProtocolRichPP {
    /// Tries to decode a [`ProtocolRichPP`] from a [`XMLNode`].
    pub fn decode(xml: XMLNode) -> io::Result<Self> {
        let mut raw = String::new();
        let inner1 = xml
            .get_child("_".to_string())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", InvalidRichPP))
            })?
            .clone();
        let inner2 = inner1
            .get_child("pp".to_string())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", InvalidRichPP))
            })?
            .clone();

        for node in inner2.children {
            if let Some(elem) = node.as_node() {
                raw += format!("<{}>{}</{}>", elem.name, elem.get_text(), elem.name).as_str();
            }
            if let Some(txt) = node.raw() {
                raw += txt;
            }
        }

        Ok(Raw(raw))
    }
}

impl FeedbackContent {
    /// Tries to decode a [`FeedbackContent`] from a [`XMLNode`].
    pub fn decode(xml: XMLNode) -> io::Result<Self> {
        assert_decode_error(xml.attributes.get("val").is_some(), || {
            InvalidFeedbackContent
        })?;

        match xml.attributes.get("val").unwrap().as_str() {
            "processed" => Ok(FeedbackContent::Processed),
            "message" => {
                assert_decode_error(!xml.children.is_empty(), || InvalidFeedbackContent)?;

                // <message>
                //    <message_level />
                //    <option />
                //    <richpp>
                //      <_><pp>...</pp></_>
                //    </richpp>
                // </message>
                let message = xml.children[0].as_node().unwrap().clone();
                assert_decode_error(!message.children.is_empty(), || InvalidFeedbackContent)?;
                assert_decode_error(message.children.len() >= 3, || InvalidFeedbackContent)?;

                ProtocolRichPP::decode(message.children[2].as_node().cloned().unwrap())
                    .map(FeedbackContent::Message)
            }
            "workerstatus" => {
                assert_decode_error(!xml.children.is_empty(), || InvalidFeedbackContent)?;

                Ok(FeedbackContent::WorkerStatus(
                    xml.children[0].as_node().cloned().unwrap(),
                ))
            }
            "processingin" => {
                assert_decode_error(!xml.children.is_empty(), || InvalidFeedbackContent)?;

                Ok(FeedbackContent::Processing(
                    xml.children[0].as_node().cloned().unwrap(),
                ))
            }
            _ => unreachable!(),
        }
    }
}

/// Asserts that the condition is true or throw the [`DecodeError`] generated by the function.
fn assert_decode_error<F>(cond: bool, gen: F) -> io::Result<()>
where
    F: FnOnce() -> DecodeError,
{
    if cond {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{:?}", gen()),
        ))
    }
}

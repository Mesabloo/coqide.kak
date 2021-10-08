use super::types::{
    ProtocolResult::{self, *},
    ProtocolRichPP::{self, *},
    ProtocolValue::{self, *},
};

use std::io::{Read, Write};
use std::num::ParseIntError;
use std::str::ParseBoolError;
use xmltree::Element;
use xmltree::ParseError;

pub enum DecodeError {
    InvalidUnit,
    InvalidList,
    InvalidString,
    IntParseError(ParseIntError),
    InvalidInteger,
    InvalidBoolean,
    BoolParseError(ParseBoolError),
    InvalidPair,
    InvalidOption,
    InvalidStatus,
    ElementParseError(ParseError),
    InvalidValue,
    InvalidRichPP,
}

use DecodeError::*;

impl std::fmt::Debug for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidUnit => write!(f, "Invalid <unit/> tag"),
            InvalidList => write!(f, "Invalid <list/> tag"),
            InvalidString => write!(f, "Invalid <string/> tag"),
            IntParseError(e) => write!(f, "Invalid <int/> content: {}", e),
            InvalidInteger => write!(f, "Invalid <int/> tag"),
            InvalidBoolean => write!(f, "Invalid <bool/> tag"),
            BoolParseError(e) => write!(f, "Invalid <bool/> 'val' attribute: {}", e),
            InvalidPair => write!(f, "Invalid <pair/> tag"),
            InvalidOption => write!(f, "Invalid <option/> tag"),
            InvalidStatus => write!(f, "Invalid <status/> tag"),
            ElementParseError(e) => write!(f, "Parse error: {}", e),
            InvalidValue => write!(f, "Invalid <value/> tag"),
            InvalidRichPP => write!(f, "Invalid <richpp/> tag"),
        }
    }
}

impl ProtocolValue {
    /// Try to decode a protocol value form an XML `Element`.
    ///
    /// May throw a `DecodeError` if the value is malformed.
    pub fn decode(xml: Element) -> Result<Self, DecodeError> {
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
                    // remove all non-element children ...
                    .filter_map(|node| node.as_element())
                    // ... and decode all the nodes as protocol values
                    .map(|el| ProtocolValue::decode(el.clone()))
                    .collect::<Result<Vec<_>, _>>()
                    .map(List)
            }
            "string" => {
                assert_decode_error(xml.attributes.is_empty(), || InvalidString)?;

                Ok(Str(xml
                    .get_text()
                    .map(|cow| cow.into_owned())
                    .unwrap_or_else(|| "".to_string())))
            }
            "int" => {
                assert_decode_error(xml.attributes.is_empty(), || InvalidInteger)?;

                xml.get_text()
                    .map(|cow| cow.into_owned().parse::<i64>())
                    .transpose()
                    .map_err(IntParseError)
                    .and_then(|opt| opt.map(Int).ok_or(InvalidInteger))
            }
            "bool" => {
                assert_decode_error(xml.children.is_empty(), || InvalidBoolean)?;
                assert_decode_error(xml.attributes.get("val").is_some(), || InvalidBoolean)?;

                let val = xml.attributes.get("val").unwrap();
                val.parse::<bool>().map_err(BoolParseError).map(Boolean)
            }
            "pair" => {
                assert_decode_error(xml.children.len() == 2, || InvalidPair)?;
                assert_decode_error(xml.attributes.is_empty(), || InvalidPair)?;

                let mut vals = xml
                    .children
                    .iter()
                    .filter_map(|node| node.as_element())
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
                                .as_element()
                                .map(|el| ProtocolValue::decode(el.clone()).map(Box::new))
                                .transpose()?,
                        ))
                    }
                    "none" => {
                        assert_decode_error(xml.children.is_empty(), || InvalidOption)?;

                        Ok(Optional(None))
                    }
                    _ => Err(InvalidOption),
                }
            }
            "status" => {
                assert_decode_error(xml.children.len() == 4, || InvalidStatus)?;

                let mut children = xml
                    .children
                    .iter()
                    .filter_map(|node| node.as_element())
                    .map(|el| ProtocolValue::decode(el.clone()))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(Status(
                    Box::new(children.remove(0)),
                    Box::new(children.remove(0)),
                    Box::new(children.remove(0)),
                    Box::new(children.remove(0)),
                ))
            }
            _ => Ok(Unknown(xml)),
        }
    }

    pub fn decode_stream<R>(stream: R) -> Result<Self, DecodeError>
    where
        R: Read,
    {
        let elem = Element::parse(stream).map_err(ElementParseError)?;
        ProtocolValue::decode(elem.clone())
    }
}

impl ProtocolResult {
    pub fn decode(xml: Element) -> Result<Self, DecodeError> {
        match xml.name.as_str() {
            "value" => {
                assert_decode_error(xml.attributes.len() >= 1, || InvalidValue)?;
                assert_decode_error(xml.attributes.get("val").is_some(), || InvalidValue)?;

                let val = xml.attributes.get("val").unwrap();
                match val.as_str() {
                    "good" => Ok(Good(
                        xml.get_text()
                            .map_or_else(|| String::new(), |cow| cow.into_owned()),
                    )),
                    "fail" => {
                        let loc_s = xml
                            .attributes
                            .get("loc_s")
                            .map(|str| str.parse::<i64>().map_err(IntParseError))
                            .transpose()?;
                        let loc_e = xml
                            .attributes
                            .get("loc_e")
                            .map(|str| str.parse::<i64>().map_err(IntParseError))
                            .transpose()?;

                        let richpp_elem =
                            xml.get_child("richpp").ok_or_else(|| InvalidValue)?.clone();
                        let richpp = ProtocolRichPP::decode(richpp_elem)?;

                        Ok(Fail(loc_s, loc_e, richpp))
                    }
                    _ => Err(InvalidValue),
                }
            }
            _ => Err(InvalidValue),
        }
    }

    pub fn decode_stream<R>(stream: R) -> Result<Self, DecodeError>
    where
        R: Read,
    {
        let elem = Element::parse(stream).map_err(ElementParseError)?;
        ProtocolResult::decode(elem.clone())
    }
}

impl ProtocolRichPP {
    pub fn decode(xml: Element) -> Result<Self, DecodeError> {
        let mut raw = String::new();
        let inner1 = xml.get_child("_").ok_or_else(|| InvalidRichPP)?.clone();
        let inner2 = inner1.get_child("pp").ok_or_else(|| InvalidRichPP)?.clone();

        for node in inner2.children {
            if let Some(elem) = node.as_element() {
                raw = format!(
                    "{}<{}>{}</{}>",
                    raw,
                    elem.name,
                    elem.get_text()
                        .map(|cow| cow.into_owned())
                        .unwrap_or_else(|| "".to_string()),
                    elem.name
                );
            }
            if let Some(txt) = node.as_text() {
                raw += txt;
            }
        }

        Ok(Raw(raw))
    }
}

fn assert_decode_error<F>(cond: bool, gen: F) -> Result<(), DecodeError>
where
    F: FnOnce() -> DecodeError,
{
    if cond {
        Ok(())
    } else {
        Err(gen())
    }
}

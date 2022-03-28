use bytes::{Buf, BytesMut};
use combine::{
    attempt, choice, easy, from_str, many, many1, parser,
    parser::{
        byte::{alpha_num, byte, space, take_until_byte},
        combinator::{any_partial_state, AnyPartialState},
        repeat::{repeat_until, skip_many},
    },
    stream::PartialStream,
    ParseError, RangeStream,
};
use std::{collections::HashMap, io};
use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};

/// A generic representation of the XML node `<name attributes...>children...</name>`.
#[derive(Clone, Debug)]
pub struct XMLNode {
    /// The name of the node.
    pub name: String,
    /// All attributes present in the tag.
    pub attributes: HashMap<String, String>,
    /// A list of all children inside the node.
    pub children: Vec<Child>,
}

impl Default for XMLNode {
    /// Creates a new XML node with no name, no attributes and no children.
    fn default() -> Self {
        XMLNode {
            name: String::new(),
            attributes: HashMap::new(),
            children: Vec::new(),
        }
    }
}

/// The child of a XML node.
///
/// It is either:
/// - another [`XMLNode`]
/// - some raw text
#[derive(Clone, Debug)]
pub enum Child {
    Node(XMLNode),
    Raw(String),
}

pub struct XMLDecoder {
    state: AnyPartialState,
}

unsafe impl Send for XMLDecoder {}

impl Default for XMLDecoder {
    fn default() -> Self {
        XMLDecoder {
            state: Default::default(),
        }
    }
}

impl Decoder for XMLDecoder {
    type Item = XMLNode;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let (opt, removed) = combine::stream::decode(
            parse_node(),
            &mut easy::Stream(PartialStream(&src[..])),
            &mut self.state,
        )
        .map_err(|err| {
            let err = err
                .map_range(|r| {
                    std::str::from_utf8(r)
                        .ok()
                        .map_or_else(|| format!("{:?}", r), |s| s.to_string())
                })
                .map_position(|p| p.translate_position(&src[..]));
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}\nIn input: `{}`", err, std::str::from_utf8(src).unwrap()),
            )
        })?;

        log::debug!(
            "Accepted {} bytes from stream: {:?}",
            removed,
            std::str::from_utf8(&src[..removed]).unwrap_or("NOT UTF-8")
        );

        src.advance(removed);

        match opt {
            None => {
                log::warn!("More input needed to parse response!");
                Ok(None)
            }
            o => Ok(o),
        }
    }
}

pub fn xml_decoder<R>(stream: R) -> FramedRead<R, XMLDecoder>
where
    R: AsyncRead + Unpin,
{
    FramedRead::new(stream, XMLDecoder::default())
}

impl XMLNode {
    /// Decodes a stream chunks by chunks until a complete XML node can be decoded.
    pub async fn decode_stream<R>(reader: &mut FramedRead<R, XMLDecoder>) -> io::Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        tokio::select! {
            Some(xml) = reader.next() => xml,
            else => Err(io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe")),
        }
    }

    /// Retrieves all the raw text inside a node.
    pub fn get_text(&self) -> String {
        self.children
            .iter()
            .filter_map(|el| el.raw().cloned())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Tries to retrieve the first child whose tag name is `name`.
    pub fn get_child(&self, name: String) -> Option<&XMLNode> {
        for child in &self.children {
            if let Some(node) = child.as_node() {
                if node.name == name {
                    return Some(node);
                }
            }
        }
        None
    }
}

impl Child {
    /// Tries to convert the current child into a [`XMLNode`] if it was one.
    pub fn as_node(&self) -> Option<&XMLNode> {
        match self {
            Child::Node(n) => Some(n),
            _ => None,
        }
    }

    /// Retrieves the raw text the child is.
    pub fn raw(&self) -> Option<&String> {
        match self {
            Child::Raw(str) => Some(str),
            _ => None,
        }
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_node['a, Input]()(Input) -> XMLNode
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            byte(b'<'),
            parse_identifier(),
            skip_many(space()),
            many::<Vec<_>, _, _>(parse_attribute()),
            skip_many(space()),
            choice((
                parse_slash_end().map(|cs| (cs, String::new())),
                parse_normal_end(),
            )),
        ))
        .map(|(_, n1, _, attrs, _, (children, _))|
            // hoping that the node is properly closed with a closing tag with the same name
            XMLNode {
                name: n1,
                attributes: HashMap::from_iter(attrs.into_iter()),
                children
            }
        )
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_slash_end['a, Input]()(Input) -> Vec<Child>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            byte(b'/').expected("slash"),
            byte(b'>').expected("right angle"),
        )).map(|_| Vec::new())
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_tag_end['a, Input]()(Input) -> String
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            byte(b'<').expected("left angle"),
            byte(b'/').expected("slash"),
            parse_identifier(),
            byte(b'>').expected("right angle"),
        )).map(|(_, _, n, _)| n)
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_normal_end['a, Input]()(Input) -> (Vec<Child>, String)
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            byte(b'>').expected("right angle"),
            repeat_until::<Vec<_>, _, _, _>(
                choice((
                    attempt(parse_node().map(Child::Node)),
                    from_str(take_until_byte(b'<')).map(|str| Child::Raw(unescape(str))),
                )),
                attempt(parse_tag_end()),
            ),
            parse_tag_end(),
        ))
        .map(|(_, cs, n)| {
            (
              // small trick: sometimes, the parser yields a `Child::Raw("")` which should be ignored.
              cs.into_iter()
                  .filter(|n | match n {
                      Child::Raw(str) => !str.is_empty(),
                      _ => true,
                  })
                  .collect(),
              n
            )
        })
    }
}

/// Unescape special character sequences like `&nbsp;` or `&amp;` back to ASCII characters.
fn unescape(str: String) -> String {
    str.replace("&nbsp;", " ")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&#40;", "(")
        .replace("&#41;", ")")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_attribute['a, Input]()(Input) -> (String, String)
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            parse_identifier(),
            skip_many(space()),
            byte(b'=').expected("equals"),
            skip_many(space()),
            parse_string(),
            skip_many(space()),
        ))
        .map(|(name, _, _, _, value, _)| (name, value))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_string['a, Input]()(Input) -> String
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            byte(b'"').expected("double quote"),
            from_str(take_until_byte(b'"')),
            byte(b'"').expected("double quote")
        )).map(|(_, val, _)| val)
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_identifier['a, Input]()(Input) -> String
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(from_str(many1::<Vec<_>, _, _>(choice((
            alpha_num(),
            byte(b'_').expected("underscore"),
            byte(b'-').expected("dash"),
            byte(b'.').expected("dot"),
        )))))
    }
}

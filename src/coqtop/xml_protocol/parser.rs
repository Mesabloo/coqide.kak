use bytes::{Buf, BytesMut};
use combine::{
    attempt, choice, easy, from_str, many, many1, parser,
    parser::{
        byte::{self, alpha_num, byte, space, take_until_byte},
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

#[derive(Clone, Debug)]
pub struct XMLNode {
    pub name: String,
    pub attributes: HashMap<String, String>,
    pub children: Vec<Child>,
}

impl Default for XMLNode {
    fn default() -> Self {
        XMLNode {
            name: String::new(),
            attributes: HashMap::new(),
            children: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Child {
    Node(XMLNode),
    Raw(String),
}

struct XMLDecoder {
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

impl XMLNode {
    pub async fn decode_stream<R>(stream: R) -> io::Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        let decoder = XMLDecoder::default();

        FramedRead::new(stream, decoder)
            .try_next()
            .await
            .and_then(|opt| {
                opt.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::BrokenPipe, "Cannot fetch next XML node")
                })
            })
    }

    pub fn get_text(&self) -> String {
        self.children
            .iter()
            .filter_map(|el| el.raw().cloned())
            .collect::<Vec<_>>()
            .join("")
    }

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
    pub fn as_node(&self) -> Option<&XMLNode> {
        match self {
            Child::Node(n) => Some(n),
            _ => None,
        }
    }

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
                parse_normal_end(),
                parse_slash_end().map(|cs| (cs, String::new())),
            )),
        ))
        .flat_map(|(_, n1, _, attrs, _, (children, n2))|
          if n2.is_empty() || n1 == n2 {
              Ok(XMLNode {
                  name: n1,
                  attributes: HashMap::from_iter(attrs.into_iter()),
                  children
              })
          } else {
              todo!()
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
        any_partial_state(byte::bytes(&b"/>"[..]).map(|_| Vec::new()))
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
            byte::bytes(&b"</"[..]),
            parse_identifier(),
            byte(b'>'),
        )).map(|(_, n, _)| n)
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
            byte(b'>'),
            repeat_until::<Vec<_>, _, _, _>(
                choice((
                    attempt(parse_node().map(Child::Node)),
                    from_str(take_until_byte(b'<')).map(Child::Raw),
                )),
                attempt(parse_tag_end()),
            ),
            parse_tag_end(),
        ))
        .map(|(_, cs, n)| (cs, n))
    }
}

parser! {
    fn parse_attribute['a, Input]()(Input) -> (String, String)
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            parse_identifier(),
            skip_many(space()),
            byte(b'='),
            skip_many(space()),
            parse_string(),
            skip_many(space()),
        ))
        .map(|(name, _, _, _, value, _)| (name, value))
    }
}

parser! {
    fn parse_string['a, Input]()(Input) -> String
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((byte(b'"'), from_str(take_until_byte(b'"')), byte(b'"'))).map(|(_, val, _)| val)
    }
}

parser! {
    fn parse_identifier['a, Input]()(Input) -> String
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(from_str(many1::<Vec<_>, _, _>(choice((
            alpha_num(),
            byte(b'_'),
            byte(b'-'),
        )))))
    }
}

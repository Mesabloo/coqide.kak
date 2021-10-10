use crate::{
    coqtop::slave::IdeSlave,
    xml_protocol::types::{ProtocolCall, ProtocolValue},
};
use bytes::Buf;
use combine::{
    choice, easy,
    error::ParseError,
    parser,
    parser::{
        combinator::{any_partial_state, AnyPartialState},
        range::range,
        token,
    },
    stream::PartialStream,
    RangeStream,
};
use futures::TryStreamExt;
use std::io;
use tokio::fs::File;
use tokio_util::codec::{Decoder, FramedRead};

pub struct Command<'a> {
    pub session: &'a String,
    pub slave: &'a mut IdeSlave,
    pub kind: CommandKind,
}

#[derive(Debug)]
pub enum CommandKind {
    Init,
    Nop,
}

impl<'a> Command<'a> {
    pub async fn execute(self) -> io::Result<()> {
        match self.kind {
            CommandKind::Init => {
                self.slave
                    .send_call(ProtocolCall::Init(ProtocolValue::Optional(None)))
                    .await
            }
            CommandKind::Nop => Ok(()),
        }
    }
}

use CommandKind::*;

#[derive(Default)]
struct CommandDecoder {
    state: AnyPartialState,
}

impl Decoder for CommandDecoder {
    type Item = CommandKind;
    type Error = io::Error;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let (opt, removed) = combine::stream::decode(
            parse_command(),
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

impl CommandKind {
    pub async fn parse_from(input: &mut File) -> io::Result<Option<Self>> {
        let decoder = CommandDecoder::default();

        FramedRead::new(input, decoder).try_next().await
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_command['a, Input]()(Input) -> CommandKind
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(choice((parse_init(), ignore_byte())))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_init['a, Input]()(Input) -> CommandKind
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(range(&b"init\n"[..]).map(|_| CommandKind::Init))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn ignore_byte['a, Input]()(Input) -> CommandKind
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(token::any()).map(|_| Nop)
    }
}

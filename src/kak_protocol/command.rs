use crate::{
    coqtop::slave::IdeSlave,
    xml_protocol::types::{ProtocolCall, ProtocolValue},
};
use combine::{
    attempt, choice, decode_tokio,
    error::ParseError,
    parser,
    parser::{
        self,
        combinator::{any_partial_state, AnyPartialState},
        range::range,
        token,
    },
    stream::Decoder,
    Parser, RangeStream,
};
use std::{io, ops::DerefMut, pin::Pin};
use tokio::fs::File;

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

impl CommandKind {
    pub async fn parse_from(mut input: &mut File) -> io::Result<Self> {
        let mut decoder = Decoder::new();

        decode_tokio!(decoder, input, parse_command(), |input, _position| {
            combine::easy::Stream::from(input)
        })
        .map_err(combine::easy::Errors::<u8, &[u8], _>::from)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{:?}", err)))
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

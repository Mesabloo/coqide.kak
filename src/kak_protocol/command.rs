use crate::{
    coqtop::slave::IdeSlave,
    xml_protocol::types::{ProtocolCall, ProtocolValue},
};
use combine::{
    attempt, choice, decode_tokio,
    error::ParseError,
    parser::{
        combinator::{any_partial_state, AnyPartialState},
        range::range,
    },
    stream::Decoder,
    Parser, RangeStream,
};
use tokio::fs::File;
use std::{io, ops::DerefMut, pin::Pin};

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
        // 'main: loop {
        //     parse_command()
        // }

        // 'main: loop {
        //     match parse_command(&buf) {
        //         IResult::Ok((_, cmd)) => break Ok(cmd),
        //         Err(nom::Err::Incomplete(nom::Needed::Size(n))) => {
        //             let mut n = n.get();

        //             'fetch_loop: loop {
        //                 let m = buf.len();
        //                 buf.resize(m + n, 0);

        //                 let o = stream.read(&mut buf[m..m + n])?;
        //                 match o {
        //                     0 => {
        //                         // No bytes could be fetched, stop
        //                         log::warn!("Received EOF while parsing a command. Ignoring");
        //                         break 'main Ok(Nop);
        //                     }
        //                     _ if o != n => {
        //                         n = n - o;
        //                     }
        //                     _ => break 'fetch_loop, // enough data was fetched, go on
        //                 }
        //             }
        //         }
        //         Err(err) => {
        //             log::error!("Error while parsing command: {:?}\nSkipping some bytes until parsing succeeds...", err);
        //             buf.remove(0);
        //         }
        //     }
        // }
    }
}

fn parse_command<'a, I>(
) -> impl Parser<I, Output = CommandKind, PartialState = AnyPartialState> + 'a
where
    I: RangeStream<Token = u8, Range = &'a [u8]> + 'a,
    I::Error: ParseError<I::Token, I::Range, I::Position>,
{
    any_partial_state(choice((attempt(parse_init()),)))
}

fn parse_init<'a, I>() -> impl Parser<I, Output = CommandKind, PartialState = AnyPartialState> + 'a
where
    I: RangeStream<Token = u8, Range = &'a [u8]> + 'a,
    I::Error: ParseError<I::Token, I::Range, I::Position>,
{
    any_partial_state(range(&b"init\n"[..]).map(|_| CommandKind::Init))
}

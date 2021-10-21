use std::io;

use bytes::Buf;

use combine::{
    choice, easy, from_str, many, parser,
    parser::{
        byte::{self, digit, newline, space},
        combinator::{any_partial_state, AnyPartialState},
        range::range,
        repeat::repeat_until,
        token,
    },
    skip_many1,
    stream::PartialStream,
    ParseError, RangeStream,
};

use tokio::{fs::File, io::AsyncRead};
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};

use super::types::Command;

#[derive(Default)]
struct CommandDecoder {
    state: AnyPartialState,
}

unsafe impl Send for CommandDecoder {}

impl Decoder for CommandDecoder {
    type Item = Option<Command>;
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

impl Command {
    /// Tries to parse a [`Command`] from the given file.
    ///
    /// **Note:** the double [`Option`] may seem weird at first, but here is why:
    /// - The first [`Option`] defines whether the stream was closed or not (which *should* never happen in such case, but nobody knows).
    ///   If the stream is closed, then this will return [`None`].
    /// - The second [`Option`] defines whether a command was successfully decoded, or if a simple byte as been ignored.
    ///   If a command could not be decoded, this will return [`None`].
    pub async fn parse_from<R>(input: R) -> io::Result<Option<Option<Self>>>
    where
        R: AsyncRead + Unpin,
    {
        let decoder = CommandDecoder::default();

        FramedRead::new(input, decoder).try_next().await
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_command['a, Input]()(Input) -> Option<Command>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(choice((parse_init(), parse_previous(), parse_rewind(), parse_query(), ignore_byte())))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_init['a, Input]()(Input) -> Option<Command>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(range(&b"init\n"[..])).map(|_| Some(Command::Init))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_previous['a, Input]()(Input) -> Option<Command>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(range(&b"previous\n"[..])).map(|_| Some(Command::Previous))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_rewind['a, Input]()(Input) -> Option<Command>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            range(&b"rewind-to"[..]).map(|_| ()),
            skip_many1(space()),
            from_str::<_, String, _>(many::<Vec<_>, _, _>(digit())),
            skip_many1(space()),
            from_str::<_, String, _>(many::<Vec<_>, _, _>(digit())),
            newline(),
        )).map(|(_, _, line, _, col, _)| {
            let line = line.parse::<u64>().unwrap();
            let col = col.parse::<u64>().unwrap();
            Some(Command::RewindTo(line, col))
        })
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_query['a, Input]()(Input) -> Option<Command>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            range(&b"query"[..]).map(|_| ()),
            skip_many1(space()),
            parse_string(),
            newline(),
        )).map(|(_, _, str, _)| Some(Command::Query(str)))
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
        let escaped = || range(&b"\\\""[..]).map(|_| b'"');

        any_partial_state((
            byte::byte(b'"'),
            from_str(repeat_until::<Vec<_>, _, _, _>(
                choice((escaped(), token::any())),
                byte::byte(b'"'),
            )),
            byte::byte(b'"'),
        )).map(|(_, str, _)| str)
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn ignore_byte['a, Input]()(Input) -> Option<Command>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(token::any()).map(|_| None)
    }
}

use std::io;

use bytes::Buf;

use combine::{ParseError, RangeStream, attempt, choice, easy, from_str, many, many1, parser, parser::{
        byte::{self, digit, newline},
        combinator::{any_partial_state, AnyPartialState},
        range::range,
        repeat::repeat_until,
        token,
    }, sep_by1, skip_many, skip_many1, stream::PartialStream};

use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};

use crate::state::CodeSpan;

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
        any_partial_state(choice((
            attempt(parse_init()),
            attempt(parse_previous()),
            attempt(parse_rewind_to()),
            attempt(parse_query()),
            attempt(parse_move_to()),
            attempt(parse_next()),
            attempt(parse_quit()),
            ignore_byte(),
        )))
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

    fn parse_rewind_to['a, Input]()(Input) -> Option<Command>
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
            skip_many(space()),
            newline(),
        )).map(|(_, _, line, _, col, _, _)| {
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
            skip_many(space()),
            newline(),
        )).map(|(_, _, str, _, _)| Some(Command::Query(str)))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_move_to['a, Input]()(Input) -> Option<Command>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            range(&b"move-to"[..]).map(|_| { log::debug!("Parsed 'move-to'"); () }),
            skip_many1(space()),
            sep_by1(parse_coq_statement(), skip_many1(space())),
            skip_many(space()),
            newline(),
        )).map(|(_, _, codes, _, _)| Some(Command::MoveTo(codes)))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_next['a, Input]()(Input) -> Option<Command>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            range(&b"next"[..]).map(|_| ()),
            skip_many1(space()),
            parse_coq_statement(),
            skip_many(space()),
            newline(),
        )).map(|(_, _, (span, stmt), _, _)| Some(Command::Next(span, stmt)))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_quit['a, Input]()(Input) -> Option<Command>
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(range(&b"quit\n"[..])).map(|_| Some(Command::Quit))
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
                choice((attempt(escaped()), token::any())),
                attempt(byte::byte(b'"')),
            )),
            byte::byte(b'"'),
        )).map(|(_, str, _)| str)
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_coq_statement['a, Input]()(Input) -> (CodeSpan, String)
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            parse_line_span(),
            byte::byte(b','),
            parse_string(),
        )).map(|(span, _, code)| (span, code))
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_int['a, Input]()(Input) -> u64
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state(from_str(many1::<Vec<_>, _, _>(digit()))).map(|int: String| int.parse::<u64>().unwrap())
    }
}

parser! {
    type PartialState = AnyPartialState;

    fn parse_line_span['a, Input]()(Input) -> CodeSpan
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        any_partial_state((
            parse_int(),
            byte::byte(b'.'),
            parse_int(),
            byte::byte(b','),
            parse_int(),
            byte::byte(b'.'),
            parse_int(),
        )).map(|(begin_line, _, begin_column, _, end_line, _, end_column)| {
            CodeSpan::new(begin_line, begin_column, end_line, end_column)
        })
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

parser! {
    fn space['a, Input]()(Input) -> ()
    where [
        Input: RangeStream<Token = u8, Range = &'a [u8]>,
        Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
    ]
    {
        choice((byte::byte(b' '), byte::byte(b'\t'))).map(|_| ())
    }
}

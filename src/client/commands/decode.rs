use std::io;

use bytes::Buf;

use nom::{
    branch::alt,
    bytes::streaming::{is_a, tag, take, take_while, take_while1},
    combinator::{cut, map, value, verify},
    multi::{many0, separated_list1},
    sequence::{delimited, pair, preceded, tuple},
    IResult,
};
use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};

use crate::range::Range;

use super::types::ClientCommand;

#[derive(Default)]
pub struct CommandDecoder {}

unsafe impl Send for CommandDecoder {}

impl Decoder for CommandDecoder {
    type Item = Option<ClientCommand>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let result = parse_command(&src[..]);
        match result {
            Ok((remaining, parsed)) => {
                let count = src.len() - remaining.len();
                log::debug!(
                    "Accepted {} bytes from stream: {:?}",
                    count,
                    std::str::from_utf8(&src[..count]).unwrap()
                );

                src.advance(count);

                Ok(Some(parsed))
            }
            Err(nom::Err::Incomplete(_)) => {
                log::warn!("More data needed to parse input");

                Ok(None)
            }
            Err(err) => Err(io::Error::new(io::ErrorKind::InvalidData, err.to_string())),
        }
    }
}

impl ClientCommand {
    /// Decodes a stream chunks by chunks until a complete XML node can be decoded.
    pub async fn decode_stream<R>(
        reader: &mut FramedRead<R, CommandDecoder>,
    ) -> io::Result<Option<Self>>
    where
        R: AsyncRead + Unpin,
    {
        tokio::select! {
            Some(cmd) = reader.next() => cmd,
            else => Err(io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe")),
        }
    }
}

pub fn command_decoder<R>(stream: R) -> FramedRead<R, CommandDecoder>
where
    R: AsyncRead + Unpin,
{
    FramedRead::new(stream, CommandDecoder::default())
}

type Input<'a> = &'a [u8];
type Output = Option<ClientCommand>;

fn parse_command<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    alt((
        parse_init,
        parse_query,
        parse_quit,
        parse_previous,
        parse_hints,
        parse_ignore_error,
        parse_next,
        parse_rewind_to,
        parse_move_to,
        parse_show_goals,
        parse_status,
        //map(take(1usize), |_| None),
    ))(input)
}

fn parse_init<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("init"), space0),
        cut(value(Some(ClientCommand::Init), tag("\n"))),
    )(input)
}

fn parse_query<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("query"), space1),
        cut(map(tuple((parse_string, tag("\n"))), |(query, _)| {
            Some(ClientCommand::Query(query))
        })),
    )(input)
}

fn parse_quit<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("quit"), space0),
        cut(value(Some(ClientCommand::Quit), tag("\n"))),
    )(input)
}

fn parse_previous<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("previous"), space0),
        cut(value(Some(ClientCommand::Previous), tag("\n"))),
    )(input)
}

fn parse_hints<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("hints"), space0),
        cut(value(Some(ClientCommand::Hints), tag("\n"))),
    )(input)
}

fn parse_ignore_error<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("ignore-error"), space0),
        cut(value(Some(ClientCommand::IgnoreError), tag("\n"))),
    )(input)
}

fn parse_next<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("next"), space1),
        cut(map(
            tuple((parse_coq_statement, space0, tag("\n"))),
            |((range, code), _, _)| Some(ClientCommand::Next(range, code)),
        )),
    )(input)
}

fn parse_rewind_to<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("rewind-to"), space1),
        cut(map(
            tuple((u64, space1, u64, space0, tag("\n"))),
            |(line, _, column, _, _)| Some(ClientCommand::RewindTo(line, column)),
        )),
    )(input)
}

fn parse_move_to<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("move-to"), space1),
        cut(map(
            tuple((
                separated_list1(space1, parse_coq_statement),
                space0,
                tag("\n"),
            )),
            |(ranges, _, _)| Some(ClientCommand::MoveTo(ranges)),
        )),
    )(input)
}

fn parse_show_goals<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("show-goals"), space0),
        cut(map(
            tuple((parse_range, space0, tag("\n"))),
            |(range, _, _)| Some(ClientCommand::ShowGoals(range)),
        )),
    )(input)
}

fn parse_status<'a>(input: Input<'a>) -> IResult<Input<'a>, Output> {
    preceded(
        pair(tag("status"), space0),
        cut(value(Some(ClientCommand::Status), tag("\n"))),
    )(input)
}

// ---------------------------

fn parse_range<'a>(input: Input<'a>) -> IResult<Input<'a>, Range> {
    map(
        tuple((u64, tag("."), u64, tag(","), u64, tag("."), u64)),
        |(begin_line, _, begin_column, _, end_line, _, end_column)| {
            Range::new(begin_line, begin_column, end_line, end_column)
        },
    )(input)
}

fn parse_coq_statement<'a>(input: Input<'a>) -> IResult<Input<'a>, (Range, String)> {
    map(
        tuple((parse_range, space0, parse_string)),
        |(range, _, code)| (range, code),
    )(input)
}

fn parse_string<'a>(input: Input<'a>) -> IResult<Input<'a>, String> {
    let escaped = |input| map(tag("\\\""), |_| b'"')(input);

    delimited(
        tag("\""),
        map(many0(alt((escaped, any_single))), |res| {
            std::str::from_utf8(&res[..]).unwrap().to_string()
        }),
        tag("\""),
    )(input)
}

fn any_single<'a>(input: Input<'a>) -> IResult<Input<'a>, u8> {
    map(
        verify(take(1usize), |s: &[u8]| s[0] != b'"'),
        |s: &[u8]| s[0],
    )(input)
}

fn space1<'a>(input: Input<'a>) -> IResult<Input<'a>, ()> {
    let is_space = |c: u8| c == b' ' || c == b'\t';

    value((), take_while1(is_space))(input)
}

fn space0<'a>(input: Input<'a>) -> IResult<Input<'a>, ()> {
    let is_space = |c: u8| c == b' ' || c == b'\t';

    value((), take_while(is_space))(input)
}

fn u64<'a>(input: Input<'a>) -> IResult<Input<'a>, u64> {
    map(is_a("0123456789"), |slice| {
        std::str::from_utf8(slice).unwrap().parse::<u64>().unwrap()
    })(input)
}

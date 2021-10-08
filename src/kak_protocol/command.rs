use crate::{
    coqtop::slave::IdeSlave,
    xml_protocol::types::{ProtocolCall, ProtocolValue},
};
use futures::{
    io::AsyncRead, io::BufReader, AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncSeekExt,};
use nom::{
    bytes::streaming::tag,
    character::streaming::{newline, space0},
    IResult,
};
use std::{fs::File, io::{self, Read, SeekFrom}, ops::DerefMut, pin::Pin};

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
                    .send_message(ProtocolCall::Init(ProtocolValue::Optional(None)))
                    .await
            }
            CommandKind::Nop => Ok(()),
        }
    }
}

use CommandKind::*;

impl CommandKind {
    pub async fn parse_from(mut input: Pin<&mut File>) -> io::Result<Self> {
        let mut buf = vec![];
        let stream = input.deref_mut();

        'main: loop {
            log::debug!("Read {} bytes for parser", buf.len());
            match parse_command(&buf) {
                IResult::Ok((_, cmd)) => break Ok(cmd),
                Err(nom::Err::Incomplete(nom::Needed::Size(n))) => {
                    let mut n = n.get();

                    log::debug!("Parsing needs {} more bytes", n);

                    'fetch_loop: loop {
                        let m = buf.len();
                        buf.resize(m + n, 0);

                        let o = stream.read(&mut buf[m..m + n])?;
                        match o {
                            0 => {
                                // No bytes could be fetched, stop
                                log::warn!("Received EOF while parsing a command. Ignoring");
                                break 'main Ok(Nop);
                            }
                            _ if o != n => {
                                n = n - o;
                                log::warn!("Missing {} bytes...", n);
                            }
                            _ => break 'fetch_loop, // enough data was fetched, go on
                        }
                    }
                }
                Err(err) => {
                    log::error!("Error while parsing command: {:?}\nSkipping some bytes until parsing succeeds...", err);
                    buf.remove(0);
                }
            }
        }
    }
}

fn parse_init<'a>(s: &'a [u8]) -> IResult<&'a [u8], CommandKind> {
    let (s, _) = tag("init")(s)?;
    let (s, _) = space0(s)?;
    let (s, _) = newline(s)?;
    Ok((s, Init))
}

fn parse_command<'a>(s: &'a [u8]) -> IResult<&'a [u8], CommandKind> {
    let (s, kind) = parse_init(s)?;
    Ok((s, kind))
}

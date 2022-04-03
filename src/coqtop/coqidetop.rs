use std::{collections::VecDeque, io, process::Stdio, sync::Arc};

use tokio::{
    io::AsyncWriteExt,
    process::{Child, ChildStdin, ChildStdout, Command},
};
use tokio_util::codec::FramedRead;

use crate::{
    coqtop::xml_protocol::{parser::xml_decoder, types::ProtocolResult},
    session::{edited_file, temporary_folder, Session},
};

use super::xml_protocol::{parser::XMLDecoder, types::ProtocolCall};

/// The name of the `coqtop` process.
pub const COQTOP: &'static str = "coqidetop";

pub struct CoqIdeTop {
    /// The main channel where [`COQTOP`] sends its responses.
    //main_r: TcpStream,
    /// The main channel to send commands (calls, see [`ProtocolCall`]) to [`COQTOP`].
    ///
    /// [`ProtocolCall`]: crate::coqtop::xml_protocol::types::ProtocolCall
    main_w: ChildStdin,
    /// The underlying process.
    _process: Child,
    /// The framed reader which decodes all input coming from [`COQTOP`]'s stdout.
    reader: FramedRead<ChildStdout, XMLDecoder>,
}

impl CoqIdeTop {
    /// Creates a new [`COQTOP`] wrapper which allows asynchronously processing messages coming
    /// from an unbounded channel.
    pub async fn spawn(session: Arc<Session>) -> io::Result<Self> {
        let flags = ["-topfile", &edited_file(session.clone())];
        // TODO: add flags found in a `_CoqProject` file

        let mut coqidetop = coqidetop(&temporary_folder(session.clone()), [0, 0], flags).await?;

        log::info!(
            "{} (process {}) is up and running!",
            COQTOP,
            coqidetop.id().unwrap_or(0)
        );

        let reader = xml_decoder(coqidetop.stdout.take().unwrap());

        Ok(Self {
            main_w: coqidetop.stdin.take().unwrap(),
            _process: coqidetop,
            reader,
        })
    }

    /// Send a [`ProtocolCall`] to [`COQTOP`] and wait until a response is received,
    /// potentially accumulating some feedback along the way.
    pub async fn ask(
        &mut self,
        call: ProtocolCall,
    ) -> io::Result<(ProtocolResult, VecDeque<ProtocolResult>)> {
        let encoded = call.encode();
        log::debug!(
            "Sending XML-encoded command `{}` to {} process",
            encoded,
            COQTOP
        );

        self.main_w.write_all(encoded.as_bytes()).await?;

        let mut feedback = VecDeque::new();
        loop {
            let response = ProtocolResult::decode_stream(&mut self.reader).await?;

            if response.is_feedback() {
                feedback.push_back(response);
            } else {
                break Ok((response, feedback));
            }
        }
    }

    /// Stops the underlying [`COQTOP`] process dirtily.
    pub async fn quit(mut self) -> io::Result<()> {
        self._process.kill().await?;

        Ok(())
    }
}

/// Spawns a new [`COQTOP`] process by feeding it additional flags to take in account.
async fn coqidetop<const N: usize>(
    _tmp_dir: &String,
    _ports: [u16; 2],
    flags: [&str; N],
) -> io::Result<Child> {
    Command::new(COQTOP)
        .arg("-main-channel")
        // .arg(format!("127.0.0.1:{}:{}", ports[0], ports[1]))
        .arg("stdfds")
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}

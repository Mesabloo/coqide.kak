use std::{io, process::Stdio, sync::Arc};

use tokio::{
    io::AsyncWriteExt,
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{mpsc, watch},
};
use tokio_util::codec::FramedRead;

use crate::{
    coqtop::xml_protocol::{parser::xml_decoder, types::ProtocolResult},
    session::{edited_file, temporary_folder, Session},
};

use super::xml_protocol::{parser::XMLDecoder, types::ProtocolCall};

/// The name of the `coqtop` process.
pub const COQTOP: &str = "coqidetop";

pub struct CoqIdeTop {
    /// The channel to write messages to.
    main_w: ChildStdin,
    /// The underlying process.
    process: Child,
    /// The framed reader which decodes all input coming from [`COQTOP`]'s stdout.
    reader: FramedRead<ChildStdout, XMLDecoder>,
    /// The receiving end of the channel used to transmit commands to [`COQTOP`].
    coqtop_call_rx: mpsc::UnboundedReceiver<ProtocolCall>,
    /// The sending end of the channel used to transmit responses from [`COQTOP`].
    coqtop_response_tx: mpsc::UnboundedSender<ProtocolResult>,
}

impl CoqIdeTop {
    /// Creates a new [`COQTOP`] wrapper which allows asynchronously processing messages coming
    /// from an unbounded channel.
    pub async fn spawn(
        session: Arc<Session>,
        coqtop_call_rx: mpsc::UnboundedReceiver<ProtocolCall>,
        coqtop_response_tx: mpsc::UnboundedSender<ProtocolResult>,
    ) -> io::Result<Self> {
        let flags = [
            "-async-proofs",
            "on",
            "-topfile",
            &edited_file(session.clone()),
        ];
        // TODO: add flags found in a `.CoqProject` file

        let mut coqidetop = coqidetop(&temporary_folder(session.clone()), flags).await?;
        let main_w = coqidetop.stdin.take().unwrap();
        let main_r = coqidetop.stdout.take().unwrap();

        log::info!(
            "{} (process {}) is up and running!",
            COQTOP,
            coqidetop.id().unwrap_or(0)
        );

        let reader = xml_decoder(main_r);

        Ok(Self {
            main_w,
            process: coqidetop,
            reader,
            coqtop_call_rx,
            coqtop_response_tx,
        })
    }

    /// Implements a simple bridge, sending commands received from the channel `coqtop_call` to [`COQTOP`],
    /// and sending responses from [`COQTOP`] into the channel `coqtop_response`.
    pub async fn transmit_until(&mut self, mut stop: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop.changed() => break Ok(()),
                Ok(resp) = ProtocolResult::decode_stream(&mut self.reader) => {
                    self.coqtop_response_tx
                        .send(resp)
                        .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                }
                Some(call) = self.coqtop_call_rx.recv() => {
                    let encoded = call.encode();

                    log::debug!("Sending XML-encoded command `{}` to {} process", encoded, COQTOP);

                    self.main_w.write_all(encoded.as_bytes()).await?;
                }
            }
        }
    }

    /// Stops the underlying [`COQTOP`] process dirtily.
    pub async fn quit(mut self) -> io::Result<()> {
        self.main_w.shutdown().await?;
        self.process.kill().await?;

        Ok(())
    }
}

/// Spawns a new [`COQTOP`] process by feeding it additional flags to take in account.
async fn coqidetop<const N: usize>(_tmp_dir: &String, flags: [&str; N]) -> io::Result<Child> {
    Command::new(COQTOP)
        .arg("-main-channel")
        .arg("stdfds")
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}

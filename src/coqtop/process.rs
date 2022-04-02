use std::{io, process::Stdio, sync::Arc};

use tokio::{
    io::AsyncWriteExt,
    join,
    net::{TcpListener, TcpStream},
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
pub const COQTOP: &'static str = "coqidetop";

pub struct CoqIdeTop {
    /// The main channel where [`COQTOP`] sends its responses.
    //main_r: TcpStream,
    /// The main channel to send commands (calls, see [`ProtocolCall`]) to [`COQTOP`].
    ///
    /// [`ProtocolCall`]: crate::coqtop::xml_protocol::types::ProtocolCall
    main_w: TcpStream,
    /// The underlying process.
    process: Child,
    /// The framed reader which decodes all input coming from [`COQTOP`]'s stdout.
    reader: FramedRead<TcpStream, XMLDecoder>,
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
        let (main_w_listen, main_w_port) = create_listener().await?;
        let (main_r_listen, main_r_port) = create_listener().await?;

        let ports = [main_r_port, main_w_port];

        // NOTE: `async { X.await }` can also be written `X`. However, I find it less clear when types
        // are not inlined in my code (which rust-analyzer is able to do).
        // Please do not refactor this...
        let main_r = async { main_r_listen.accept().await };
        let main_w = async { main_w_listen.accept().await };

        let flags = [
            // "-async-proofs",
            // "on",
            "-topfile",
            &edited_file(session.clone()),
        ];
        // TODO: add flags found in a `_CoqProject` file

        let mut coqidetop =
            async { coqidetop(&temporary_folder(session.clone()), ports, flags).await };

        let (main_r, main_w, coqidetop) = join!(main_r, main_w, coqidetop);
        // NOTE: because we are using TCP streams, we don't care about the second parameter returned by [`TcpListener::accept`]
        // hence all the `.0`s.
        let (main_r, main_w, coqidetop) = (main_r?.0, main_w?.0, coqidetop?);

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
        self.reader.into_inner().shutdown().await?;
        self.process.kill().await?;

        Ok(())
    }
}

/// Creates a new [`TcpListener`] listening on `127.0.0.1:0`, and returns both the
/// listener and the port it is listening on.
async fn create_listener() -> io::Result<(TcpListener, u16)> {
    let listen = TcpListener::bind("127.0.0.1:0").await?;
    let port = listen.local_addr()?.port();

    Ok((listen, port))
}

/// Spawns a new [`COQTOP`] process by feeding it additional flags to take in account.
async fn coqidetop<const N: usize>(
    _tmp_dir: &String,
    ports: [u16; 2],
    flags: [&str; N],
) -> io::Result<Child> {
    Command::new(COQTOP)
        .arg("-main-channel")
        .arg(format!("127.0.0.1:{}:{}", ports[0], ports[1]))
        //.arg("stdfds")
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}

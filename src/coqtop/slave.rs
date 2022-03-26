use std::{
    io::{self, SeekFrom},
    process::Stdio,
    sync::{Arc, Mutex},
};

use tokio::{
    fs::File,
    io::{AsyncSeekExt, AsyncWriteExt},
    join,
    net::{TcpListener, TcpStream},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{mpsc, watch},
};
use tokio_util::codec::FramedRead;

use crate::{
    coqtop::xml_protocol::types::{
        FeedbackContent, MessageType, ProtocolRichPP, ProtocolRichPPPart,
    },
    files::{goal_file, result_file},
    logger,
};

use super::xml_protocol::{
    parser::{xml_decoder, XMLDecoder},
    types::{ProtocolCall, ProtocolResult, ProtocolValue},
};

/// The name of the process used for IDE interactions with Coq.
pub const COQTOP: &'static str = "coqidetop";

/// The structure encapsulating all communications with the underlying [`COQTOP`] process.
pub struct CoqtopSlave {
    /// The main channel where [`COQTOP`] sends its responses.
    //main_r: TcpStream,
    /// The main channel to send commands (calls, see [`ProtocolCall`]) to [`COQTOP`].
    ///
    /// [`ProtocolCall`]: crate::coqtop::xml_protocol::types::ProtocolCall
    //main_w: TcpStream,
    //main_r: ChildStdout,
    main_w: ChildStdin,

    /// The [`COQTOP`] process itself.
    coqidetop: Child,
    /// The receiving end of a channel used to transmit protocol calls to send to [`COQTOP`].
    call_rx: mpsc::UnboundedReceiver<ProtocolCall>,
    /// The sending end of a channel used to transmit responses from [`COQTOP`].
    response_tx: mpsc::UnboundedSender<ProtocolResult>,

    reader: FramedRead<ChildStdout, XMLDecoder>,
}

impl CoqtopSlave {
    /// Creates a new [`CoqtopSlave`] by spawning 2 or 4 TCP sockets as well as a [`COQTOP`] process.
    pub async fn new(
        call_rx: mpsc::UnboundedReceiver<ProtocolCall>,
        response_tx: mpsc::UnboundedSender<ProtocolResult>,
        tmp_dir: &String,
        topfile: String,
    ) -> io::Result<Self> {
        //let (main_w_listen, main_w_port) = create_listener().await?;
        //let (main_r_listen, main_r_port) = create_listener().await?;

        // NOTE: `async { X.await }` can also be written `X`. However, I find it less clear when types
        // are not inlined in my code (which rust-analyzer is able to do).
        // Please do not refactor this...
        //let main_r = async { main_r_listen.accept().await };
        //let main_w = async { main_w_listen.accept().await };

        //let ports = [main_r_port, main_w_port];
        let flags = [/*"-async-proofs", "on",*/ "-topfile", &topfile];

        let mut coqidetop = coqidetop(tmp_dir, /*ports,*/ flags).await?;
        let main_w = coqidetop.stdin.take().unwrap();
        let main_r = coqidetop.stdout.take().unwrap();

        //let (main_r, main_w, coqidetop) = join!(main_r, main_w, coqidetop);
        // NOTE: because we are using TCP streams, we don't care about the second parameter returned by [`TcpListener::accept`]
        // hence all the `.0`s.
        //let (main_r, main_w, coqidetop) = (main_r?.0, main_w?.0, coqidetop?);

        log::info!(
            "{} (process {}) is up and running!",
            COQTOP,
            coqidetop.id().unwrap_or(0)
        );

        let reader = xml_decoder(main_r);

        Ok(Self {
            //main_r,
            main_w,
            coqidetop,
            call_rx,
            response_tx,
            reader,
        })
    }

    /// Runs a join point which processes anything related to [`COQTOP`]:
    /// - until `stop_rx` receives a value (in which case it ends).
    /// - when a [`ProtocolCall`] is received through the `main_w` channel, it encodes it
    ///   and sends it directly to [`COQTOP`].
    /// - when a [`ProtocolResult`] can be decoded from [`COQTOP`], try to process the response
    ///   according to these rules:
    ///   - if the response is a [`ProtocolResult::Fail`], output the error to the result buffer,
    ///     change the current state to be non-processing and report the error to Kakoune.
    ///   - if the response is a [`ProtocolResult::Good`] and it contains a [`ProtocolValue::StateId`], update
    ///     the current state ID.
    ///   - if the response is a [`ProtocolResult::Feedback`] and its content as a `processed` tag, update
    ///     the processed range in Kakoune.
    ///   - if the response is a [`ProtocolResult::Feedback`] and its content as a `message` tag,
    ///     output the message to the result buffer.
    ///   - else no special treatment is reserved, therefore we can ignore
    pub async fn process(
        &mut self,
        mut stop_rx: watch::Receiver<()>,
    ) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                resp = ProtocolResult::decode_stream(&mut self.reader) => {
                    let resp = resp?;

                    log::debug!("Received response `{:?}` from {}", resp, COQTOP);

                    self.response_tx.send(resp).unwrap();
                }
                Some(call) = self.call_rx.recv() => {
                    let encoded = call.encode();
                    log::debug!("Sending encoded command `{}` to {}", encoded, COQTOP);

                    self.main_w.write_all(encoded.as_bytes()).await?;
                }
            }
        }
    }

    /// Drops the TCP sockets as well as the [`COQTOP`] process.
    pub async fn quit(mut self) -> io::Result<()> {
        log::debug!("Shutting down communication channels");
        //self.main_r.shutdown().await?;
        self.main_w.shutdown().await?;

        log::debug!("Stopping {}", COQTOP);
        self.coqidetop.kill().await?;

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

/// Spawns a new [`COQTOP`] process given the 2 or 4 ports it should connect to
/// (in order: `[main_r, main_w, control_r, control_w]`) as well as some more flags
/// (e.g. `["-topfile", file]`).
async fn coqidetop<const N: usize>(
    tmp_dir: &String,
    //ports: [u16; 2],
    flags: [&str; N],
) -> io::Result<Child> {
    Command::new(COQTOP)
        .arg("-main-channel")
        .arg("stdfds")
        //.arg(format!("127.0.0.1:{}:{}", ports[0], ports[1]))
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        //.stderr(std::fs::File::create(logger::log_file(&tmp_dir))?)
        .kill_on_drop(true)
        .spawn()
}

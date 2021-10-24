use std::{fs::File, io, process::Stdio, sync::Arc};

use tokio::{
    io::AsyncWriteExt,
    join,
    net::{TcpListener, TcpStream},
    process::{Child, Command},
};

use crate::logger;

use super::xml_protocol::types::{ProtocolCall, ProtocolResult};

/// The name of the process used for IDE interactions with Coq.
pub static COQTOP: &'static str = "coqidetop";

/// The structure encapsulating all communications with the underlying [`COQTOP`] process.
pub struct IdeSlave {
    /// The main channel where [`COQTOP`] sends its responses.
    main_r: TcpStream,
    /// The main channel to send commands (calls, see [`ProcotolCall`]) to [`COQTOP`].
    main_w: TcpStream,
    /// The [`COQTOP`] process itself.
    coqidetop: Child,
}

impl IdeSlave {
    /// Creates a new [`ideSlave`] by spawning 4 TCP sockets as well as a [`COQTOP`] process.
    pub async fn new(tmp_dir: &String, topfile: String) -> io::Result<Self> {
        let (main_w_listen, main_w_port) = create_listener().await?;
        let (main_r_listen, main_r_port) = create_listener().await?;

        // NOTE: `async { X.await }` can also be written `X`. However, I find it less clear when types
        // are not inlined in my code (which rust-analyzer is able to do).
        // Please do not refactor this...
        let main_r = async { main_r_listen.accept().await };
        let main_w = async { main_w_listen.accept().await };

        let ports = [main_r_port, main_w_port];
        let flags = ["-async-proofs", "on", "-topfile", &topfile];

        let coqidetop = async { coqidetop(tmp_dir, ports, flags).await };

        let (main_r, main_w, coqidetop) = join!(main_r, main_w, coqidetop);
        // NOTE: because we are using TCP streams, we don't care about the second parameter returned by [`TcpListener::accept`]
        // hence all the `.0`s.
        let (main_r, main_w, coqidetop) = (main_r?.0, main_w?.0, coqidetop?);

        log::info!(
            "{} (process {}) is up and running!",
            COQTOP,
            coqidetop.id().unwrap_or(0)
        );

        Ok(Self {
            main_r,
            main_w,
            coqidetop,
        })
    }

    /// Send a [`ProtocolCall`] to the underlying [`COQTOP`] process.
    pub async fn send(&mut self, call: ProtocolCall) -> io::Result<()> {
        self.main_w.write_all(call.encode().as_bytes()).await
    }

    pub async fn recv(&mut self) -> io::Result<ProtocolResult> {
        ProtocolResult::decode_stream(&mut self.main_r).await
    }

    //     /// Sends a [`ProtocolCall`] to [`COQTOP`], and returns its response as a [`ProtocolResult`].
    //     pub async fn send(&mut self, call: ProtocolCall) -> io::Result<ProtocolResult> {
    //         log::debug!("Sending call `{:?}` to `{}`", call, COQTOP);
    //         self.main_w.write_all(call.encode().as_bytes()).await?;

    //         let response = ProtocolResult::decode_stream(&mut self.main_r).await?;

    //         log::debug!("`{}` responded with `{:?}`", COQTOP, response);
    //         Ok(response)
    //     }

    /// Drops the TCP sockets as well as the [`COQTOP`] process.
    pub async fn quit(mut self) -> io::Result<()> {
        log::debug!("Shutting down communication channels"); 
        self.main_r.shutdown().await?;
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

/// Spawns a new [`COQTOP`] process given the 4 ports it should connect to
/// (in order: `[main_r, main_w, control_r, control_w]`) as well as some more flags
/// (e.g. `["-topfile", file]`).
async fn coqidetop<const N: usize>(
    tmp_dir: &String,
    ports: [u16; 2],
    flags: [&str; N],
) -> io::Result<Child> {
    Command::new(COQTOP)
        .arg("-main-channel")
        .arg(format!("127.0.0.1:{}:{}", ports[0], ports[1]))
        .args(flags)
        .stdout(File::create(logger::log_file(&tmp_dir))?)
        .kill_on_drop(true)
        .spawn()
}

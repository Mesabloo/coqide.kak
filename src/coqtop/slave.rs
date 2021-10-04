use crate::{coqtop, xml_protocol::types::ProtocolCall};
use async_net::{TcpListener, TcpStream};
use async_process::Child;
use futures::{join, AsyncWriteExt};
use std::io;

pub struct IdeSlave {
    main_r: Box<TcpStream>,
    main_w: Box<TcpStream>,
    control_r: Box<TcpStream>,
    control_w: Box<TcpStream>,
    proc: Child,
    state: SlaveState,
}

/// Create a new TCP server and return its socket
async fn new_server(addr: String) -> io::Result<Box<TcpStream>> {
    let listener = TcpListener::bind(addr).await?;
    let (socket, _addr) = listener.accept().await?;

    Ok(Box::new(socket))
}

impl IdeSlave {
    pub async fn init(ports: &[u32; 4]) -> io::Result<Self> {
        let main_r = new_server(format!("127.0.0.1:{}", ports[0]));
        let main_w = new_server(format!("127.0.0.1:{}", ports[1]));
        let control_r = new_server(format!("127.0.0.1:{}", ports[2]));
        let control_w = new_server(format!("127.0.0.1:{}", ports[3]));

        let proc = coqtop::spawn(ports);

        let (main_r, main_w, control_r, control_w, proc) =
            join!(main_r, main_w, control_r, control_w, proc);
        let (main_r, main_w, control_r, control_w, proc) =
            (main_r?, main_w?, control_r?, control_w?, proc?);

        log::debug!(
            "`{}` (process {}) is up and running!",
            coqtop::COQTOP,
            proc.id()
        );

        Ok(Self {
            main_r,
            main_w,
            control_r,
            control_w,
            proc,
            state: SlaveState::Connected,
        })
    }

    pub async fn quit(mut self) -> io::Result<()> {
        log::info!("Closing communication channels");

        self.main_w
            .write_all(ProtocolCall::Quit.encode().as_bytes())
            .await?;

        self.main_r.shutdown(async_net::Shutdown::Read)?;
        self.main_w.shutdown(async_net::Shutdown::Write)?;
        self.control_r.shutdown(async_net::Shutdown::Read)?;
        self.control_w.shutdown(async_net::Shutdown::Write)?;

        log::info!("Stopping `{}` process", crate::coqtop::COQTOP);

        self.proc.kill()?;
        self.state = SlaveState::Disconnected;

        Ok(())
    }
}

pub enum SlaveState {
    Connected,
    Disconnected,
    Error,
}

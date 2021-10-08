use crate::{coqtop, xml_protocol::types::{ProtocolCall, ProtocolValue}};
use async_net::{TcpListener, TcpStream};
use async_process::Child;
use futures::{join, AsyncWriteExt};
use std::io;

pub struct IdeSlave {
    main_r: TcpStream,
    main_w: TcpStream,
    control_r: TcpStream,
    control_w: TcpStream,
    proc: Child,
    state: SlaveState,
}

impl IdeSlave {
    pub async fn init(file: String) -> io::Result<Self> {
        let listener1 = TcpListener::bind("127.0.0.1:0").await?;
        let listener2 = TcpListener::bind("127.0.0.1:0").await?;
        let listener3 = TcpListener::bind("127.0.0.1:0").await?;
        let listener4 = TcpListener::bind("127.0.0.1:0").await?;

        let main_r = listener1.accept();
        let main_w = listener2.accept();
        let control_r = listener3.accept();
        let control_w = listener4.accept();

        let additional_flags = [
            "-async-proofs".to_string(),
            "on".to_string(),
            "-topfile".to_string(),
            file,
        ];
        let proc = coqtop::spawn(
            [
                listener1.local_addr()?.port(),
                listener2.local_addr()?.port(),
                listener3.local_addr()?.port(),
                listener4.local_addr()?.port(),
            ],
            &additional_flags,
        );

        let (main_r, main_w, control_r, control_w, proc) =
            join!(main_r, main_w, control_r, control_w, proc);
        let ((main_r, _), (mut main_w, _), (control_r, _), (control_w, _), proc) =
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

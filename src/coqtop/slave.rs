use async_net::TcpStream;
use async_process::Child;

pub struct IdeSlave {
    main_r: Box<TcpStream>,
    main_w: Box<TcpStream>,
    control_r: Box<TcpStream>,
    control_w: Box<TcpStream>,
    proc: Child,
    state: SlaveState,
}

impl IdeSlave {
    pub fn new(
        main_r: Box<TcpStream>,
        main_w: Box<TcpStream>,
        control_r: Box<TcpStream>,
        control_w: Box<TcpStream>,
        proc: Child,
        state: SlaveState,
    ) -> Self {
        IdeSlave {
            main_r,
            main_w,
            control_r,
            control_w,
            state,
            proc,
        }
    }
}

pub enum SlaveState {
    Connected,
    Disconnected,
    Error,
}

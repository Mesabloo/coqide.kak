use async_process::{Child, Command, Stdio};
use std::io;

pub mod slave;

pub async fn spawn(ports: &[u32; 4]) -> io::Result<Child> {
    Command::new("coqidetop")
        .arg("-main-channel")
        .arg(format!("127.0.0.1:{}:{}", ports[0], ports[1]))
        .arg("-control-channel")
        .arg(format!("127.0.0.1:{}:{}", ports[2], ports[3]))
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}

use async_process::{Child, Command, Stdio};
use std::io;

pub mod slave;

/// The name of the REPL executable (most probably `coqidetop` or `coqtop`)
pub static COQTOP: &str = "coqidetop";

/// Spawns the `COQTOP` program and try to connect it to the given local ports.
///
/// - `ports[0]` is the port of the main readable channel, used to retrieve messages
/// - `ports[1]` is the port of the main writable channel, used to send requests to `COQTOP`
/// - `ports[2]` ???
/// - `ports[3]` ???
pub async fn spawn(ports: &[u32; 4]) -> io::Result<Child> {
    Command::new(COQTOP)
        .arg("-main-channel")
        .arg(format!("127.0.0.1:{}:{}", ports[0], ports[1]))
        .arg("-control-channel")
        .arg(format!("127.0.0.1:{}:{}", ports[2], ports[3]))
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}

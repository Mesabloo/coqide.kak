use std::{io, process::Stdio, sync::Arc};

use tokio::{io::AsyncWriteExt, process::Command};

use super::session::SessionWrapper;

/// Launches a new `kak` process connected to the session given using the first argument
/// and tries to send commands to it.
pub async fn kak(session: Arc<SessionWrapper>, commands: String) -> io::Result<()> {
    let mut proc = Command::new("kak")
        .arg("-p")
        .arg(session.id())
        .stdin(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdin = proc
        .stdin
        .as_mut()
        .expect("could not get stdin of 'kak' process");
    stdin.write_all(commands.as_bytes()).await?;
    proc.wait().await?;

    Ok(())
}

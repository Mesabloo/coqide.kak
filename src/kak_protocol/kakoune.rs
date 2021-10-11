use std::{io, process::Stdio};
use tokio::{io::AsyncWriteExt, process::Command};

pub async fn kakoune(session: String, stdin: String) -> io::Result<()> {
    let mut proc = Command::new("kak")
        .arg("-p")
        .arg(session)
        .stdin(Stdio::piped())
        .spawn()?;

    let kak_stdin = proc.stdin.as_mut().unwrap();
    kak_stdin.write_all(stdin.as_bytes()).await?;

    proc.wait().await.map(|_| ())
}

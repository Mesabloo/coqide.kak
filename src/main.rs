#![feature(box_patterns)]

use std::{
    env,
    path::Path,
    process::{exit, Stdio},
};

use async_signals::Signals;
use tokio::{
    fs::File,
    io,
    net::TcpListener,
    sync::{mpsc, watch},
};
use tokio_stream::StreamExt;

use crate::coqtop::xml_protocol::types::ProtocolResult;
use crate::kakoune::commands::types::Command;
use crate::{
    channels::{CommandProcessor, CommandReceiver, ResponseProcessor, ResponseReceiver},
    files::{goal_file, result_file, COQTOP},
};

mod channels;
mod coqtop;
mod files;
mod kakoune;
mod logger;

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        eprintln!(
            "3 arguments needed: <KAK_SESSION> <COQ_FILE> <TMP_DIR>\n{} provided.",
            args.len()
        );

        exit(exitcode::CONFIG);
    }

    let kak_session = args[1].clone();
    let coq_file = args[2].clone();
    let tmp_dir = args[3].clone();

    // Create all necessary files
    for fun in &[
        logger::log_file,
        goal_file,
        result_file, /*, command_file*/
    ] {
        let path = fun(&tmp_dir);
        let path = Path::new(&path);
        if !Path::exists(path) {
            File::create(&path).await?;
        }
    }

    // Initialise logging
    let _handle = logger::init(logger::log_file(&tmp_dir))?;

    let (stop_tx, stop_rx) = watch::channel(());

    // - `pipe_tx` is used to transmit commands from the pipe file to the internal channel
    // - `pipe_rx` is the receiving end used to get those commands
    let (pipe_tx, pipe_rx) = mpsc::unbounded_channel::<Command>();
    // - `result_tx` is the transmitting end of [`COQTOP`] responses
    // - `result_rx` receives responses for further processing
    let (result_tx, result_rx) = mpsc::unbounded_channel::<ProtocolResult>();

    let main_r_listener = TcpListener::bind("127.0.0.1:0").await?;
    let main_r_port = main_r_listener.local_addr()?.port();
    let main_w_listener = TcpListener::bind("127.0.0.1:0").await?;
    let main_w_port = main_w_listener.local_addr()?.port();

    let main_r = async { main_r_listener.accept().await };
    let main_w = async { main_w_listener.accept().await };
    let coqidetop = async {
        tokio::process::Command::new(COQTOP)
            .arg("-main-channel")
            .arg(format!("127.0.0.1:{}:{}", main_r_port, main_w_port))
            .arg("-topfile")
            .arg(&coq_file)
            .arg("-async-proofs")
            .arg("on")
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
    };

    log::debug!(
        "Connecting {} to ports {}:{}",
        COQTOP,
        main_r_port,
        main_w_port
    );

    let (main_r, main_w, coqidetop) = tokio::join!(main_r, main_w, coqidetop);
    let (main_r, main_w, mut coqidetop) = (main_r?.0, main_w?.0, coqidetop?);

    let mut kakoune_command_receiver = CommandReceiver::new(pipe_tx, stop_rx.clone());
    let mut kakoune_command_processor = CommandProcessor::new(pipe_rx, main_w, stop_rx.clone());
    let mut coqidetop_response_receiver = ResponseReceiver::new(result_tx, main_r, stop_rx.clone());
    let mut coqidetop_response_processor =
        ResponseProcessor::new(result_rx, stop_rx.clone(), &tmp_dir, &kak_session, &coq_file).await?;

    let mut signals = Signals::new(vec![libc::SIGINT]).unwrap();
    loop {
        tokio::select! {
            Some(libc::SIGINT) = signals.next() => {
                stop_tx.send(()).unwrap();
                break Ok(());
            }
            res = kakoune_command_receiver.process(kak_session.clone(), tmp_dir.clone(), coq_file.clone()) => break res,
            res = kakoune_command_processor.process() => break res,
            res = coqidetop_response_receiver.process() => break res,
            res = coqidetop_response_processor.process() => break res,
            else => {}
        }
    }?;

    kakoune_command_receiver.stop().await?;
    kakoune_command_processor.stop().await?;

    log::debug!("Killing {}", COQTOP);

    coqidetop.kill().await?;

    log::debug!("Shutting down all sockets");

    coqidetop_response_processor.stop().await?;
    coqidetop_response_receiver.stop().await?;

    drop(signals);

    Ok(())
}

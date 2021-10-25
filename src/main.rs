#![feature(box_patterns)]
#![feature(async_closure)]
#![feature(io_error_more)]

use std::{env, path::Path, process::exit, sync::{Arc, RwLock}};

use async_signals::Signals;
use tokio::{
    fs::File,
    io,
    sync::{mpsc, watch},
};
use tokio_stream::StreamExt;

use crate::{
    coqtop::{slave::IdeSlave, xml_protocol::types::ProtocolCall},
    files::{goal_file, result_file},
    kakoune::{
        commands::{processor::CommandProcessor, receiver::CommandReceiver, types::Command},
        slave::KakSlave,
    },
    state::CoqState,
};

mod coqtop;
mod files;
mod kakoune;
mod logger;
mod state;

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
    //
    let (call_tx, call_rx) = mpsc::unbounded_channel::<ProtocolCall>();
    //
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<String>();

    let coq_state = Arc::new(RwLock::new(CoqState::new()));

    let mut ideslave = IdeSlave::new(call_rx, cmd_tx.clone(), &tmp_dir, coq_file.clone()).await?;
    let mut kakoune_command_receiver = CommandReceiver::new(pipe_tx, stop_rx.clone());
    let mut kakoune_command_processor = CommandProcessor::new(
        pipe_rx,
        call_tx,
        coq_state.clone(),
        goal_file(&tmp_dir),
        result_file(&tmp_dir),
    )
    .await?;
    let mut kakslave = KakSlave::new(cmd_rx, kak_session.clone(), coq_file.clone(), &tmp_dir);

    let mut signals = Signals::new(vec![libc::SIGINT]).unwrap();
    loop {
        tokio::select! {
            Some(libc::SIGINT) = signals.next() => {
                stop_tx.send(()).unwrap();
                break Ok(());
            }
            res = kakoune_command_receiver.process(kak_session.clone(), tmp_dir.clone(), coq_file.clone()) => break res,
            res = kakoune_command_processor.process(stop_rx.clone()) => break res,
            res = ideslave.process(coq_state.clone(), stop_rx.clone()) => break res,
            res = kakslave.process(stop_rx.clone()) => break res,
            else => {}
        }
    }?;

    kakoune_command_receiver.stop().await?;
    ideslave.quit().await?;

    drop(signals);

    Ok(())
}

#![feature(box_patterns)]

use std::{env, path::Path, process::exit};

use async_signals::Signals;
use coqtop::{
    adapter::SynchronizedState,
    feedback_queue::{Feedback, FeedbackQueue},
    slave::CoqtopSlave,
    xml_protocol::types::{ProtocolCall, ProtocolResult},
};
use files::{goal_file, result_file};
use kakoune::{
    commands::{
        receiver::CommandReceiver,
        types::{DisplayCommand, KakouneCommand},
    },
    slave::KakSlave,
};
use tokio::{
    fs::File,
    sync::{mpsc, watch},
};
use tokio_stream::StreamExt;

mod codespan;
mod coqtop;
mod files;
mod kakoune;
mod logger;

#[tokio::main]
pub async fn main() {
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
            File::create(&path).await.unwrap();
        }
    }

    // Initialise logging
    let _handle = logger::init(logger::log_file(&tmp_dir)).unwrap();

    let (stop_tx, stop_rx) = watch::channel(());

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<KakouneCommand>();
    let (call_tx, call_rx) = mpsc::unbounded_channel::<ProtocolCall>();
    let (response_tx, response_rx) = mpsc::unbounded_channel::<ProtocolResult>();
    let (disp_cmd_tx, disp_cmd_rx) = mpsc::unbounded_channel::<DisplayCommand>();
    let (feedback_tx, feedback_rx) = mpsc::unbounded_channel::<Feedback>();

    let mut coqtop_slave = CoqtopSlave::new(call_rx, response_tx, &tmp_dir, coq_file.clone())
        .await
        .unwrap();
    let mut coqtop_adapter = SynchronizedState::new(
        call_tx,
        response_rx,
        feedback_tx,
        cmd_rx,
        disp_cmd_tx.clone(),
        kak_session.clone(),
        coq_file.clone(),
    );
    let mut kakoune_receiver =
        CommandReceiver::new(cmd_tx, kak_session.clone(), &tmp_dir, coq_file.clone());
    let mut kakoune_slave =
        KakSlave::new(disp_cmd_rx, kak_session.clone(), coq_file.clone(), &tmp_dir);
    let mut feedback_queue = FeedbackQueue::new(feedback_rx, disp_cmd_tx.clone());

    let mut signals = Signals::new(vec![libc::SIGINT]).unwrap();
    loop {
        tokio::select! {
            Some(libc::SIGINT) = signals.next() => {
                stop_tx.send(()).unwrap();
                break Ok(());
            }
            res = coqtop_slave.process(stop_rx.clone()) => break res,
            res = coqtop_adapter.process(stop_rx.clone()) => break res,
            res = kakoune_receiver.process(stop_rx.clone()) => break res,
            res = kakoune_slave.process(stop_rx.clone()) => break res,
            res = feedback_queue.process(stop_rx.clone()) => break res,
            else => {}
        }
    }
    .unwrap();

    kakoune_receiver.stop().await.unwrap();
    coqtop_slave.quit().await.unwrap();

    drop(signals);
}

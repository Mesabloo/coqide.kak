#![feature(box_patterns)]

use std::{
    io,
    path::Path,
    process::exit,
    sync::{Arc, RwLock},
};

use client::bridge::ClientBridge;
use coqtop::{coqidetop::CoqIdeTop, processor::CoqIdeTopProcessor};
use files::{goal_file, log_file, result_file};
use kakoune::{command_line::kak, ui_updater::KakouneUIUpdater};
use session::{edited_file, session_id, temporary_folder, Session};
use state::State;
use tokio::{fs::File, sync::watch};

mod client;
mod coqtop;
mod files;
mod kakoune;
mod logger;
mod range;
mod session;
mod state;

#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        eprintln!(
            "5 arguments needed on the command-line: <KAK_CLIENT> <KAK_SESSION> <COQ_FILE> <TMP_DIR> <INPUT_FIFO>\n{} provided.",
            args.len() - 1
        );
        exit(exitcode::CONFIG);
    }

    let session = Session::new(
        args[1].clone(),
        args[2].clone(),
        args[3].clone(),
        args[4].clone(),
        args[5].clone(),
    );

    for fun in &[log_file, goal_file, result_file] {
        let path = fun(&temporary_folder(session.clone()));
        let path = Path::new(&path);
        if !Path::exists(path) {
            File::create(&path).await.unwrap();
        }
    }

    let _handle = logger::init(log_file(&temporary_folder(session.clone()))).unwrap();
    // from now on, we can use the macros inside log::

    let (stop_tx, stop_rx) = watch::channel(());

    let res = loop {
        let mut stop_rx1 = stop_rx.clone();
        let stop_rx2 = stop_rx.clone();
        tokio::select! {
            Ok(_) = stop_rx1.changed() => break Ok(()),
            res = main_loop(stop_rx2, stop_tx, session.clone()) => break res,
        }
    };
    log::debug!("Global result: {:?}", res);

    kak(
        &session_id(session.clone()),
        format!(
            r#"evaluate-commands -buffer '{}' %{{ coqide-purge }}"#,
            edited_file(session.clone())
        ),
    )
    .await
    .unwrap();
}

async fn main_loop(
    stop_rx: watch::Receiver<()>,
    stop_tx: watch::Sender<()>,
    session: Arc<Session>,
) -> io::Result<()> {
    let state = Arc::new(RwLock::new(State::new()));

    let mut client_bridge =
        ClientBridge::new::<100>(session.clone(), state.clone(), stop_tx).await?;
    let mut coqtop_bridge = CoqIdeTop::spawn(session.clone()).await?;
    let mut coqtop_processor = CoqIdeTopProcessor::new(
        session.clone(),
        state.clone(),
        client_bridge.command_tx.clone(),
    )?;
    let mut ui_updater = KakouneUIUpdater::new(session.clone());

    loop {
        let cmd = client_bridge.recv(stop_rx.clone()).await?;
        let (call, cmd, display3) = client_bridge.process(cmd).await?;

        ui_updater.process(display3.into_iter().collect()).await?;
        if let Some(call) = call {
            let (response, feedback) = coqtop_bridge.ask(call).await?;

            let mut display2 = coqtop_processor.process_feedback(feedback).await?;
            let mut display = coqtop_processor.process_response(response, cmd).await?;

            // When we receive some feedback, we want to process it first.
            // In any case, this will not change the global application state, compared
            // to processing a response.
            display.append(&mut display2);

            ui_updater.process(display).await?;
        }
    }
}

#![feature(box_patterns)]

use std::{
    io,
    path::Path,
    process::exit,
    sync::{Arc, Mutex},
};

use client::input::ClientInput;
use coqtop::{process::CoqIdeTop, response_processor::ResponseProcessor};
use files::{goal_file, log_file, result_file};
use kakoune::{command_line::kak, ui_updater::KakouneUIUpdater};
use session::{edited_file, session_id, temporary_folder, Session};
use state::State;
use tokio::{
    fs::File,
    sync::{mpsc, watch},
};

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
    if args.len() != 5 {
        eprintln!(
            "4 arguments needed on the command-line: <KAK_SESSION> <COQ_FILE> <TMP_DIR> <INPUT_FIFO>\n{} provided.",
            args.len() - 1
        );
        exit(exitcode::CONFIG);
    }

    let session = Session::new(
        args[1].clone(),
        args[2].clone(),
        args[3].clone(),
        args[4].clone(),
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

    let (stop_tx, mut stop_rx) = watch::channel(());

    let (coqtop_call_tx, coqtop_call_rx) = mpsc::unbounded_channel();
    let (coqtop_response_tx, coqtop_response_rx) = mpsc::unbounded_channel();
    let (kakoune_display_tx, kakoune_display_rx) = mpsc::unbounded_channel();

    let state = Arc::new(Mutex::new(State::new()));

    let mut coqtop_bridge = CoqIdeTop::spawn(session.clone(), coqtop_call_rx, coqtop_response_tx)
        .await
        .unwrap();
    let mut client_bridge = ClientInput::new(
        session.clone(),
        kakoune_display_tx.clone(),
        coqtop_call_tx,
        stop_tx,
        state.clone(),
    )
    .await
    .unwrap();
    let command_tx = client_bridge.command_tx.clone();
    let command_rx = command_tx.subscribe();
    let mut response_processor = ResponseProcessor::new(
        session.clone(),
        command_rx,
        command_tx,
        coqtop_response_rx,
        kakoune_display_tx,
        state.clone(),
    );
    let mut ui_updater = KakouneUIUpdater::new(session.clone(), state.clone(), kakoune_display_rx);

    let stop_rx1 = stop_rx.clone();
    let stop_rx2 = stop_rx.clone();
    let stop_rx3 = stop_rx.clone();
    let stop_rx4 = stop_rx.clone();
    let handle1 = tokio::spawn(async move {
        coqtop_bridge.transmit_until(stop_rx1).await?;
        Ok::<_, io::Error>(coqtop_bridge)
    });
    let handle2 = tokio::spawn(async move { client_bridge.handle_commands_until(stop_rx2).await });
    let handle3 = tokio::spawn(async move { response_processor.process_until(stop_rx3).await });
    let handle4 = tokio::spawn(async move { ui_updater.update_until(stop_rx4).await });
    let handle5 = tokio::spawn(async move { stop_rx.changed().await });

    let res = tokio::try_join!(handle5, handle4, handle1, handle2, handle3);

    match res {
        Ok((r5, r4, r1, r2, r3)) => {
            let coqtop_bridge = r1.unwrap();
            r2.unwrap();
            r3.unwrap();
            r4.unwrap();
            r5.unwrap();

            log::debug!("Stopping CoqIDE daemon");

            coqtop_bridge.quit().await.unwrap();

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
        Err(e) => {
            panic!("{:?}", e);
        }
    }
}

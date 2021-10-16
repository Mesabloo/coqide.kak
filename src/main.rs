#![feature(box_patterns)]
#![feature(try_trait_v2)]
#![feature(async_closure)]

use std::{cell::RefCell, env, io, path::Path, rc::Rc, sync::Arc};

use signal_hook::{
    consts::{SIGINT, SIGUSR1},
    iterator::Signals,
};
use tokio::{fs::File, sync::RwLock};

use crate::{
    coqtop::slave::IdeSlave,
    daemon::DaemonState,
    kakoune::{
        command_line::kak,
        commands::processor::CommandProcessor,
        session::SessionWrapper,
        slave::{command_file, KakSlave},
    },
    result::{goal::goal_file, result::result_file},
};

mod coqtop;
mod daemon;
mod kakoune;
mod logger;
mod result;

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = env::args().collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        4,
        "Help: <KAK_SESSION> <COQ_FILE> <KAK_TMP_DIR> ({} provided)",
        args.len() - 1
    );
    let kak_session = args[1].clone();
    let coq_file = args[2].clone();
    let kak_tmp_dir = args[3].clone();

    let session = Arc::new(SessionWrapper::new(kak_session, kak_tmp_dir));

    // Create all necessary files
    for fun in &[logger::log_file, goal_file, result_file] {
        let path = fun(session.clone());
        let path = Path::new(&path);
        if !Path::exists(path) {
            File::create(&path).await?;
        }
    }
    for fun in &[command_file] {
        let path = fun(session.clone());
        let path = Path::new(&path);
        if !Path::exists(path) {
            unix_named_pipe::create(path, None)?;
        }
    }

    // Initialise logging
    let _handle = logger::init(logger::log_file(session.clone()))?;

    // Initialise the IDE slave
    log::debug!("Initialising IDE slave");
    let ideslave = Arc::new(RwLock::new(
        IdeSlave::new(session.clone(), coq_file.clone()).await?,
    ));
    // Initialise the daemon state
    log::debug!("Creating daemon state");
    let state = Arc::new(RwLock::new(DaemonState::default()));
    // Initialise the Kakoune slave
    log::debug!("Initialising Kakoune slave");
    let kakslave = Arc::new(KakSlave::new(
        session.clone(),
        ideslave.clone(),
        state.clone(),
    )?);
    // Initialise the command processor
    log::debug!("Starting command processing");
    let processor = Arc::new(RwLock::new(CommandProcessor::new(
        session.clone(),
        kakslave.clone(),
    )?));
    let processor_ = processor.clone();

    let process_thread = tokio::task::spawn(async move {
        log::debug!("Start waiting for commands");

        processor.write().await.start().await?;

        Ok::<_, io::Error>(())
    });

    let mut signals = Signals::new(&[SIGINT])?;
    let signals_handle = signals.handle();
    let signals_thread = tokio::spawn(async move {
        signals.wait().next();
        log::debug!("Received signal SIGINT");

        process_thread.abort();

        log::debug!("Stopping daemon");

        processor_.write().await.stop().await?;
        signals_handle.close();

        Ok::<_, io::Error>(())
    });

    log::debug!("Creating buffers in Kakoune");
    kak(session.clone(), format!(
                r#"evaluate-commands -buffer '{0}' %{{ edit! -readonly -fifo "{1}" "%opt{{coqide_result_buffer}}" }}
                evaluate-commands -buffer '{0}' %{{ edit! -readonly -fifo "{2}" "%opt{{coqide_goal_buffer}}" }}
                evaluate-commands -buffer '{0}' %{{ coqide-send-to-process 'init' }}"#,
                coq_file, result_file(session.clone()), goal_file(session.clone()),
            )).await?;

    signals_thread.await??;

    drop(kakslave);
    drop(state);

    if let Ok(ideslave) = Arc::try_unwrap(ideslave) {
        ideslave.into_inner().quit().await?;
    } else {
        log::error!("Unable to drop the IDE slave. Some sockets may be left alive");
    }

    Ok(())
}

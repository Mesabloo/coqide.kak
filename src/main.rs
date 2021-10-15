#![feature(box_patterns)]
#![feature(try_trait_v2)]
#![feature(async_closure)]

use std::{env, io, path::Path, rc::Rc, sync::Arc};

use signal_hook::{
    consts::{SIGINT, SIGUSR1},
    iterator::Signals,
};
use tokio::fs::File;

use crate::{coqtop::{slave::IdeSlave, xml_protocol::types::{ProtocolCall, ProtocolValue}}, daemon::DaemonState, kakoune::{command_line::kak, commands::processor::CommandProcessor, session::SessionWrapper, slave::{KakSlave, command_file}}, result::{goal::goal_file, result::result_file}};



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
    let ideslave = Rc::new(IdeSlave::new(session.clone(), coq_file.clone()).await?);
    // Initialise the daemon state
    log::debug!("Creating daemon state");
    let mut state = DaemonState::default();
    // Initialise the command processor
    log::debug!("Starting command processing");
    let mut processor = CommandProcessor::new(session.clone(), ideslave.clone())?;
    // Initialise the Kakoune slave
    log::debug!("Initialising Kakoune slave");
    let mut kakslave = KakSlave::new(session.clone(), ideslave.clone(), &mut state)?;

    let mut signals = Signals::new(&[SIGINT, SIGUSR1])?;

    log::debug!("Creating buffers in Kakoune");
    kak(session.clone(), format!(
                r#"evaluate-commands -buffer '{0}' %{{ edit! -readonly -fifo "{1}" "%opt{{coqide_result_buffer}}" }}
                evaluate-commands -buffer '{0}' %{{ edit! -readonly -fifo "{2}" "%opt{{coqide_goal_buffer}}" }}
                evaluate-commands -buffer '{0}' %{{ coqide-send-to-process 'init' }}"#,
                coq_file, result_file(session.clone()), goal_file(session.clone()),
            )).await?;

    for signal in signals.forever() {
        log::debug!("Received signal {}", signal);
      
        if signal == SIGUSR1 {
            processor.process_next_command(&mut kakslave).await?;
        } else if signal == SIGINT {
            break;
        }
    }

    drop(kakslave);
    drop(processor);
    drop(state);

    if let Ok(ideslave) = Rc::try_unwrap(ideslave) {
        ideslave.quit().await?;
    } else {
        log::error!("Unable to drop the IDE slave");
    }

    Ok(())
}

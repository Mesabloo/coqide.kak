#![feature(derive_default_enum)]
#![feature(box_patterns)]
#![feature(path_try_exists)]

use crate::coqtop::slave::IdeSlave;
use kak_protocol::processor::CommandProcessor;
use signal_hook::{
    consts::{SIGINT, SIGUSR1},
    iterator::Signals,
};
use std::{env, io, path::Path};
use unix_named_pipe as fifos;

mod coqtop;
mod kak_protocol;
mod xml_protocol;

#[async_std::main]
async fn main() -> io::Result<()> {
    let cli_args = env::args().collect::<Vec<_>>();

    if cli_args.len() != 4 {
        panic!("coqide-kak requires three positional arguments in this order: <KAK_SESSION> <KAK_BUFFER> <KAK_COMMAND_FILE>.");
    }

    let kak_session = cli_args[1].clone();
    let kak_buffer = cli_args[2].clone();
    let kak_pipe_dirs = cli_args[3].clone();

    env_logger::builder()
        .format_level(true)
        .format_timestamp_millis()
        .format_indent(Some(4))
        // .target(env_logger::Target::Pipe(Box::new(File::open(format!(
        //     "{}/log",
        //     &kak_commands
        // ))?)))
        .init();

    log::debug!("Creating FIFO pipes to interact with Kakoune");

    let goal_path = format!("{}/goal", &kak_pipe_dirs);
    let result_path = format!("{}/result", &kak_pipe_dirs);
    let log_path = format!("{}/log", &kak_pipe_dirs);
    let cmd_path = format!("{}/cmd", &kak_pipe_dirs);

    for path in [goal_path, result_path, log_path, cmd_path] {
        if !Path::new(&path).exists() {
            fifos::create(path, None)?;
        }
    }

    let slave = IdeSlave::init(kak_buffer).await?;
    let mut kak_processor = CommandProcessor::init(kak_pipe_dirs, kak_session, slave).await?;

    log::debug!("Waiting for signals...");
    let mut signals = Signals::new(&[SIGUSR1, SIGINT])?;
    for sig in signals.forever() {
        // TODO: process SIGUSR1 as "received a message from kakoune", in buffer `cli_args[2]`
        //
        // - Read one line from `cli_args[2]`
        // - Try parse into a `KakCommand`
        // - If command, execute on `slave` and `kak_session`
        // - Keep waiting
        log::debug!("Received signal {:?}", sig);

        if sig == SIGINT {
            kak_processor.kill_session().await?;

            break;
        } else if sig == SIGUSR1 {
            kak_processor.process_command().await?.execute().await?;
        }
    }

    std::process::exit(exitcode::OK);
}

/*
   let bytes = b"<call val=\"Init\"><option val=\"none\"/></call>";
   let mut buf = [0; 256];

   let write = async {
       main_w
           .write_all(Init(Optional(Box::new(None))).encode().as_bytes())
           .await?;
       println!(
           "{} <~ `{}`",
           main_w.peer_addr()?,
           std::str::from_utf8(bytes).unwrap()
       );
       Ok::<(), io::Error>(())
   };
   let read = async {
       main_r.read(&mut buf).await?;

       let i = buf.partition_point(|x| *x != 0);

       let val = ProtocolResult::decode_stream(&buf[0..i]);
       println!(
           "{} ~> `{}` ~> {:?}",
           main_r.peer_addr()?,
           std::str::from_utf8(&buf[0..i]).unwrap(),
           val
       );
       Ok::<(), io::Error>(())
   };

   let (w_res, r_res) = join!(write, read);
   w_res?;
   r_res?;
*/

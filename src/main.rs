#![feature(derive_default_enum)]
#![feature(box_patterns)]

use crate::coqtop::slave::{IdeSlave, SlaveState};
use async_net::{TcpListener, TcpStream};
use futures::join;
use signal_hook::{consts::SIGUSR1, iterator::Signals};
use std::{env, io};

mod coqtop;
mod kak_protocol;
mod xml_protocol;

#[async_std::main]
async fn main() -> io::Result<()> {
    let cli_args = env::args().collect::<Vec<_>>();

    if cli_args.len() != 3 {
        eprintln!("coqide-kak requires two positional arguments in this order: <KAK_SESSION> <KAK_COMMAND_BUFFER>.");
        std::process::exit(exitcode::USAGE);
    }

    let main_r = new_server("127.0.0.1:55000");
    let main_w = new_server("127.0.0.1:55001");
    let control_r = new_server("127.0.0.1:55002");
    let control_w = new_server("127.0.0.1:55003");
    let proc = coqtop::spawn(&[55000, 55001, 55002, 55003]);

    let (main_r, main_w, control_r, control_w, proc) =
        join!(main_r, main_w, control_r, control_w, proc);
    let (main_r, main_w, control_r, control_w, proc) =
        (main_r?, main_w?, control_r?, control_w?, proc?);

    let mut slave = IdeSlave::new(
        main_r,
        main_w,
        control_r,
        control_w,
        proc,
        SlaveState::Connected,
    );

    let kak_session = cli_args[1].clone();
    let kak_commands = cli_args[2].clone();

    let mut signals = Signals::new(&[SIGUSR1])?;
    for sig in signals.forever() {
        // TODO: process SIGUSR1 as "received a message from kakoune", in buffer `cli_args[2]`
        //
        // - Read one line from `cli_args[2]`
        // - Try parse into a `KakCommand`
        // - If command, execute on `slave` and `kak_session`
        // - Keep waiting
        println!("Received {:?}", sig);
    }

    Ok(())
}

/// Create a new TCP server and return its socket
async fn new_server(addr: &str) -> io::Result<Box<TcpStream>> {
    let listener = TcpListener::bind(addr).await?;
    let (socket, addr) = listener.accept().await?;

    eprintln!("Connected to {}", addr);

    Ok(Box::new(socket))
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

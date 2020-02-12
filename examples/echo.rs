#[macro_use]
extern crate slog;

use std::io;

use async_std::net::TcpStream;
use async_std::task;
use futures::io::AsyncReadExt as _;
use futures::io::AsyncWriteExt as _;
use sloggers::types::Severity;
use sloggers::Build;

use async_serve::Logger;

#[derive(Clone)]
struct State {
    logger: Logger,
}

fn main() {
    let logger = sloggers::terminal::TerminalLoggerBuilder::new()
        .level(Severity::Info)
        .build()
        .expect("static config");

    let ctrl_c = async_ctrlc::CtrlC::new().expect("ctrl-c");

    let state = State { logger };

    task::block_on(async_serve::run(
        state.logger.clone(),
        "127.0.0.1:1337",
        state,
        ctrl_c,
        |conn, State { logger }| client_loop(logger, conn),
    ))
    .expect("success")
}

async fn client_loop(logger: Logger, mut conn: TcpStream) -> io::Result<()> {
    let logger = logger.new(o!("peer" => conn.peer_addr()?));

    let mut buf = [0u8; 4096];
    while let Ok(found) = conn.read(&mut buf).await {
        let buf = &buf[..found];
        if buf.is_empty() {
            info!(logger, "eof");
            break;
        }
        if let Err(e) = conn.write_all(buf).await {
            warn!(logger, "write failure"; "err" => format!("{:?}", e));
            break;
        }
        debug!(logger, "wrote"; "bytes" => buf.len());
    }

    info!(logger, "client done");

    Ok(())
}

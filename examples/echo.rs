#[macro_use]
extern crate slog;

use async_std::net::TcpStream;
use async_std::task;
use futures::io::AsyncReadExt as _;
use futures::io::AsyncWriteExt as _;
use sloggers::types::Severity;
use sloggers::Build;

use async_serve::forever;
use async_serve::Logger;

fn main() {
    let logger = sloggers::terminal::TerminalLoggerBuilder::new()
        .level(Severity::Info)
        .build()
        .expect("static config");

    task::block_on(async_serve::run(
        logger,
        "127.0.0.1:1337",
        (),
        forever(),
        |conn, (), logger| client_loop(logger, conn),
    ))
}

async fn client_loop(logger: Logger, mut conn: TcpStream) {
    let mut buf = [0u8; 4096];
    while let Ok(found) = conn.read(&mut buf).await {
        let buf = &buf[..found];
        if buf.is_empty() {
            info!(logger, "{:?}: eof", conn.peer_addr());
            break;
        }
        if let Err(e) = conn.write_all(buf).await {
            warn!(logger, "{:?} couldn't write: {:?}", conn.peer_addr(), e);
            break;
        }
        debug!(logger, "{:?} wrote {}", conn.peer_addr(), buf.len());
    }
    info!(logger, "{:?} dropping client", conn.peer_addr());
}

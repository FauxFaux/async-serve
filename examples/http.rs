#[macro_use]
extern crate slog;

use std::io;
use std::time::Duration;

use aiowrap::DequeReader;
use async_std::net::TcpStream;
use async_std::task;
use futures::io::AsyncReadExt as _;
use futures::io::AsyncWriteExt as _;
use futures::AsyncRead;
use httparse::Status;
use slog::Logger;
use sloggers::types::Severity;
use sloggers::Build;

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
        &["127.0.0.1:1337"],
        state,
        ctrl_c,
        |conn, State { logger }| async move {
            let logger = logger.new(o!("peer" => conn.peer_addr()?));
            if let Err(e) = dispatch(logger.clone(), conn).await {
                error!(logger, "request handling failed"; "err" => format!("{:?}", e));
            }
            Ok(())
        },
    ))
    .expect("success")
}

async fn dispatch(logger: Logger, conn: TcpStream) -> io::Result<()> {
    let (read, mut write) = conn.split();
    let mut read = DequeReader::new(read);
    let url = read_request(&logger, &mut read).await?;
    let logger = logger.new(o!("url" => url.to_string()));

    match url.as_ref() {
        "/hello" => {
            write.write_all(&response_header(200)).await?;
            write.write_all(b"hello, world!").await?;
        }

        "/tick" => {
            write.write_all(&response_header(200)).await?;

            loop {
                write.write_all(b"tick!\n").await?;
                write.flush().await?;
                task::sleep(Duration::from_secs(1)).await;
            }
        }

        _ => {
            info!(logger, "not found");
            write.write_all(&response_header(404)).await?;
        }
    }

    Ok(())
}

fn response_header(code: u16) -> Vec<u8> {
    format!("HTTP/1.0 {} Watevs\r\nConnection: close\r\n\r\n", code).into_bytes()
}

async fn read_request<R>(logger: &Logger, from: &mut DequeReader<R>) -> io::Result<String>
where
    R: Unpin + AsyncRead,
{
    while from.read_more().await? {
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(from.buffer()) {
            // BORROW CHECKER complains if you try and return a ref here, I think it's wrong.
            Ok(Status::Complete(_)) => return Ok(req.path.unwrap().to_string()),
            Ok(Status::Partial) => {
                if let Some(path) = req.path {
                    return Ok(path.to_string());
                }
            }
            Err(e) => {
                info!(logger, "bad request"; "err" => format!("{:?}", e));
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
    }

    Err(io::ErrorKind::UnexpectedEof.into())
}

#[macro_use]
extern crate slog;

use async_std::net::TcpListener;
use async_std::net::TcpStream;
use async_std::net::ToSocketAddrs;
use async_std::task;
use futures::io::AsyncReadExt as _;
use futures::io::AsyncWriteExt as _;
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt as _;
use futures::Future;
use futures::SinkExt;

pub use slog::Logger;

pub async fn run<A, X, S, H, R>(logger: Logger, addr: A, state: S, term: X, mut handler: H)
where
    A: ToSocketAddrs,
    S: Send + Clone,
    H: FnMut(TcpStream, S, Logger) -> R,
    X: Future,
    R: Future<Output = ()>,
{
    let serv = TcpListener::bind(addr).await.expect("bind");
    info!(logger, "listening on localhost:1773");

    let mut incoming = serv.incoming().fuse();
    let (term_send, mut term_recv) = futures::channel::mpsc::channel(1);
    ctrlc::set_handler(move || {
        task::block_on(term_send.clone().send(())).expect("messaging about shutdown can't fail")
    })
    .expect("adding termination");

    let mut workers = FuturesUnordered::new();

    loop {
        let client = futures::select! {
            _ = term_recv.next() => {
                info!(logger, "woken by cancellation");
                break;
            },
            client = incoming.next() => client,
            garbage = workers.next() => {
                // unfortunately, we see the None here
                if let Some(()) = garbage {
                    debug!(logger, "garbage collected a client");
                }
                continue;
            },
        };

        // I think this might be irrefutable.
        let client = match client {
            Some(client) => client,
            None => break,
        };

        let client = client.expect("accept");
        info!(logger, "accepted client from {:?}", client.peer_addr());
        let state = state.clone();
        let handle = handler(client, state, logger.clone());
        workers.push(handle);
    }

    drop(serv);
    info!(logger, "listener shut down, draining clients");

    loop {
        futures::select! {
            worker = workers.next() => {
                info!(logger, "{} clients still connected", workers.len());
                if worker.is_none() {
                    info!(logger, "all clients disconnected");
                    break;
                }
            },
            _ = term_recv.next() => break,
            complete => break,
        }
    }

    info!(logger, "bye");
}

pub fn forever() -> impl Future<Output = ()> {
    futures::future::pending()
}

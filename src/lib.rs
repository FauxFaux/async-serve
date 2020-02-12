#[macro_use]
extern crate slog;

use std::io;

use async_std::net::TcpListener;
use async_std::net::TcpStream;
use async_std::net::ToSocketAddrs;
use async_std::task;
use futures::future::FusedFuture;
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt as _;
use futures::Future;
use futures::FutureExt as _;

pub use slog::Logger;

pub async fn run<A, X, S, H, R>(
    logger: Logger,
    addr: A,
    state: S,
    term: X,
    mut handler: H,
) -> io::Result<()>
where
    A: ToSocketAddrs,
    S: Send + Clone,
    H: FnMut(TcpStream, S) -> R,
    X: Unpin + Future,
    R: 'static + Send + Future<Output = io::Result<()>>,
{
    let serv = TcpListener::bind(addr).await?;
    info!(logger, "listening"; "addr" => serv.local_addr()?);

    let mut incoming = serv.incoming().fuse();
    let mut term = term.fuse();

    let mut workers = FuturesUnordered::new();

    loop {
        let client = futures::select! {
            _ = term => break,
            client = incoming.next() => client,
            garbage = workers.next() => {
                // unfortunately, we see the None here
                if let Some(client) = garbage {
                    client?;
                    debug!(logger, "client released"; "clients" => workers.len());
                }
                continue;
            },
        };

        // I think this might be irrefutable.
        let client = match client {
            Some(client) => client,
            None => break,
        };

        let client = client?;
        info!(logger, "accepted client"; "peer_addr" => client.peer_addr()?);
        let state = state.clone();
        let handle = task::spawn(handler(client, state));
        workers.push(handle);
    }

    drop(serv);
    info!(logger, "listener(s) closed, draining clients"; "clients" => workers.len());

    while let Some(worker) = workers.next().await {
        worker?;
        info!(logger, "client complete"; "clients" => workers.len());
    }

    Ok(())
}

pub fn forever() -> impl FusedFuture<Output = ()> {
    futures::future::pending()
}

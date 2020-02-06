use std::pin::Pin;

use async_std::net::TcpListener;
use async_std::net::TcpStream;
use async_std::stream::StreamExt as _;
use async_std::task;
use futures::future::FusedFuture;
use futures::io::AsyncReadExt as _;
use futures::io::AsyncWriteExt as _;
use futures::stream::FuturesUnordered;
use futures::stream::Stream as _;
use futures::task::Context;
use futures::task::Poll;
use futures::Future;
use futures::FutureExt;

fn main() {
    pretty_env_logger::init();
    task::block_on(run());
    log::info!("app exiting");
}

async fn run() {
    let serv = TcpListener::bind("localhost:1773").await.expect("bind");
    log::info!("listening on localhost:1773");

    let mut incoming = serv.incoming().fuse();
    let mut stopper = async_ctrlc::CtrlC::new().expect("init").fuse();

    let mut workers = Unordered {
        inner: FuturesUnordered::new(),
    };

    loop {
        let client = futures::select! {
            _ = stopper => {
                log::info!("woken by cancellation");
                break;
            },
            client = incoming.next().fuse() => client,
            _ = workers => continue,
        };

        // I think this might be irrefutable.
        let client = match client {
            Some(client) => client,
            None => break,
        };

        let client = client.expect("accept");
        log::info!("accepted client from {:?}", client.peer_addr());
        let handle = client_loop(client);
        workers.inner.push(handle);
    }

    drop(serv);
    log::info!("listener shut down, draining clients");

    while let Some(_result) = workers.inner.next().await {
        log::info!("drained worker");
    }

    log::info!("listener shut down, bye");
}

struct Unordered<T, Fut: Future<Output = T>> {
    inner: FuturesUnordered<Fut>,
}

impl<T, Fut: Future<Output = T>> Future for Unordered<T, Fut> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.inner.is_empty() {
            return Poll::Pending;
        }

        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(x)) => Poll::Ready(x),
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => unreachable!("checked above"),
        }
    }
}

impl<T, Fut: Future<Output = T>> FusedFuture for Unordered<T, Fut> {
    fn is_terminated(&self) -> bool {
        false
    }
}

async fn client_loop(mut conn: TcpStream) {
    let mut buf = [0u8; 4096];
    while let Ok(found) = conn.read(&mut buf).await {
        let buf = &buf[..found];
        if buf.is_empty() {
            log::info!("{:?}: eof", conn.peer_addr());
            break;
        }
        if let Err(e) = conn.write_all(buf).await {
            log::warn!("{:?} couldn't write: {:?}", conn.peer_addr(), e);
            break;
        }
        log::debug!("{:?} wrote {}", conn.peer_addr(), buf.len());
    }
    log::info!("{:?} dropping client", conn.peer_addr());
}

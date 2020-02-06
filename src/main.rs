use async_std::net::TcpListener;
use async_std::net::TcpStream;
use async_std::stream::StreamExt as _;
use async_std::task;
use futures::io::AsyncReadExt as _;
use futures::io::AsyncWriteExt as _;
use futures::FutureExt;

fn main() {
    pretty_env_logger::init();
    task::block_on(run());
    log::info!("app exiting");
}

async fn run() {
    let serv = TcpListener::bind("localhost:1773").await.expect("bind");
    log::info!("listening on localhost:1773");

    let mut incoming = serv.incoming();
    let mut stopper = async_ctrlc::CtrlC::new().expect("init").fuse();

    let mut workers = Vec::new();

    loop {
        let client = futures::select! {
            _ = stopper => {
                log::info!("woken by cancellation");
                break;
            },
            client = incoming.next().fuse() => client,
        };

        // I think this might be irrefutable.
        let client = match client {
            Some(client) => client,
            None => break,
        };

        let client = client.expect("accept");
        log::info!("accepted client from {:?}", client.peer_addr());
        let handle = task::spawn(client_loop(client));
        workers.push(handle);
    }

    drop(serv);
    log::info!("listener shut down, draining clients");

    for (id, worker) in workers.into_iter().enumerate() {
        log::info!("draining client {}", id);
        worker.await;
    }

    log::info!("listener shut down, bye");
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

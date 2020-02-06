use async_std::net::TcpListener;
use async_std::net::TcpStream;
use async_std::stream::StreamExt as _;
use async_std::task;
use futures::io::AsyncReadExt as _;
use futures::io::AsyncWriteExt as _;

fn main() {
    pretty_env_logger::init();
    task::block_on(run())
}

async fn run() {
    let serv = TcpListener::bind("localhost:1773").await.expect("bind");
    log::info!("listening on localhost:1773");

    let mut serv = serv.incoming();

    loop {
        let client = serv.next().await.expect("accept").expect("huh");
        log::info!("accepted client from {:?}", client.peer_addr());
        task::spawn(client_loop(client));
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

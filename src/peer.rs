use tokio::net::TcpStream;
use tokio::time::{self, Duration};

use std::error::Error;
use std::io;
use std::net::Ipv4Addr;
use std::sync::Arc;

struct Peer {
    am_choking: bool,
    am_interested: bool,
    peer_choking: bool,
    peer_interested: bool,
    stream: Arc<TcpStream>,
}

// According to https://wiki.theory.org/index.php/BitTorrentSpecification#keep-alive:_.3Clen.3D0000.3E
// the keepalive is typically 2 minutes long.
async fn keepalive(stream: &Arc<TcpStream>) {
    let mut interval = time::interval(Duration::from_secs(110));
    const PAYLOAD: [u8; 4] = [0; 4];
    // wait away the first tick which is immediate
    interval.tick().await;

    loop {
        interval.tick().await;

        loop {
            stream.writable().await.unwrap();

            match stream.try_write(&PAYLOAD) {
                Ok(n) => {
                    assert!(n == PAYLOAD.len());
                    break;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    // Maybe the socket closed
                    return;
                }
            }
        }
    }
}

impl Peer {
    pub async fn new(ip: Ipv4Addr, port: u16) -> Result<Self, Box<dyn Error>> {
        let res = Peer {
            am_choking: true,
            am_interested: false,
            peer_choking: true,
            peer_interested: false,
            stream: Arc::new(TcpStream::connect(format!("{:?}:{}", ip, port)).await?),
        };

        let stream = res.stream.clone();

        tokio::spawn(async move { keepalive(&stream).await });

        Ok(res)
    }
}

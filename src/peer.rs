use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::{self, Duration};

use std::error::Error;
use std::io;
use std::net::Ipv4Addr;
use std::sync::Arc;

use crate::handshake::Handshake;

pub struct Peer {
    am_choking: bool,
    am_interested: bool,
    peer_choking: bool,
    peer_interested: bool,
    stream: TcpStream,
}

// According to https://wiki.theory.org/index.php/BitTorrentSpecification#keep-alive:_.3Clen.3D0000.3E
// the keepalive is typically 2 minutes long.
async fn keepalive(peer: &Arc<RwLock<Peer>>) {
    let mut interval = time::interval(Duration::from_secs(110));
    const PAYLOAD: [u8; 4] = [0; 4];
    // wait away the first tick which is immediate
    interval.tick().await;

    loop {
        interval.tick().await;

        loop {
            {
                peer.read().await.stream.writable().await;
            }
            let tw_res;
            {
                let pr = peer.write().await;
                tw_res = pr.stream.try_write(&PAYLOAD);
            }

            match tw_res {
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

async fn listen_and_dispatch(peer: &Arc<RwLock<Peer>>) {
    loop {
        peer.read().await.stream.readable().await.unwrap();
        let size = peer.write().await.stream.read_u32().await;

        if let Err(e) = size {
            return;
        }
        let size = size.unwrap();

        if size == 0 {
            // Keep-alive
            continue;
        }

        let mut buffer = vec![];
        buffer.resize(size as usize, 0u8);

        peer.write()
            .await
            .stream
            .read_exact(&mut buffer)
            .await
            .unwrap();

        match buffer[0] {
            0 => choke(&peer).await,
            1 => unchoke(&peer).await,
            2 => interested(&peer).await,
            3 => not_interested(&peer).await,
            4 => have(&peer, &buffer).await,
            5 => bitfield(&peer, &buffer).await,
            n => panic!("Not implemented: {}", n),
        };

        println!("size: {}", size);
        println!("payload: {:x?}", buffer);
    }
}

async fn choke(peer: &Arc<RwLock<Peer>>) {
    unimplemented!("choke");
}

async fn unchoke(peer: &Arc<RwLock<Peer>>) {
    unimplemented!("unchoke");
}

async fn interested(peer: &Arc<RwLock<Peer>>) {
    unimplemented!("interested");
}

async fn not_interested(peer: &Arc<RwLock<Peer>>) {
    unimplemented!("not_interested");
}

async fn have(peer: &Arc<RwLock<Peer>>, buffer: &[u8]) {
    unimplemented!("have");
}

async fn bitfield(peer: &Arc<RwLock<Peer>>, buffer: &[u8]) {
    unimplemented!("bitfield");
}

impl Peer {
    pub async fn new(ip: Ipv4Addr, port: u16) -> Result<Arc<RwLock<Self>>, Box<dyn Error>> {
        let res = Arc::new(RwLock::new(Peer {
            am_choking: true,
            am_interested: false,
            peer_choking: true,
            peer_interested: false,
            stream: TcpStream::connect(format!("{:?}:{}", ip, port)).await?,
        }));

        let alive = res.clone();
        tokio::spawn(async move { keepalive(&alive).await });

        let listen = res.clone();
        tokio::spawn(async move { listen_and_dispatch(&listen).await });

        Ok(res)
    }

    pub fn get_stream(&self) -> &TcpStream {
        &self.stream
    }

    pub fn get_stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }
}

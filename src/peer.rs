use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::{self, Duration};

use std::error::Error;
use std::io;
use std::net::Ipv4Addr;
use std::sync::Arc;

use crate::handshake::Handshake;

// TODO: Add a list of shared files with peer
pub struct Peer {
    am_choking: bool,
    am_interested: bool,
    peer_choking: bool,
    peer_interested: bool,
    stream: TcpStream,
    have: Vec<bool>,
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
            peer.read().await.stream.writable().await;
            let tw_res = peer.write().await.stream.try_write(&PAYLOAD);

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
        let mut size = [0u8; 4];
        let resp = peer.write().await.stream.try_read(&mut size);

        if let Err(e) = resp {
            if e.kind() == io::ErrorKind::WouldBlock {
                // Doesn't please me, should find a way to read only when data is available
                time::sleep(time::Duration::from_millis(100));
                continue;
            } else {
                return;
            }
        }
        let size = u32::from_be_bytes(size);

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
            4 => have(&peer, &buffer[1..]).await,
            5 => bitfield(&peer, &buffer[1..]).await,
            6 => request(&peer, &buffer[1..]).await,
            7 => piece(&peer, &buffer[1..]).await,
            8 => cancel(&peer, &buffer[1..]).await,
            n => panic!("Not implemented: {}", n),
        };
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
    peer.write().await.have[u32::from_be_bytes(buffer.try_into().unwrap()) as usize] = true;
}

async fn bitfield(peer: &Arc<RwLock<Peer>>, buffer: &[u8]) {
    assert!(peer.read().await.have.len() <= buffer.len() * 8);
    let mut idx = 0;
    let len = peer.read().await.have.len();

    while idx + 8 < len {
        // lock the struct at the beginning of each byte
        let x = buffer[idx / 8];
        let mut peer = peer.write().await;

        peer.have[idx + 0] = x & (1 << 7) != 0;
        peer.have[idx + 1] = x & (1 << 6) != 0;
        peer.have[idx + 2] = x & (1 << 5) != 0;
        peer.have[idx + 3] = x & (1 << 4) != 0;
        peer.have[idx + 4] = x & (1 << 3) != 0;
        peer.have[idx + 5] = x & (1 << 2) != 0;
        peer.have[idx + 6] = x & (1 << 1) != 0;
        peer.have[idx + 7] = x & (1 << 0) != 0;

        idx += 8;
    }

    // Handle remaining bits
    let mut peer = peer.write().await;
    let mut shift = 7;
    while idx < len {
        peer.have[idx] = buffer[buffer.len() - 1] & (1 << shift) != 0;
        idx += 1;
        shift -= 1;
    }
}

async fn request(peer: &Arc<RwLock<Peer>>, buffer: &[u8]) {
    unimplemented!("request");
}

async fn piece(peer: &Arc<RwLock<Peer>>, buffer: &[u8]) {
    unimplemented!("piece");
}

async fn cancel(peer: &Arc<RwLock<Peer>>, buffer: &[u8]) {
    unimplemented!("cancel");
}

impl Peer {
    pub async fn new(
        ip: Ipv4Addr,
        port: u16,
        pieces: usize,
    ) -> Result<Arc<RwLock<Self>>, Box<dyn Error>> {
        let res = Arc::new(RwLock::new(Peer {
            am_choking: true,
            am_interested: false,
            peer_choking: true,
            peer_interested: false,
            stream: TcpStream::connect(format!("{:?}:{}", ip, port)).await?,
            have: vec![false; pieces],
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

    pub fn get_bitfield(&self) -> &Vec<bool> {
        &self.have
    }
}

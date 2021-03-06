use std::mem;
use std::{io, net::Ipv4Addr};
use tokio::net::UdpSocket;

use crate::definitions::{InfoHash, PeerId, INFO_HASH_LEN, TORRENT_RS_PEER_ID};

pub type ConnectionId = u64;

pub type TransactionId = u32;

const SOCKET_BIND: &str = "0.0.0.0:8080";

#[derive(Debug)]
pub struct UdpConnection {
    socket: UdpSocket,
    cid: ConnectionId,
    tid: TransactionId,
}

#[repr(C, align(4))]
#[derive(Debug)]
struct ConnectIn {
    cid: ConnectionId,
    action: u32,
    tid: TransactionId,
}

#[repr(C, align(4))]
#[derive(Debug)]
struct ConnectOut {
    action: u32,
    tid: TransactionId,
    cid: ConnectionId,
}

#[repr(packed)]
#[derive(Debug, Copy, Clone)]
struct AnnounceIn {
    cid: ConnectionId,
    action: u32,
    tid: TransactionId,
    info_hash: InfoHash,
    peer_id: PeerId,
    downloaded: u64,
    left: u64,
    uploaded: u64,
    event: u32,
    ipv4: u32,
    key: u32,
    num_want: u32, // number of wanted peers
    port: u16,
}

#[repr(packed)]
#[derive(Debug)]
pub struct AnnounceOut {
    action: u32,
    tid: TransactionId,
    interval: u32,
    leechers: u32,
    seeders: u32,
    peers: Option<Vec<(Ipv4Addr, u16)>>,
}

// TODO: return Result
pub fn hash_to_bytes(hash: &str) -> InfoHash {
    let mut res = [0u8; INFO_HASH_LEN];

    // TODO: look for another way to split the str
    hash.as_bytes()
        .chunks(2)
        .map(|b| std::str::from_utf8(b).unwrap())
        .map(|n| u8::from_str_radix(n, 16).unwrap())
        .enumerate()
        .for_each(|(i, x)| res[i] = x);

    res
}

impl UdpConnection {
    pub async fn new(tracker: &str, id: Option<TransactionId>) -> io::Result<Self> {
        let sock = UdpSocket::bind(SOCKET_BIND).await?;
        sock.connect(tracker).await?;
        let tid = id.unwrap_or_default();

        Ok(UdpConnection {
            socket: sock,
            cid: ConnectionId::default(),
            tid,
        })
    }

    pub async fn connect(&mut self) -> io::Result<()> {
        let tid = rand::random();
        let cin = ConnectIn {
            cid: 0x8019102717040000,
            action: 0,
            tid,
        };

        let data_in: [u8; mem::size_of::<ConnectIn>()] = unsafe { mem::transmute(cin) };
        let mut data_out = [0u8; mem::size_of::<ConnectOut>()];

        self.socket.send(&data_in).await?;
        self.socket.recv(&mut data_out).await?;

        let cout: ConnectOut = unsafe { mem::transmute(data_out) };

        // TODO: fail gracefully
        assert!(cout.action == 0);
        assert!(cout.tid == tid);
        assert!(cout.cid != 0);

        self.tid = tid;
        self.cid = cout.cid;

        Ok(())
    }

    pub async fn announce(
        &self,
        info_hash: &str,
        peer_id: Option<&PeerId>,
        num_peers: Option<u32>,
    ) -> io::Result<AnnounceOut> {
        let pid = peer_id.unwrap_or(TORRENT_RS_PEER_ID);
        let num_peers = num_peers.unwrap_or(1);

        let ann = AnnounceIn {
            cid: self.cid,
            action: (1_u32).to_be(),
            tid: self.tid,
            info_hash: hash_to_bytes(info_hash),
            peer_id: *pid,
            downloaded: 0,
            left: 0,
            uploaded: 0,
            event: 0,
            ipv4: 0,
            key: 0,
            num_want: num_peers.to_be(),
            port: 0,
        };

        let mut buf = vec![0u8; 20 + 6 * num_peers as usize];
        let data: [u8; std::mem::size_of::<AnnounceIn>()] = unsafe { mem::transmute(ann) };
        self.socket.send(&data).await?;
        self.socket.recv(&mut buf).await?;

        let res = AnnounceOut {
            action: u32::from_be_bytes(buf[0..4].try_into().unwrap()),
            tid: u32::from_ne_bytes(buf[4..8].try_into().unwrap()),
            interval: u32::from_be_bytes(buf[8..12].try_into().unwrap()),
            leechers: u32::from_be_bytes(buf[12..16].try_into().unwrap()),
            seeders: u32::from_be_bytes(buf[16..20].try_into().unwrap()),
            peers: match num_peers {
                0 => None,
                n => Some(
                    (0..n as usize)
                        .map(|x| {
                            let idx = 20 + 6 * x;
                            (
                                Ipv4Addr::new(buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3]),
                                u16::from_be_bytes(buf[24 + x * 6..26 + x * 6].try_into().unwrap()),
                            )
                        })
                        .filter(|ipport| *ipport != (Ipv4Addr::new(0, 0, 0, 0), 0))
                        .collect(),
                ),
            },
        };

        Ok(res)
    }
}

impl AnnounceOut {
    pub fn get_peers(&self) -> Option<&Vec<(Ipv4Addr, u16)>> {
        self.peers.as_ref()
    }
}

#[cfg(test)]
mod tracker_tests {
    use super::*;
    use serial_test::serial;

    const TRACKER: &str = "192.168.0.101:3000";

    #[tokio::test]
    #[serial]
    async fn test_connect_empty_id() {
        let udpc = UdpConnection::new(TRACKER, None).await;
        if let Err(ref e) = udpc {
            println!("Error: {}", e);
            panic!();
        }

        let mut udpc = udpc.unwrap();
        let res = udpc.connect().await;
        if let Err(ref e) = res {
            println!("Error: {}", e);
            panic!();
        }

        assert_ne!(udpc.cid, 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_announce_empty_peer() {
        let mut udpc = UdpConnection::new(TRACKER, None).await.unwrap();

        udpc.connect().await.unwrap();
        let ann = udpc
            .announce("52b62d34a8336f2e934df62181ad4c2f1b43c185", None, None)
            .await
            .unwrap();

        assert_eq!(1, ann.action);
        assert_eq!(udpc.tid, ann.tid);
        // Shouldn't be true for every case
        assert_ne!(None, ann.peers);
    }
}

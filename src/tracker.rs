use rand::prelude::*;
use std::mem;
use std::{io, net::SocketAddr};
use tokio::net::UdpSocket;

use crate::definitions::{InfoHash, PeerId, INFO_HASH_LEN, TORRENT_RS_PEER_ID};

pub type ConnectionId = u64;

pub type TransactionId = u32;

const SOCKET_BIND: &str = "0.0.0.0:8080";

// // Generate a random TransactionId
// // Could be rewritten with a u32 and bitmasking
// fn generate_transaction_id() -> TransactionId {
//     [
//         rand::random(),
//         rand::random(),
//         rand::random(),
//         rand::random(),
//     ]
// }

#[derive(Debug)]
struct UdpConnection {
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
#[derive(Debug)]
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
struct AnnounceOut {
    action: u32,
    tid: TransactionId,
    interval: u32,
    leechers: u32,
    seeders: u32,
    peers: Option<Vec<(u32, u16)>>,
}

// TODO: return Result
fn hash_to_bytes(hash: &str) -> InfoHash {
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

        Ok(UdpConnection {
            socket: sock,
            cid: ConnectionId::default(),
            tid: TransactionId::default(),
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
    ) -> io::Result<AnnounceOut> {
        let pid = peer_id.unwrap_or(TORRENT_RS_PEER_ID);
        let ann = AnnounceIn {
            cid: self.cid,
            action: (1 as u32).to_be(),
            tid: self.tid,
            info_hash: hash_to_bytes(info_hash),
            peer_id: *pid,
            downloaded: 0,
            left: 0,
            uploaded: 0,
            event: 0,
            ipv4: 0,
            key: 0,
            num_want: 0,
            port: 0,
        };

        // TODO: make buf's size num_want dependant
        let mut buf = [0u8; 256];
        let data: [u8; std::mem::size_of::<AnnounceIn>()] = unsafe { mem::transmute(ann) };
        self.socket.send(&data).await?;
        self.socket.recv(&mut buf).await?;

        let res = AnnounceOut {
            action: u32::from_be_bytes(buf[0..4].try_into().unwrap()),
            tid: u32::from_ne_bytes(buf[4..8].try_into().unwrap()),
            interval: u32::from_be_bytes(buf[8..12].try_into().unwrap()),
            leechers: u32::from_be_bytes(buf[12..16].try_into().unwrap()),
            seeders: u32::from_be_bytes(buf[16..20].try_into().unwrap()),
            peers: None,
        };

        Ok(res)
    }
}

#[cfg(test)]
mod tracker_tests {
    use super::*;
    use crate::definitions::TORRENT_RS_PEER_ID;

    #[test]
    #[ignore]
    fn test_connect_empty_id() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let udpc = UdpConnection::new("tracker.opentrackr.org:1337", None).await;
            if let Err(ref e) = udpc {
                println!("Error: {}", e);
                assert!(false);
            }

            let mut udpc = udpc.unwrap();
            let res = udpc.connect().await;
            if let Err(ref e) = res {
                println!("Error: {}", e);
                assert!(false);
            }

            assert_ne!(udpc.cid, 0);
        });
        // Would be surprising if it was the case
    }

    #[test]
    fn test_announce_empty_peer() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut udpc = UdpConnection::new("tracker.opentrackr.org:1337", None)
                .await
                .unwrap();

            udpc.connect().await.unwrap();
            let ann = udpc
                .announce("52b62d34a8336f2e934df62181ad4c2f1b43c185", None)
                .await
                .unwrap();
            println!("Announce resp: {:X?}", ann);
        });
        assert!(false);
    }
}

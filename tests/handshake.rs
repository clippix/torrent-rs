use torrent_rs::*;

use tokio::net::TcpStream;
use tokio::time;

use serial_test::serial;

const TRACKER: &str = "192.168.37.239:3000";
const HASH: &str = "52b62d34a8336f2e934df62181ad4c2f1b43c185";

#[tokio::test]
#[serial]
async fn connect_announce_handshake() {
    let mut udpc = tracker::UdpConnection::new(TRACKER, None).await.unwrap();
    udpc.connect().await.unwrap();

    let hash_bytes: definitions::InfoHash = tracker::hash_to_bytes(HASH);

    let ann = udpc.announce(HASH, None, Some(1)).await.unwrap();

    let (addr, port) = ann.get_peers().unwrap()[0];
    let mut stream = TcpStream::connect(format!("{:?}:{}", addr, port))
        .await
        .unwrap();

    let mut hs = handshake::Handshake::default();
    hs.set_hash(&hash_bytes);

    let hs = match hs.send(&mut stream).await {
        Ok(hs) => hs,
        Err(e) => panic!("{:?}", e),
    };

    assert_eq!(hash_bytes, *hs.get_hash());
    assert_ne!(*definitions::TORRENT_RS_PEER_ID, *hs.get_peer_id());
}

#[tokio::test]
#[serial]
async fn handshake_bitfield() {
    let mut udpc = tracker::UdpConnection::new(TRACKER, None).await.unwrap();
    udpc.connect().await.unwrap();

    let ann = udpc.announce(HASH, None, Some(1)).await.unwrap();
    let (addr, port) = ann.get_peers().unwrap()[0];

    let mut hs = handshake::Handshake::default();
    hs.set_hash(&tracker::hash_to_bytes(HASH));
    let peer = peer::Peer::new(addr, port).await.unwrap();
    {
        let mut peer = peer.write().await;
        let mut stream = peer.get_stream_mut();
        hs.send(&mut stream).await.unwrap();
    }
}

use torrent_rs::*;

use tokio::net::TcpStream;

use serial_test::serial;

#[tokio::test]
#[serial]
async fn connect_announce_handshake() {
    let mut udpc = tracker::UdpConnection::new("192.168.37.239:3000", None)
        .await
        .unwrap();
    udpc.connect().await.unwrap();

    let hash: &str = "52b62d34a8336f2e934df62181ad4c2f1b43c185";
    let hash_bytes: definitions::InfoHash = tracker::hash_to_bytes(hash);

    let ann = udpc.announce(hash, None, Some(1)).await.unwrap();

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

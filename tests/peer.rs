use torrent_rs::*;

use std::{fs, sync::Arc};

use tokio::{sync::RwLock, time};

use bendy::decoding::FromBencode;

use serial_test::serial;

async fn common() -> (handshake::Handshake, Arc<RwLock<peer::Peer>>) {
    let torrent = fs::read("./tests/torrent_files/test_local.torrent").unwrap();
    let meta_info = decode_torrent::MetaInfo::from_bencode(&torrent).unwrap();
    let info_hash = decode_torrent::get_info_hash(&torrent);
    let hash = decode_torrent::bytes_to_hash(&info_hash);

    let mut udpc = tracker::UdpConnection::new(&meta_info.announce[6..], None)
        .await
        .unwrap();
    udpc.connect().await.unwrap();

    let ann = udpc.announce(&hash, None, Some(1)).await.unwrap();
    let (addr, port) = ann.get_peers().unwrap()[0];

    let mut hs = handshake::Handshake::default();
    hs.set_hash(&info_hash);
    let peer = peer::Peer::new(addr, port, meta_info.info.pieces.len())
        .await
        .unwrap();
    {
        let mut peer = peer.write().await;
        let mut stream = peer.get_stream_mut();
        hs.send(&mut stream).await.unwrap();
    }

    (hs, peer)
}

#[tokio::test]
#[serial]
async fn decode_handshake_bitfield() {
    let (_, peer) = common().await;
    // Wait for bitfield message to be sent and decoded
    time::sleep(time::Duration::from_secs(1)).await;

    let peer = peer.read().await;
    let bitfield = peer.get_bitfield();
    for &x in bitfield {
        assert_eq!(true, x);
    }
}

#[tokio::test]
#[serial]
#[ignore]
// Check the result using wireshark
async fn keepalive() {
    // console_subscriber::init();
    let (_, _) = common().await;
    time::sleep(time::Duration::from_secs(240)).await;
}

use torrent_rs::*;

use serial_test::serial;

#[tokio::test]
#[serial]
async fn it_works2() {
    let mut udpc = tracker::UdpConnection::new("tracker.opentrackr.org:1337", None).await.unwrap();
    udpc.connect().await.unwrap();

    let ann = udpc
            .announce("52b62d34a8336f2e934df62181ad4c2f1b43c185", None, None)
            .await
            .unwrap();

    let (addr, port) = ann.get_peers().unwrap()[0];
    assert!(true);
}

pub const INFO_HASH_LEN: usize = 20;
pub const PEER_ID_LEN: usize = 20;
pub const TORRENT_RS_PEER_ID: &[u8; PEER_ID_LEN] = b"-RS0001-RANDOM_CHARA";

pub type InfoHash = [u8; INFO_HASH_LEN];

pub type PeerId = [u8; PEER_ID_LEN];

pub struct Peer {}

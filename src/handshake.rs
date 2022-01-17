use crate::definitions::*;

const PSTR: &[u8; 19] = b"BitTorrent protocol";
const PSTR_LEN: usize = 19;
const RESERVED_LEN: usize = 8;
const TORRENT_RS_PEER_ID: &[u8; PEER_ID_LEN] = b"-RS0001-RANDOM_CHARA";
const HANDSHAKE_SIZE: usize = 1 + PSTR_LEN + RESERVED_LEN + INFO_HASH_LEN + PEER_ID_LEN;

#[derive(Copy, Clone, Debug, PartialEq)]
struct Handshake {
    pstr_len: u8,
    protocol: [u8; PSTR_LEN],
    reserved: [u8; RESERVED_LEN],
    info_hash: InfoHash,
    peer_id: PeerId,
}

impl Default for Handshake {
    fn default() -> Self {
        Handshake {
            pstr_len: PSTR_LEN as u8,
            protocol: *PSTR,
            reserved: [0; RESERVED_LEN],
            info_hash: [0; INFO_HASH_LEN],
            peer_id: *TORRENT_RS_PEER_ID,
        }
    }
}

impl Handshake {
    pub fn new(input: &[u8; HANDSHAKE_SIZE]) -> Self {
        // Handcoded for now
        // TODO: cleanup
        Handshake {
            pstr_len: input[0],
            protocol: input[1..20].try_into().expect("Big problem here"),
            reserved: input[20..28].try_into().expect("Big problem here"),
            info_hash: input[28..48].try_into().expect("Big problem here"),
            peer_id: input[48..].try_into().expect("Big problem here"),
        }
    }

    // TODO: look for a more idiomatic / effective method
    pub fn to_bytes(self) -> [u8; HANDSHAKE_SIZE] {
        use std::mem;
        use std::ptr;
        let mut res = [0; HANDSHAKE_SIZE];

        res[0] = self.pstr_len;
        unsafe {
            ptr::copy_nonoverlapping(self.protocol.as_ptr(), res[1..20].as_mut_ptr(), PSTR_LEN);
            ptr::copy_nonoverlapping(
                self.reserved.as_ptr(),
                res[20..28].as_mut_ptr(),
                RESERVED_LEN,
            );
            ptr::copy_nonoverlapping(
                self.info_hash.as_ptr(),
                res[28..48].as_mut_ptr(),
                INFO_HASH_LEN,
            );
            ptr::copy_nonoverlapping(self.peer_id.as_ptr(), res[48..].as_mut_ptr(), PEER_ID_LEN);
        }

        res
    }
}

fn is_header_valid(hs: &Handshake) -> bool {
    hs.pstr_len == PSTR_LEN as u8 && hs.protocol == *PSTR
}

#[cfg(test)]
mod handshake_tests {
    use super::*;

    #[test]
    fn is_header_valid_good() {
        let hs = Handshake::default();
        assert!(is_header_valid(&hs));
    }

    #[test]
    fn is_header_valid_bad() {
        let bytes = [PSTR_LEN as u8; HANDSHAKE_SIZE];
        let hs = Handshake::new(&bytes);
        assert!(!is_header_valid(&hs));
    }

    #[test]
    fn new_handshake_good() {
        let mut bytes = [0; HANDSHAKE_SIZE];

        bytes[0] = PSTR_LEN as u8;
        for (i, x) in PSTR.iter().enumerate() {
            bytes[1 + i] = *x;
        }

        let hs = Handshake::new(&bytes);

        assert!(is_header_valid(&hs));
    }

    #[test]
    fn handshake_to_bytes_to_handshake() {
        let bytes = Handshake::default().to_bytes();
        let hs = Handshake::new(&bytes);

        assert_eq!(hs, Handshake::default());
    }
}

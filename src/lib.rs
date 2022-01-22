pub mod decode_torrent;
pub mod definitions;
pub mod file;
pub mod handshake;
pub mod peer;
pub mod tracker;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}

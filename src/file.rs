use std::{
    fs::{self, File},
    io,
    io::Error,
    os::{raw::c_int, unix::fs::MetadataExt, unix::prelude::AsRawFd},
    path::Path,
    sync::Arc,
};

use crate::definitions::InfoHash;

use rio::Rio;

use sha1::{Digest, Sha1};

use tokio::sync::Mutex;

#[derive(Debug)]
struct Piece {
    piece_size: usize,
    ring: Arc<Mutex<Rio>>,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct FileEntity {
    file: File,
    ring: Arc<Mutex<Rio>>,
    piece_size: usize,
    pieces: Vec<Option<Piece>>,
}

impl Piece {
    pub fn new(piece_size: usize, actual_size: usize, ring: Arc<Mutex<Rio>>) -> Self {
        Piece {
            piece_size,
            ring,
            bytes: vec![0u8; actual_size],
        }
    }

    pub async fn read(&mut self, file: &File, offset: usize) -> io::Result<()> {
        let bytes_read = self
            .ring
            .lock()
            .await
            .read_at(file, &mut self.bytes, offset as u64)
            .await?;
        assert!(bytes_read == self.bytes.len());

        Ok(())
    }

    pub fn update(&mut self, offset: usize, data: &[u8]) {
        assert!(offset + data.len() <= self.bytes.len());
        self.bytes[offset..offset + data.len()].copy_from_slice(data);
    }

    pub async fn write(&mut self, file: &File, offset: usize) -> io::Result<()> {
        let bytes_wrote = self
            .ring
            .lock()
            .await
            .write_at(file, &self.bytes, offset as u64)
            .await?;
        assert!(bytes_wrote == self.bytes.len());

        Ok(())
    }

    pub fn hash(&self) -> InfoHash {
        let mut hasher = Sha1::new();
        hasher.update(&self.bytes);
        hasher.finalize().try_into().unwrap()
    }
}

impl FileEntity {
    pub fn new<F: AsRef<Path>>(file: F, piece_size: usize, size: usize) -> io::Result<Self> {
        let meta = fs::metadata(&file);

        let file = match meta {
            Ok(m) => {
                if m.is_file() && m.size() as usize != size {
                    return Err(Error::new(
                        io::ErrorKind::AlreadyExists,
                        "File already exist",
                    ));
                }
                fs::OpenOptions::new().read(true).write(true).open(file)?
            }
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => fallocate(file, size)?,
            Err(e) => return Err(e),
        };

        let pieces = if size % piece_size == 0 {
            size / piece_size
        } else {
            size / piece_size + 1
        };

        Ok(FileEntity {
            file,
            ring: Arc::new(Mutex::new(rio::new()?)),
            piece_size,
            pieces: std::iter::repeat_with(|| None).take(pieces).collect(),
        })
    }
}

// TODO: handle failed allocation
fn fallocate<S: AsRef<Path>>(file: S, size: usize) -> io::Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(file)?;

    let fd = file.as_raw_fd();
    let mode: c_int = 0;
    let offset: libc::off_t = 0;
    let len: libc::off_t = size as i64;
    unsafe {
        libc::fallocate(fd, mode, offset, len);
    }

    Ok(file)
}

#[cfg(test)]
mod file_tests {
    use super::*;

    #[test]
    fn allocate_file() {
        const SIZE_10M: usize = 10 * 1024 * 1024;
        const FILE: &str = "./test_allocate_file";

        assert!(fallocate(FILE, SIZE_10M).is_ok());

        let path = Path::new(FILE);
        assert!(path.exists());
        assert!(path.is_file());

        let meta = fs::metadata(FILE).unwrap();
        assert_eq!(SIZE_10M, meta.size() as usize);

        fs::remove_file(FILE).unwrap();
    }

    #[test]
    fn create_new_file() {
        const FILE: &str = "./non_existing";
        const PSIZE: usize = 256;
        const FSIZE: usize = 1024;

        let fe = FileEntity::new(FILE, PSIZE, FSIZE);
        assert!(fe.is_ok());

        let fe = fe.unwrap();
        assert_eq!(fe.piece_size, PSIZE);
        assert_eq!(fe.pieces.len(), FSIZE / PSIZE);

        drop(fe);
        fs::remove_file(FILE).unwrap();
    }

    #[test]
    fn file_already_exist() {
        let fe = FileEntity::new("./Cargo.toml", 0, 0);
        assert!(fe.is_err());
        if let Err(e) = fe {
            assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn file_not_allowed() {
        let fe = FileEntity::new("/root/haxxor", 1024, 1024);
        assert!(fe.is_err());
        if let Err(e) = fe {
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        } else {
            assert!(false);
        }
    }

    #[tokio::test]
    async fn read_local_torrent() {
        const TORRENT: &str = "./tests/torrent_files/test_local.torrent";
        let fread = fs::read(TORRENT).unwrap();
        let size = fs::metadata(TORRENT).unwrap().size();
        let file = fs::OpenOptions::new().read(true).open(TORRENT).unwrap();

        let mut piece = Piece::new(
            size as usize,
            size as usize,
            Arc::new(Mutex::new(rio::new().unwrap())),
        );
        let res = piece.read(&file, 0).await;

        assert!(res.is_ok());
        assert_eq!(fread, piece.bytes);
    }

    #[tokio::test]
    async fn write_local_torrent() {
        const TORRENT: &str = "./tests/torrent_files/test_local.torrent";
        const OUT_FILE: &str = "./duplicate.torrent";
        let fread = fs::read(TORRENT).unwrap();
        let size = fs::metadata(TORRENT).unwrap().size() as usize;
        let fout = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(OUT_FILE)
            .unwrap();

        let mut piece = Piece::new(size, size, Arc::new(Mutex::new(rio::new().unwrap())));
        piece.update(0, &fread);
        assert_eq!(fread, piece.bytes);
        let res = piece.write(&fout, 0).await;

        drop(fout);
        let out_read = fs::read(OUT_FILE).unwrap();

        assert!(res.is_ok());
        assert_eq!(fread, out_read);

        fs::remove_file(OUT_FILE).unwrap();
    }

    #[tokio::test]
    async fn hash_local_torrent() {
        const TORRENT: &str = "./tests/torrent_files/test_local.torrent";
        let file = fs::OpenOptions::new().read(true).open(TORRENT).unwrap();
        let size = fs::metadata(TORRENT).unwrap().size() as usize;

        let mut piece = Piece::new(size, size, Arc::new(Mutex::new(rio::new().unwrap())));
        piece.read(&file, 0).await.unwrap();

        assert_eq!(
            "4365572a000ba4d3a321594bf0509fd5abd8dfa3".to_string(),
            crate::decode_torrent::bytes_to_hash(&piece.hash())
        );
    }
}

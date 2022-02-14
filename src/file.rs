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

use tokio::{net::TcpStream, sync::Mutex};

#[derive(Debug)]
pub struct Piece {
    piece_size: usize,
    ring: Arc<Mutex<Rio>>,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct FileEntity {
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

    pub async fn read(&self, file: &File, offset: usize) -> io::Result<()> {
        let bytes_read = self
            .ring
            .lock()
            .await
            .read_at(file, &self.bytes, offset as u64)
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

    pub async fn load_piece(&mut self, index: usize) -> io::Result<()> {
        if self.pieces[index].is_some() {
            return Ok(());
        }

        // TODO: Handle the case of the last piece
        let piece = Piece::new(self.piece_size, self.piece_size, self.ring.clone());
        piece.read(&self.file, index * self.piece_size).await?;
        self.pieces[index] = Some(piece);

        Ok(())
    }

    pub fn sub_piece(&self, index: usize, offset: usize, length: usize) -> Vec<u8> {
        if let Some(p) = &self.pieces[index] {
            p.bytes[offset..offset + length].try_into().unwrap()
        } else {
            // TODO: change panic to error
            panic!("Block at index: {} not loaded", index);
        }
    }

    pub async fn write_sub_piece(
        &mut self,
        index: usize,
        offset: usize,
        buf: &[u8],
    ) -> io::Result<()> {
        if self.pieces[index].is_none() {
            self.load_piece(index).await?;
        }

        let p = self.pieces[index].as_mut().unwrap();
        for (x, &y) in p.bytes[offset..offset + buf.len()]
            .iter_mut()
            .zip(buf.iter())
        {
            *x = y;
        }

        Ok(())
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
            panic!();
        }
    }

    #[test]
    fn file_not_allowed() {
        let fe = FileEntity::new("/root/haxxor", 1024, 1024);
        assert!(fe.is_err());
        if let Err(e) = fe {
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        } else {
            panic!();
        }
    }

    #[tokio::test]
    async fn read_local_torrent() {
        const TORRENT: &str = "./tests/torrent_files/test_local.torrent";
        let fread = fs::read(TORRENT).unwrap();
        let size = fs::metadata(TORRENT).unwrap().size();
        let file = fs::OpenOptions::new().read(true).open(TORRENT).unwrap();

        let piece = Piece::new(
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

        let piece = Piece::new(size, size, Arc::new(Mutex::new(rio::new().unwrap())));
        piece.read(&file, 0).await.unwrap();

        assert_eq!(
            "c4e5b681c0bf06b5946229d63ce013b41495e9b7".to_string(),
            crate::decode_torrent::bytes_to_hash(&piece.hash())
        );
    }
}

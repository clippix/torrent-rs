use std::{
    fs::{self, File},
    io,
    io::Error,
    os::{raw::c_int, unix::fs::MetadataExt, unix::prelude::AsRawFd},
    path::Path,
};

const BLOCK_SIZE: usize = 2 << 14;

type Block = [u8; BLOCK_SIZE];

#[derive(Debug)]
struct Piece {
    blocks: Vec<Block>,
}

#[derive(Debug)]
struct FileEntity {
    file: File,
    piece_size: usize,
    pieces: Vec<Option<Piece>>,
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
            piece_size,
            pieces: std::iter::repeat_with(|| None).take(pieces).collect(),
        })
    }
}

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
        let fe = FileEntity::new("/root/haxxor", BLOCK_SIZE, BLOCK_SIZE);
        assert!(fe.is_err());
        if let Err(e) = fe {
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        } else {
            assert!(false);
        }
    }
}

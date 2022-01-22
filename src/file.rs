use std::{
    fs,
    io::Error,
    os::{raw::c_int, unix::prelude::AsRawFd},
    path::Path,
};

fn fallocate<S: AsRef<Path>>(file: S, size: usize) -> Result<(), Error> {
    let file = fs::OpenOptions::new()
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

    Ok(())
}

#[cfg(test)]
mod file_tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn allocate_file() {
        const SIZE_10M: usize = 10 * 1024 * 1024;
        const FILE: &str = "./test_allocate_file";

        fallocate(FILE, SIZE_10M);

        let path = Path::new(FILE);
        assert!(path.exists());
        assert!(path.is_file());

        let meta = fs::metadata(FILE).unwrap();
        assert_eq!(SIZE_10M, meta.size() as usize);

        fs::remove_file(FILE).unwrap();
    }
}

// Module heavily inspired by https://github.com/P3KI/bendy/blob/master/examples/decode_torrent.rs
use bendy::{
    decoding::{Error, FromBencode, Object, ResultExt},
    encoding::AsString,
};

use sha1::{Digest, Sha1};

use crate::definitions::InfoHash;

#[derive(Debug)]
pub struct MetaInfo {
    pub announce: String,
    pub info: Info,
    pub comment: Option<String>,
    pub created_by: Option<String>,
    pub creation_date: Option<u64>,
    pub http_seeds: Option<Vec<String>>,
    pub url_list: Option<String>,
}

// File related information (Single-file format)
#[derive(Debug)]
pub struct Info {
    pub piece_length: String,
    pub pieces: Vec<String>,
    pub name: String,
    pub file_length: String,
    pub md5sum: Option<String>,
}

fn bytes_to_num(input: &[u8]) -> usize {
    let mut res = 0;

    for &x in input {
        res *= 10;
        res += (x - b'0') as usize;
    }

    res
}

// TODO: Find a more elegant / normal way of getting Info Hash
pub fn get_info_hash(input: &[u8]) -> InfoHash {
    let mut idx = 0;
    let mut buf = vec![];

    loop {
        let bytes: [u8; 7] = input[idx..idx + 7].try_into().unwrap();
        if bytes == *b"4:infod" {
            break;
        }
        idx += 1;
    }

    idx += 7;
    buf.push(b'd');

    let mut stack = 0;

    loop {
        match input[idx] {
            b'e' if stack == 0 => break,
            b'e' => {
                stack -= 1;
                buf.push(input[idx]);
                idx += 1;
            }
            n if (b'0'..=b'9').contains(&n) => {
                let mut idx2 = 0;

                while input[idx + idx2] != b':' {
                    buf.push(input[idx + idx2]);
                    idx2 += 1;
                }

                let num = bytes_to_num(&input[idx..idx + idx2]);

                idx += idx2;
                for _ in 0..num + 1 {
                    buf.push(input[idx]);
                    idx += 1;
                }
            }
            b'i' => {
                while input[idx] != b'e' {
                    buf.push(input[idx]);
                    idx += 1;
                }
                buf.push(input[idx]);
                idx += 1;
            }
            b'l' => {
                stack += 1;
                buf.push(input[idx]);
                idx += 1;
            }
            b'd' => {
                stack += 1;
                buf.push(input[idx]);
                idx += 1;
            }
            x => panic!("Unexpected byte: {}", x),
        }
    }

    buf.push(b'e');
    let mut hasher = Sha1::new();
    hasher.update(&buf);

    hasher.finalize().try_into().unwrap()
}

impl FromBencode for MetaInfo {
    // Try to parse with a `max_depth` of two.
    //
    // The required max depth of a data structure is calculated as follows:
    //
    //  - Every potential nesting level encoded as bencode dictionary  or list count as +1,
    //  - everything else is ignored.
    //
    // This typically means that we only need to count the amount of nested structs and container
    // types. (Potentially ignoring lists of bytes as they are normally encoded as strings.)
    //
    // struct MetaInfo {                    // encoded as dictionary (+1)
    //    announce: String,
    //    info: Info {                      // encoded as dictionary (+1)
    //      piece_length: String,
    //      pieces: Vec<u8>,                // encoded as string and therefore ignored
    //      name: String,
    //      file_length: String,
    //    },
    //    comment: Option<String>,
    //    creation_date: Option<u64>,
    //    http_seeds: Option<Vec<String>>   // if available encoded as list but even then doesn't
    //                                         increase the limit over the deepest chain including
    //                                         info
    // }
    const EXPECTED_RECURSION_DEPTH: usize = Info::EXPECTED_RECURSION_DEPTH + 1;

    /// Entry point for decoding a torrent. The dictionary is parsed for all
    /// non-optional and optional fields. Missing optional fields are ignored
    /// but any other missing fields result in stopping the decoding and in
    /// spawning [`DecodingError::MissingField`].
    fn decode_bencode_object(object: Object) -> Result<Self, Error>
    where
        Self: Sized,
    {
        let mut announce = None;
        let mut comment = None;
        let mut creation_date = None;
        let mut http_seeds = None;
        let mut info = None;
        let mut created_by = None;
        let mut url_list = None;

        let mut dict_dec = object.try_into_dictionary()?;
        while let Some(pair) = dict_dec.next_pair()? {
            match pair {
                (b"announce", value) => {
                    announce = String::decode_bencode_object(value)
                        .context("announce")
                        .map(Some)?;
                }
                (b"comment", value) => {
                    comment = String::decode_bencode_object(value)
                        .context("comment")
                        .map(Some)?;
                }
                (b"creation date", value) => {
                    creation_date = u64::decode_bencode_object(value)
                        .context("creation_date")
                        .map(Some)?;
                }
                (b"httpseeds", value) => {
                    http_seeds = Vec::decode_bencode_object(value)
                        .context("http_seeds")
                        .map(Some)?;
                }
                (b"info", value) => {
                    info = Info::decode_bencode_object(value)
                        .context("info")
                        .map(Some)?;
                }
                (b"created by", value) => {
                    created_by = String::decode_bencode_object(value)
                        .context("created_by")
                        .map(Some)?;
                }
                (b"url-list", value) => {
                    url_list = String::decode_bencode_object(value)
                        .context("url-list")
                        .map(Some)?;
                }
                (unknown_field, _) => {
                    return Err(Error::unexpected_field(String::from_utf8_lossy(
                        unknown_field,
                    )));
                }
            }
        }

        let announce = announce.ok_or_else(|| Error::missing_field("announce"))?;
        let info = info.ok_or_else(|| Error::missing_field("info"))?;

        Ok(MetaInfo {
            announce,
            info,
            comment,
            created_by,
            creation_date,
            http_seeds,
            url_list,
        })
    }
}

pub fn bytes_to_hash(hash: &InfoHash) -> String {
    hash.iter().map(|c| format!("{:02x}", c)).collect()
}

pub fn pieces_to_hash(input: &[u8]) -> Vec<String> {
    assert!(input.len() % 20 == 0);

    let mut res = Vec::new();

    for chk in input.chunks(20) {
        res.push(bytes_to_hash(chk.try_into().unwrap()));
    }

    res
}

impl FromBencode for Info {
    const EXPECTED_RECURSION_DEPTH: usize = 1;

    /// Treats object as dictionary containing all fields for the info struct.
    /// On success the dictionary is parsed for the fields of info which are
    /// necessary for torrent. Any missing field will result in a missing field
    /// error which will stop the decoding.
    fn decode_bencode_object(object: Object) -> Result<Self, Error>
    where
        Self: Sized,
    {
        let mut file_length = None;
        let mut name = None;
        let mut piece_length = None;
        let mut pieces = None;
        let mut md5sum = None;

        let mut dict_dec = object.try_into_dictionary()?;
        while let Some(pair) = dict_dec.next_pair()? {
            match pair {
                (b"length", value) => {
                    file_length = value
                        .try_into_integer()
                        .context("file.length")
                        .map(ToString::to_string)
                        .map(Some)?;
                }
                (b"name", value) => {
                    name = String::decode_bencode_object(value)
                        .context("name")
                        .map(Some)?;
                }
                (b"piece length", value) => {
                    piece_length = value
                        .try_into_integer()
                        .context("length")
                        .map(ToString::to_string)
                        .map(Some)?;
                }
                (b"pieces", value) => {
                    pieces = AsString::decode_bencode_object(value)
                        .context("pieces")
                        .map(|bytes| Some(pieces_to_hash(&bytes.0)))?;
                }
                (b"md5sum", value) => {
                    md5sum = String::decode_bencode_object(value)
                        .context("md5sum")
                        .map(Some)?;
                }
                (unknown_field, _) => {
                    return Err(Error::unexpected_field(String::from_utf8_lossy(
                        unknown_field,
                    )));
                }
            }
        }

        let file_length = file_length.ok_or_else(|| Error::missing_field("file_length"))?;
        let name = name.ok_or_else(|| Error::missing_field("name"))?;
        let piece_length = piece_length.ok_or_else(|| Error::missing_field("piece_length"))?;
        let pieces = pieces.ok_or_else(|| Error::missing_field("pieces"))?;

        // Check that we discovered all necessary fields
        Ok(Info {
            file_length,
            name,
            piece_length,
            pieces,
            md5sum,
        })
    }
}

#[cfg(test)]
mod decode_torrent_tests {
    use super::*;
    use std::fs;

    fn read_torrent(torrent: &str) -> Vec<u8> {
        fs::read(torrent).unwrap()
    }

    #[test]
    fn test_decode_test_torrent() {
        let torrent = read_torrent("./tests/torrent_files/test.torrent");
        let meta_info = MetaInfo::from_bencode(&torrent).unwrap();
        assert_eq!(meta_info.announce, "udp://tracker.opentrackr.org:1337");
        assert_eq!(meta_info.created_by, Some("mktorrent 1.1".to_string()));
        assert_eq!(
            meta_info.info.name,
            "manjaro-gnome-21.2.1-minimal-220103-linux515.iso"
        );
        assert_eq!(
            meta_info.url_list.unwrap(),
            "https://download.manjaro.org/gnome/21.2.1/manjaro-gnome-21.2.1-minimal-220103-linux515.iso"
        );
    }

    #[test]
    fn test_local_torrent() {
        let torrent = read_torrent("./tests/torrent_files/test_local.torrent");
        let meta_info = MetaInfo::from_bencode(&torrent).unwrap();
        assert_eq!(meta_info.announce, "udp://192.168.0.101:3000");
    }

    #[test]
    fn test_get_info_hash() {
        let torrent = read_torrent("./tests/torrent_files/test_local.torrent");
        let hash = get_info_hash(&torrent);
        assert_eq!(
            "52b62d34a8336f2e934df62181ad4c2f1b43c185".to_string(),
            bytes_to_hash(&hash)
        );
    }
}

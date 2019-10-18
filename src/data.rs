use std::{
    convert::TryFrom,
    fmt::Display,
    io::{self, prelude::*},
    ops::Deref,
    path::Path,
};

use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Tag(String);

impl Deref for Tag {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Error, Debug)]
#[error("Invalid tag {s}: Must be between 1 and 255 characters, is {len}")]
pub struct TagError {
    len: usize,
    s: String,
}

impl TryFrom<String> for Tag {
    type Error = TagError;

    fn try_from(other: String) -> Result<Self, Self::Error> {
        let len = other.chars().count();
        if len > 1 && len < 256 {
            Ok(Tag(other))
        } else {
            Err(TagError { len, s: other })
        }
    }
}

#[derive(Clone)]
pub struct Blake2bpHash(Box<[u8; 64]>);

impl Display for Blake2bpHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_str(&hex::encode(&self.0[..]))
    }
}

impl Deref for Blake2bpHash {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

#[derive(Error, Debug)]
#[error("Invalid hash, must be exactly 64 byte long, is {len}")]
pub struct HashError {
    len: usize,
}

impl TryFrom<&[u8]> for Blake2bpHash {
    type Error = HashError;

    fn try_from(other: &[u8]) -> Result<Self, Self::Error> {
        let len = other.len();
        let mut buf = [0; 64];
        if len == 64 {
            buf.copy_from_slice(other);
            Ok(Self(Box::new(buf)))
        } else {
            Err(HashError { len })
        }
    }
}

impl Blake2bpHash {
    pub fn from_file<P>(path: P) -> Result<Blake2bpHash, io::Error>
    where
        P: AsRef<Path>,
    {
        use blake2b_simd::blake2bp::State;

        let mut fh = std::fs::File::open(path)?;
        let mut state = State::new();
        let mut buf = [0; 8192];
        loop {
            match fh.read(&mut buf)? {
                0 => break,
                n => {
                    state.update(&buf[..n]);
                }
            }
        }

        let ret = Box::new(*state.finalize().as_array());
        Ok(Self(ret))
    }
}

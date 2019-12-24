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
        if len > 0 && len < 256 {
            Ok(Tag(other))
        } else {
            Err(TagError { len, s: other })
        }
    }
}

impl Tag {
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for Tag {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Clone)]
pub struct Blake2bHash(Box<[u8; 64]>);

impl Display for Blake2bHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_str(&hex::encode(&self.0[..]))
    }
}

impl Deref for Blake2bHash {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl AsRef<[u8]> for Blake2bHash {
    fn as_ref(&self) -> &[u8] {
        &*self.0
    }
}

#[derive(Error, Debug)]
#[error("Invalid hash, must be exactly 64 byte long, is {len}")]
pub struct HashError {
    len: usize,
}

impl TryFrom<&[u8]> for Blake2bHash {
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

impl Blake2bHash {
    pub fn from_read(r: &mut dyn Read) -> Result<Blake2bHash, io::Error> {
        let mut state = blake2b_simd::State::new();
        let mut buf = [0; 8192];
        loop {
            match r.read(&mut buf)? {
                0 => break,
                n => {
                    state.update(&buf[..n]);
                }
            }
        }
        let ret = Box::new(*state.finalize().as_array());
        Ok(Self(ret))
    }

    pub fn from_file<P>(path: P) -> Result<Blake2bHash, io::Error>
    where
        P: AsRef<Path>,
    {
        let mut fh = std::fs::File::open(path)?;

        Self::from_read(&mut fh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashing_works() {
        let input = vec![b'a'; 8192 * 3 - 28];
        let real_input_hash = "140def0a7a9c50efd14d7a11330e8a8c4d0cf3a1d1fe0953060c13a78928ded152d198c7e20a69d237b98ee3639822156fb78778577a97efd1dccabb6c4a74f6";
        let hash = Blake2bHash::from_read(&mut std::io::Cursor::new(input)).unwrap();
        assert_eq!(&hash.to_string(), real_input_hash);
    }

    #[test]
    fn try_from_hash() {
        assert!(Blake2bHash::try_from(&vec![0_u8; 64][..]).is_ok());
        assert!(Blake2bHash::try_from(&vec![0_u8; 20][..]).is_err())
    }

    #[test]
    fn try_from_tag() {
        assert!(Tag::try_from(String::from("")).is_err());
        assert!(Tag::try_from(String::from("a")).is_ok());
        assert!(Tag::try_from(std::iter::repeat('a').take(255).collect::<String>()).is_ok());
        assert!(Tag::try_from(std::iter::repeat('a').take(256).collect::<String>()).is_err());
        assert!(Tag::try_from(std::iter::repeat('a').take(1000).collect::<String>()).is_err());
    }
}

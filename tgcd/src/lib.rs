mod client;
mod config;
mod data;

pub mod raw {
    tonic::include_proto!("tgcd");
}

pub use client::{Error, TgcdClient};
pub use data::{Blake2bHash, HashError, Tag, TagError};

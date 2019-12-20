use std::convert::TryFrom;

use thiserror::Error;
use tonic::Request;

use crate::{config, raw, Blake2bHash, Tag};

pub struct TgcdClient(raw::tgcd_client::TgcdClient<tonic::transport::Channel>);

#[derive(Error, Debug)]
pub enum Error {
    #[error("server returned status {0}")]
    Status(#[from] tonic::Status),

    #[error("Server returned invalid Tag")]
    InvalidTag(#[from] crate::data::TagError),

    #[error("Can't load global config: {0}")]
    Config(#[from] config::Error),

    #[error("Can't connect to endpoint: {0}")]
    Connect(tonic::transport::Error),
}

impl TgcdClient {
    pub async fn connect<D>(url: D) -> Result<Self, tonic::transport::Error>
    where
        D: std::convert::TryInto<tonic::transport::Endpoint>,
        D::Error: Into<Box<dyn std::error::Error + Sync + Send>>,
    {
        raw::tgcd_client::TgcdClient::connect(url)
            .await
            .map(|c| Self(c))
    }

    pub async fn from_global_config() -> Result<Self, Error> {
        let cfg = config::Config::load().await?;
        Self::connect(cfg.endpoint.into_string())
            .await
            .map_err(Error::Connect)
    }

    pub async fn add_tags_to_hash(
        &mut self,
        hash: &Blake2bHash,
        tags: Vec<Tag>,
    ) -> Result<(), Error> {
        self.0
            .add_tags_to_hash(Request::new(raw::AddTags {
                hash: hash.to_vec(),
                tags: tags.into_iter().map(|s| s.into_string()).collect(),
            }))
            .await?;
        Ok(())
    }

    pub async fn get_tags(&mut self, hash: &Blake2bHash) -> Result<Vec<Tag>, Error> {
        self.0
            .get_tags(Request::new(raw::Hash {
                hash: hash.to_vec(),
            }))
            .await
            .map_err(Error::from)
            .and_then(|resp| {
                resp.into_inner()
                    .tags
                    .into_iter()
                    .map(|t| Tag::try_from(t).map_err(Error::from))
                    .collect()
            })
    }

    pub async fn get_multiple_tags(
        &mut self,
        hashes: impl IntoIterator<Item = &Blake2bHash>,
    ) -> Result<Vec<Vec<Tag>>, Error> {
        let tags = self
            .0
            .get_multiple_tags(Request::new(raw::GetMultipleTagsReq {
                hashes: hashes.into_iter().map(|hash| hash.to_vec()).collect(),
            }))
            .await?
            .into_inner()
            .tags;

        tags.into_iter()
            .map(|tags| {
                tags.tags
                    .into_iter()
                    .map(|tag| Tag::try_from(tag).map_err(Error::from))
                    .collect::<Result<Vec<Tag>, Error>>()
            })
            .collect()
    }

    pub async fn copy_tags(&mut self, src: &Blake2bHash, dest: &Blake2bHash) -> Result<(), Error> {
        self.0
            .copy_tags(Request::new(raw::SrcDest {
                src_hash: src.to_vec(),
                dest_hash: dest.to_vec(),
            }))
            .await?;

        Ok(())
    }
}

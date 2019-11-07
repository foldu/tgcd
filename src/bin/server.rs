use std::{convert::TryFrom, sync::Arc};

use futures::prelude::*;
use serde::Deserialize;
use tgcd::raw::{server, AddTags, GetMultipleTagsReq, GetMultipleTagsResp, Hash, SrcDest, Tags};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_postgres as postgres;
use tonic::{transport::Server, Request, Response, Status};

use tgcd::{Blake2bHash, HashError, Tag, TagError};

#[derive(Deserialize)]
struct Config {
    postgres_url: String,
    port: u16,
}

#[derive(Clone)]
struct Tgcd {
    inner: Arc<TgcdInner>,
}

impl Tgcd {
    async fn new(cfg: &Config) -> Result<Self, SetupError> {
        let (mut client, connection) = postgres::connect(&cfg.postgres_url, postgres::NoTls)
            .map_err(SetupError::PostgresConnect)
            .await?;

        tokio::spawn(connection.map(|r| {
            if let Err(e) = r {
                log::error!("{}", e);
            }
        }));

        let txn = client.transaction().await.unwrap();
        let schema = include_str!("../../sql/schema.sql");
        let _ = txn
            .batch_execute(schema)
            .map_err(SetupError::PostgresSchema)
            .await;
        txn.commit().await.unwrap();

        Ok(Self {
            inner: Arc::new(TgcdInner {
                client: Mutex::new(client),
            }),
        })
    }
}

struct TgcdInner {
    client: Mutex<postgres::Client>,
}

#[derive(Error, Debug)]
pub enum SetupError {
    #[error("Can't connect to postgres: {0}")]
    PostgresConnect(#[source] postgres::Error),

    #[error("Failed creating schema: {0}")]
    PostgresSchema(#[source] postgres::Error),

    #[error("Missing environment variable: {0}")]
    Env(#[from] envy::Error),

    #[error("Can't bind server: {0}")]
    Bind(#[from] tonic::transport::Error),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Error from postgres: {0}")]
    Postgres(#[from] postgres::Error),

    #[error("Invalid hash: {0}")]
    ArgHash(HashError),

    #[error("Invalid tag: {0}")]
    ArgTag(TagError),
}

impl From<Error> for Status {
    fn from(other: Error) -> Self {
        match other {
            Error::Postgres(_) => Status::new(tonic::Code::Unavailable, "db error"),
            Error::ArgHash(_) | Error::ArgTag(_) => {
                Status::new(tonic::Code::InvalidArgument, "Received invalid argument")
            }
        }
    }
}

async fn get_tags(client: &postgres::Client, hash: &Blake2bHash) -> Result<Vec<String>, Error> {
    let stmnt = client
        .prepare(
            "
        SELECT tag.name
        FROM tag tag, hash_tag hash_tag, hash hash
        WHERE
            tag.id = hash_tag.tag_id
            AND hash_tag.hash_id = hash.id
            AND hash.hash = $1",
        )
        .await?;
    let tags = client.query(&stmnt, &[&hash.as_ref()]).await?;
    Ok(tags.into_iter().map(|row| row.get(0)).collect())
}

async fn get_or_insert_hash(
    client: &postgres::Transaction<'_>,
    hash: &Blake2bHash,
) -> Result<i32, Error> {
    let stmnt = client
        .prepare(
            "
    WITH inserted AS (
        INSERT INTO hash(hash)
        VALUES($1)
        ON CONFLICT DO NOTHING
        RETURNING id
    )
    SELECT * FROM inserted

    UNION ALL

    SELECT id FROM hash
    WHERE hash = $1
    ",
        )
        .await?;

    let row = client.query_one(&stmnt, &[&hash.as_ref()]).await?;

    Ok(row.get(0))
}

async fn get_or_insert_tag(txn: &postgres::Transaction<'_>, tag: &str) -> Result<i32, Error> {
    let stmnt = txn
        .prepare(
            "
    WITH inserted AS (
        INSERT INTO tag(name)
        VALUES($1)
        ON CONFLICT DO NOTHING
        RETURNING id
    )
    SELECT * FROM inserted

    UNION ALL

    SELECT id FROM tag
    WHERE name = $1
    ",
        )
        .await?;

    let row = txn.query_one(&stmnt, &[&tag]).await?;

    Ok(row.get(0))
}

async fn add_tags_to_hash(
    txn: &postgres::Transaction<'_>,
    hash: &Blake2bHash,
    tags: &[Tag],
) -> Result<(), Error> {
    let hash_id = get_or_insert_hash(&txn, &hash).await?;
    for tag in tags {
        let tag_id = get_or_insert_tag(&txn, &tag).await?;
        txn.execute(
            "INSERT INTO hash_tag(tag_id, hash_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[&tag_id, &hash_id],
        )
        .await?;
    }
    Ok(())
}

#[tonic::async_trait]
impl server::Tgcd for Tgcd {
    async fn get_tags(&self, req: Request<Hash>) -> Result<Response<Tags>, Status> {
        let client = self.inner.client.lock().await;
        let hash = Blake2bHash::try_from(&*req.into_inner().hash).map_err(Error::ArgHash)?;
        let tags = get_tags(&client, &hash).await?;

        Ok(Response::new(Tags { tags }))
    }

    async fn add_tags_to_hash(&self, req: Request<AddTags>) -> Result<Response<()>, Status> {
        let mut client = self.inner.client.lock().await;
        let AddTags { hash, tags } = req.into_inner();
        let hash = Blake2bHash::try_from(&*hash).map_err(Error::ArgHash)?;
        let tags = tags
            .into_iter()
            .map(Tag::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::ArgTag)?;

        let txn = client.transaction().map_err(Error::Postgres).await?;
        add_tags_to_hash(&txn, &hash, &tags).await?;

        txn.commit().map_err(Error::Postgres).await?;

        Ok(Response::new(()))
    }

    async fn get_multiple_tags(
        &self,
        req: Request<GetMultipleTagsReq>,
    ) -> Result<Response<GetMultipleTagsResp>, Status> {
        let client = self.inner.client.lock().await;
        let hashes = req.into_inner().hashes;
        let hashes = hashes
            .into_iter()
            .map(|hash| Blake2bHash::try_from(&*hash))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::ArgHash)?;

        let tags = future::try_join_all(
            hashes
                .iter()
                .map(|hash| get_tags(&client, &hash).map_ok(|tags| Tags { tags }))
                .collect::<Vec<_>>(),
        )
        .await?;

        Ok(Response::new(GetMultipleTagsResp { tags }))
    }

    async fn copy_tags(&self, req: Request<SrcDest>) -> Result<Response<()>, Status> {
        let SrcDest {
            src_hash,
            dest_hash,
        } = req.into_inner();
        let mut client = self.inner.client.lock().await;

        let src_hash = Blake2bHash::try_from(&*src_hash).map_err(Error::ArgHash)?;
        let dest_hash = Blake2bHash::try_from(&*dest_hash).map_err(Error::ArgHash)?;

        let src_tags = get_tags(&client, &src_hash)
            .await?
            .into_iter()
            .map(|a| Tag::try_from(a).unwrap())
            .collect::<Vec<_>>();

        let txn = client.transaction().await.map_err(Error::Postgres)?;
        add_tags_to_hash(&txn, &dest_hash, &src_tags).await?;
        txn.commit().await.map_err(Error::Postgres)?;

        Ok(Response::new(()))
    }
}

async fn run() -> Result<(), SetupError> {
    let config: Config = envy::from_env()?;
    let addr = format!("0.0.0.0:{}", config.port).parse().unwrap();
    let tgcd = Tgcd::new(&config).await?;

    Server::builder()
        .add_service(server::TgcdServer::new(tgcd))
        .serve(addr)
        .await?;

    Ok(())
}

fn main() {
    env_logger::init();
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Err(e) = rt.block_on(run()) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

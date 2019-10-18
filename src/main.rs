use std::sync::Arc;

use futures::prelude::*;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_postgres as postgres;
use tonic::{transport::Server, Request, Response, Status};

mod tgcd {
    tonic::include_proto!("tgcd");
}

use tgcd::{server, AddTags, Hash, Tags};

#[derive(Deserialize)]
struct Config {
    postgres_url: String,
}

#[derive(Clone)]
struct Tgcd {
    inner: Arc<TgcdInner>,
}

impl Tgcd {
    async fn new(cfg: &Config) -> Result<Self, SetupError> {
        let (client, connection) = postgres::connect(&cfg.postgres_url, postgres::NoTls)
            .map_err(SetupError::PostgresConnect)
            .await?;

        let schema = include_str!("../sql/schema.sql");
        client
            .batch_execute(schema)
            .map_err(SetupError::PostgresSchema)
            .await?;

        let connection = connection.map(|r| {
            if let Err(e) = r {
                log::error!("{}", e);
            }
        });
        tokio::spawn(connection);

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
}

impl From<Error> for Status {
    fn from(other: Error) -> Self {
        match other {
            Error::Postgres(_) => Status::new(tonic::Code::Unavailable, "db error"),
        }
    }
}

async fn get_tags(client: &postgres::Client, hash: &[u8]) -> Result<Vec<String>, Error> {
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
    let tags = client.query(&stmnt, &[&hash]).await?;
    Ok(tags.into_iter().map(|row| row.get(0)).collect())
}

async fn get_or_insert_hash(client: &postgres::Transaction<'_>, hash: &[u8]) -> Result<i32, Error> {
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

    let row = client.query_one(&stmnt, &[&hash]).await?;

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

#[tonic::async_trait]
impl server::Tgcd for Tgcd {
    async fn get_tags(&self, req: Request<Hash>) -> Result<Response<Tags>, Status> {
        let client = self.inner.client.lock().await;
        let tags = get_tags(&client, &req.into_inner().hash).await?;

        Ok(Response::new(Tags { tags }))
    }

    async fn add_tags_to_hash(&self, req: Request<AddTags>) -> Result<Response<()>, Status> {
        let mut client = self.inner.client.lock().await;
        let AddTags { hash, tags } = req.into_inner();

        let txn = client.transaction().map_err(Error::Postgres).await?;
        let hash_id = get_or_insert_hash(&txn, &hash).await?;
        for tag in tags {
            let tag_id = get_or_insert_tag(&txn, &tag).await?;
            txn.execute(
                "INSERT INTO hash_tag(tag_id, hash_id) VALUES ($1, $2)",
                &[&tag_id, &hash_id],
            )
            .map_err(Error::Postgres)
            .await?;
        }

        txn.commit().map_err(Error::Postgres).await?;

        Ok(Response::new(()))
    }
}

async fn run() -> Result<(), SetupError> {
    let addr = "0.0.0.0:8000".parse().unwrap();
    let config = envy::from_env()?;
    let tgcd = Tgcd::new(&config).await?;

    Server::builder()
        .serve(addr, server::TgcdServer::new(tgcd))
        .await?;

    Ok(())
}

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Err(e) = rt.block_on(run()) {
        eprintln!("{}", e);
    }
}

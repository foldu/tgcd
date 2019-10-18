use std::sync::Arc;

use futures::prelude::*;
use serde::Deserialize;
use snafu::{futures::TryFutureExt, ResultExt, Snafu};
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
    async fn new(cfg: &Config) -> Result<Self, anyhow::Error> {
        let (client, connection) = postgres::connect(&cfg.postgres_url, postgres::NoTls).await?;

        let schema = include_str!("../sql/schema.sql");
        client.batch_execute(schema).await?;

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

#[derive(Snafu, Debug)]
pub enum Error {
    Postgres { source: postgres::Error },
}

impl From<Error> for Status {
    fn from(other: Error) -> Self {
        match other {
            Error::Postgres { .. } => Status::new(tonic::Code::Unavailable, "db error"),
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
        .context(Postgres)
        .await?;
    let tags = client.query(&stmnt, &[&hash]).context(Postgres).await?;
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
        .context(Postgres)
        .await?;

    client
        .query_one(&stmnt, &[&hash])
        .context(Postgres)
        .await
        .map(|row| row.get(0))
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
        .context(Postgres)
        .await?;

    txn.query_one(&stmnt, &[&tag])
        .context(Postgres)
        .await
        .map(|row| row.get(0))
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

        let txn = client.transaction().context(Postgres).await?;
        let hash_id = get_or_insert_hash(&txn, &hash).await?;
        for tag in tags {
            let tag_id = get_or_insert_tag(&txn, &tag).await?;
            txn.execute(
                "INSERT INTO hash_tag(tag_id, hash_id) VALUES ($1, $2)",
                &[&tag_id, &hash_id],
            )
            .context(Postgres)
            .await?;
        }

        txn.commit().context(Postgres).await?;

        Ok(Response::new(()))
    }
}

async fn run() -> Result<(), anyhow::Error> {
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

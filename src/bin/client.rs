use std::{
    collections::HashMap,
    convert::TryFrom,
    io,
    path::{Path, PathBuf},
};

use rayon::prelude::*;
use structopt::StructOpt;
use tgcd::{client::TgcdClient, Blake2bHash, Tag};
use thiserror::Error;

#[derive(StructOpt)]
struct Opt {
    #[structopt(short, long)]
    json: bool,

    #[structopt(subcommand)]
    cmd: Subcmd,
}

#[derive(StructOpt)]
enum Subcmd {
    AddFileTags { file: PathBuf, tags: Vec<String> },
    GetFileTags { file: PathBuf },
    GetFilesTags { files: Vec<String> },
    CopyTags { src: PathBuf, dest: PathBuf },
}

#[derive(Error, Debug)]
enum Error {
    #[error("{0}")]
    InvalidTag(#[from] tgcd::TagError),

    #[error("{0}")]
    RpcConnect(tgcd::client::Error),

    #[error("Error while doing rpc call: {0}")]
    Rpc(#[from] tgcd::client::Error),

    #[error("Can't hash file {}: {}", path.display(), e)]
    Hash {
        #[source]
        e: std::io::Error,
        path: PathBuf,
    },
}

trait Output {
    fn file_tags(&self, tags: &[String], out: &mut dyn std::io::Write) -> Result<(), io::Error>;
    fn files_tags(
        &self,
        tag_map: &HashMap<String, Vec<String>>,
        out: &mut dyn io::Write,
    ) -> Result<(), io::Error>;
}

struct Json;

impl Output for Json {
    fn file_tags(&self, tags: &[String], out: &mut dyn io::Write) -> Result<(), io::Error> {
        let s = serde_json::to_string(tags).unwrap();
        out.write(s.as_bytes()).map(|_| ())
    }

    fn files_tags(
        &self,
        tag_map: &HashMap<String, Vec<String>>,
        out: &mut dyn io::Write,
    ) -> Result<(), io::Error> {
        let s = serde_json::to_string(tag_map).unwrap();
        out.write(s.as_bytes()).map(|_| ())
    }
}

struct Human;

impl Output for Human {
    fn file_tags(&self, tags: &[String], out: &mut dyn io::Write) -> Result<(), io::Error> {
        for tag in tags {
            writeln!(out, "{}", tag)?;
        }
        Ok(())
    }

    fn files_tags(
        &self,
        tag_map: &HashMap<String, Vec<String>>,
        out: &mut dyn io::Write,
    ) -> Result<(), io::Error> {
        for (file, tags) in tag_map {
            writeln!(out, "{}:", file)?;
            for tag in tags {
                writeln!(out, "{}", tag)?;
            }
        }
        Ok(())
    }
}

fn try_hash(path: PathBuf) -> Result<Blake2bHash, Error> {
    Blake2bHash::from_file(&path).map_err(|e| Error::Hash { e, path })
}

async fn run() -> Result<(), Error> {
    let opt = Opt::from_args();

    rayon::ThreadPoolBuilder::new().build_global().unwrap();

    let mut client = TgcdClient::from_global_config()
        .await
        .map_err(Error::RpcConnect)?;

    let output: Box<dyn Output> = if opt.json {
        Box::new(Json)
    } else {
        Box::new(Human)
    };

    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    match opt.cmd {
        Subcmd::AddFileTags { file, tags } => {
            let tags = tags
                .into_iter()
                .map(|tag| Tag::try_from(tag).map_err(Error::from))
                .collect::<Result<Vec<_>, Error>>()?;
            let hash = try_hash(file)?;
            client.add_tags_to_hash(&hash, tags).await?;
        }

        Subcmd::CopyTags { src, dest } => {
            // FIXME: is this even worth it?
            let mut hashes = vec![src, dest]
                .into_par_iter()
                .map(|path| try_hash(path))
                .collect::<Result<Vec<_>, _>>()?;

            let dest_hash = hashes.pop().unwrap();
            let src_hash = hashes.pop().unwrap();

            client.copy_tags(&src_hash, &dest_hash).await?;
        }

        Subcmd::GetFileTags { file } => {
            let hash = try_hash(file)?;

            let tags: Vec<_> = client
                .get_tags(&hash)
                .await?
                .into_iter()
                .map(|t| t.into_string())
                .collect();

            output.file_tags(&tags, &mut stdout).unwrap();
        }

        Subcmd::GetFilesTags { files } => {
            let file_hashes = files
                .into_par_iter()
                .filter_map(|file| match Blake2bHash::from_file(&file) {
                    Ok(hash) => Some((file, hash)),
                    Err(e) => {
                        eprintln!(
                            "{}",
                            Error::Hash {
                                path: Path::new(&file).to_owned(),
                                e
                            }
                        );
                        None
                    }
                })
                .collect::<Vec<_>>();

            let tags = client
                .get_multiple_tags(file_hashes.iter().map(|(_, hash)| hash))
                .await?;

            let out = file_hashes
                .into_iter()
                .zip(tags.into_iter())
                .map(|((file, _), tags)| {
                    let tags = tags.into_iter().map(|t| t.into_string()).collect();
                    (file, tags)
                })
                .collect::<HashMap<_, _>>();

            output.files_tags(&out, &mut stdout).unwrap();
        }
    }

    Ok(())
}

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Err(e) = rt.block_on(run()) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

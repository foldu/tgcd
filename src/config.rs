use std::path::PathBuf;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub endpoint: url::Url,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Can't retrieve project directory")]
    GetProjectDirs,

    #[error("Can't read config in {}: {}", path.display(), source)]
    ReadConfig {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Invalid toml in {}: {}", path.display(), source)]
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl Config {
    pub async fn load() -> Result<Self, Error> {
        let path = directories::ProjectDirs::from("org", "foldu", env!("CARGO_PKG_NAME"))
            .ok_or(Error::GetProjectDirs)?
            .config_dir()
            .join("config.toml");

        let cont = tokio::fs::read(&path)
            .await
            .map_err(|source| Error::ReadConfig {
                path: path.clone(),
                source,
            })?;

        toml::from_slice(&cont).map_err(|source| Error::Toml { path, source })
    }
}

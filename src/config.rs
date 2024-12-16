use serde::Deserialize;
use tokio::fs::read_to_string;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    admin: Vec<i64>,
    telegram: Telegram,
}

impl Config {
    pub fn telegram(&self) -> &Telegram {
        &self.telegram
    }

    pub fn admin(&self) -> &[i64] {
        &self.admin
    }

    pub async fn read(file: &str) -> anyhow::Result<Self> {
        let content = read_to_string(file).await?;
        Ok(toml::from_str(&content)?)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Telegram {
    #[serde(alias = "server", alias = "api-server")]
    api_server: Option<String>,
    #[serde(alias = "key", alias = "api-key", alias = "api")]
    api_key: String,
}

impl Telegram {
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn api_server(&self) -> Option<&String> {
        self.api_server.as_ref()
    }
}

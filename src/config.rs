use std::io::ErrorKind;
use std::path::PathBuf;

use anyhow::{Context, Result};
use etcetera::BaseStrategy;
use serde::Deserialize;

use crate::engines::Engine;

/// Settings from `$XDG_CONFIG_HOME/wq/config.toml`. Every field is optional,
/// and command-line flags take precedence.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub engine: Option<Engine>,
    pub count: Option<usize>,
    pub summary: Option<bool>,
    pub sidebar: Option<bool>,
    pub searxng: Searxng,
    pub brave_api: BraveApi,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Searxng {
    /// Base URL of the instance, e.g. `http://localhost:8888`.
    pub url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BraveApi {
    /// Subscription token for the web search endpoint.
    pub key: Option<String>,
    /// Subscription token for the Answers endpoint; defaults to `key`.
    pub answers_key: Option<String>,
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let base = etcetera::choose_base_strategy().context("cannot locate the home directory")?;
        Ok(base.config_dir().join("wq").join("config.toml"))
    }

    /// Reads the config file, or returns defaults when it does not exist.
    pub fn load() -> Result<Config> {
        let path = Config::path()?;
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                Config::parse(&text).with_context(|| format!("invalid config {}", path.display()))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    fn parse(text: &str) -> Result<Config> {
        Ok(toml::from_str(text)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_field() {
        let config = Config::parse(
            r#"
            engine = "brave-api"
            count = 5
            summary = true
            sidebar = false

            [searxng]
            url = "http://localhost:8888"

            [brave_api]
            key = "k1"
            answers_key = "k2"
            "#,
        )
        .unwrap();
        assert!(matches!(config.engine, Some(Engine::BraveApi)));
        assert_eq!(config.count, Some(5));
        assert_eq!(config.summary, Some(true));
        assert_eq!(config.sidebar, Some(false));
        assert_eq!(config.searxng.url.as_deref(), Some("http://localhost:8888"));
        assert_eq!(config.brave_api.key.as_deref(), Some("k1"));
        assert_eq!(config.brave_api.answers_key.as_deref(), Some("k2"));
    }

    #[test]
    fn accepts_engine_alias() {
        let config = Config::parse(r#"engine = "duckduckgo""#).unwrap();
        assert!(matches!(config.engine, Some(Engine::Ddg)));
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(Config::parse("engnie = \"bing\"").is_err());
    }
}

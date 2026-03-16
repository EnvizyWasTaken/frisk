use crate::util::{config_dir, config_path};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub mirrors: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mirrors: vec!["https://example.com/repo".to_string()],
        }
    }
}

impl Config {
    pub async fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            let cfg = Self::default();
            fs::create_dir_all(config_dir()?).await?;
            let content = serde_json::to_string_pretty(&cfg)?;
            fs::write(&path, content).await?;
            return Ok(cfg);
        }

        let content = fs::read_to_string(path).await?;
        if content.trim().is_empty() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&content)?)
    }

    pub fn resolve_mirror(&self, input: &str) -> Result<String> {
        if input == "default" {
            return self
                .mirrors
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("no mirrors configured in config.json"));
        }

        if input.starts_with("http://") || input.starts_with("https://") {
            return Ok(input.to_string());
        }

        if let Some(found) = self.mirrors.iter().find(|m| m.ends_with(input) || *m == input) {
            return Ok(found.clone());
        }

        Ok(input.to_string())
    }
}

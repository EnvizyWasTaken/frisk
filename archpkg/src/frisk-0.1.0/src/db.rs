use crate::util::{data_dir, db_path};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackageSource {
    Github { repo: String },
    HttpMirror { mirror: String, package: String },
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub name: String,
    pub version: Option<String>,
    pub source: PackageSource,
    pub installed_files: Vec<String>,
    pub installed_bin_names: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledDb {
    pub packages: Vec<InstalledPackage>,
}

impl InstalledDb {
    pub async fn load() -> Result<Self> {
        let path = db_path()?;
        if !path.exists() {
            fs::create_dir_all(data_dir()?).await?;
            let db = Self::default();
            db.save().await?;
            return Ok(db);
        }

        let content = fs::read_to_string(path).await?;
        if content.trim().is_empty() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&content)?)
    }

    pub async fn save(&self) -> Result<()> {
        fs::create_dir_all(data_dir()?).await?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(db_path()?, content).await?;
        Ok(())
    }

    pub fn find(&self, name_or_repo: &str) -> Option<&InstalledPackage> {
        self.packages.iter().find(|pkg| {
            pkg.name == name_or_repo
                || matches!(&pkg.source, PackageSource::Github { repo } if repo == name_or_repo)
        })
    }

    pub fn find_index(&self, name_or_repo: &str) -> Option<usize> {
        self.packages.iter().position(|pkg| {
            pkg.name == name_or_repo
                || matches!(&pkg.source, PackageSource::Github { repo } if repo == name_or_repo)
        })
    }

    pub fn find_index_by_source(&self, source: &PackageSource) -> Option<usize> {
        self.packages.iter().position(|pkg| &pkg.source == source)
    }
}

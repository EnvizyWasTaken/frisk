use anyhow::{anyhow, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MirrorPackageMetadata {
    pub name: String,
    pub version: String,
    pub file: String,
}

pub async fn fetch_package_metadata(mirror_url: &str, package: &str) -> Result<MirrorPackageMetadata> {
    let url = join_url(mirror_url, &format!("{package}.json"));
    let response = reqwest::get(&url).await?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "mirror metadata request failed with status {} for {}",
            response.status(),
            url
        ));
    }
    Ok(response.json::<MirrorPackageMetadata>().await?)
}

pub fn join_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

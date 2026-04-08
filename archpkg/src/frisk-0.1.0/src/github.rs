use anyhow::{anyhow, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

pub fn parse_repo_spec(repo_spec: &str) -> Result<(String, String)> {
    let mut parts = repo_spec.split('/');
    let owner = parts
        .next()
        .ok_or_else(|| anyhow!("missing GitHub owner in repo spec"))?;
    let repo = parts
        .next()
        .ok_or_else(|| anyhow!("missing GitHub repo in repo spec"))?;
    if parts.next().is_some() {
        return Err(anyhow!("repo spec must be in the form owner/repo"));
    }
    Ok((owner.to_string(), repo.to_string()))
}

pub async fn latest_release(owner: &str, repo: &str) -> Result<Release> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/releases/latest"
    );

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("User-Agent", "frisk")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "GitHub release lookup failed with status {}",
            response.status()
        ));
    }

    Ok(response.json::<Release>().await?)
}

pub fn pick_best_asset<'a>(release: &'a Release) -> Option<&'a Asset> {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;

    let arch_aliases = arch_aliases(arch);
    let os_aliases = os_aliases(os);
    let preferred_exts = [".tar.gz", ".tgz", ".zip"];

    release
        .assets
        .iter()
        .filter(|asset| preferred_exts.iter().any(|ext| asset.name.ends_with(ext)))
        .max_by_key(|asset| score_asset_name(&asset.name.to_lowercase(), &arch_aliases, &os_aliases))
        .filter(|asset| score_asset_name(&asset.name.to_lowercase(), &arch_aliases, &os_aliases) > 0)
}

fn score_asset_name(name: &str, arch_aliases: &[&str], os_aliases: &[&str]) -> i32 {
    let mut score = 0;

    for alias in arch_aliases {
        if name.contains(alias) {
            score += 4;
            break;
        }
    }

    for alias in os_aliases {
        if name.contains(alias) {
            score += 4;
            break;
        }
    }

    if name.contains("musl") {
        score += 1;
    }
    if name.contains("gnu") {
        score += 1;
    }
    if name.contains("linux") && std::env::consts::OS == "linux" {
        score += 2;
    }
    if name.contains("apple-darwin") && std::env::consts::OS == "macos" {
        score += 2;
    }

    score
}

fn arch_aliases(arch: &str) -> Vec<&str> {
    match arch {
        "x86_64" => vec!["x86_64", "amd64", "x64"],
        "aarch64" => vec!["aarch64", "arm64"],
        other => vec![other],
    }
}

fn os_aliases(os: &str) -> Vec<&str> {
    match os {
        "linux" => vec!["linux", "unknown-linux", "unknown-linux-gnu", "unknown-linux-musl"],
        "macos" => vec!["darwin", "apple-darwin", "macos", "osx"],
        "windows" => vec!["windows", "pc-windows", "win64", "win32"],
        other => vec![other],
    }
}

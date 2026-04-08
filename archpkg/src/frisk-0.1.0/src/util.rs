use crate::config::Config;
use crate::db::InstalledDb;
use anyhow::{anyhow, Context, Result};
use dirs::{config_dir as dirs_config_dir, data_dir as dirs_data_dir, home_dir};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt, process::Command};

pub fn config_dir() -> Result<PathBuf> {
    Ok(dirs_config_dir()
        .ok_or_else(|| anyhow!("could not resolve config directory"))?
        .join("frisk"))
}

pub fn data_dir() -> Result<PathBuf> {
    Ok(dirs_data_dir()
        .ok_or_else(|| anyhow!("could not resolve data directory"))?
        .join("frisk"))
}

pub fn cache_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("cache"))
}

pub fn temp_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("tmp"))
}

pub fn bin_dir() -> Result<PathBuf> {
    Ok(home_dir()
        .ok_or_else(|| anyhow!("could not resolve home directory"))?
        .join(".local")
        .join("bin"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("installed.json"))
}

pub async fn ensure_base_dirs() -> Result<()> {
    fs::create_dir_all(config_dir()?).await?;
    fs::create_dir_all(data_dir()?).await?;
    fs::create_dir_all(cache_dir()?).await?;
    fs::create_dir_all(temp_dir()?).await?;
    fs::create_dir_all(bin_dir()?).await?;

    let config_path = config_path()?;
    if !config_path.exists() {
        let content = serde_json::to_string_pretty(&Config::default())?;
        fs::write(config_path, content).await?;
    }

    let db_path = db_path()?;
    if !db_path.exists() {
        let content = serde_json::to_string_pretty(&InstalledDb::default())?;
        fs::write(db_path, content).await?;
    }

    Ok(())
}

pub async fn download_to_file(url: &str, destination: &Path) -> Result<()> {
    let response = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "frisk")
        .send()
        .await
        .with_context(|| format!("request failed for {url}"))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "download failed with status {} for {}",
            response.status(),
            url
        ));
    }

    let bytes = response.bytes().await?;
    let mut file = fs::File::create(destination).await?;
    file.write_all(&bytes).await?;
    Ok(())
}

pub async fn run_command(program: &str, args: &[&str], current_dir: Option<&Path>) -> Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = current_dir {
        cmd.current_dir(dir);
    }
    let output = cmd.output().await?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(anyhow!(
        "command failed: {} {:?}\nstdout:\n{}\nstderr:\n{}",
        program,
        args,
        stdout,
        stderr
    ))
}

mod cli;
mod config;
mod db;
mod extract;
mod github;
mod installer;
mod mirror;
mod util;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use cli::Cli;
use config::Config;
use db::{InstalledDb, InstalledPackage, PackageSource};
use installer::{install_from_archive, install_from_directory, remove_installed_package};
use std::path::{Path, PathBuf};
use tokio::fs;
use util::{cache_dir, ensure_base_dirs, temp_dir};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    ensure_base_dirs().await?;
    dispatch(cli).await
}

async fn dispatch(cli: Cli) -> Result<()> {
    let mirror = &cli.mirror;

    match cli.action.as_str() {
        "-g" => {
            let package = cli.require_package()?;
            get_package(mirror, &package).await?;
        }
        "-d" => {
            let package = cli.require_package()?;
            delete_package(&package).await?;
        }
        "-u" => {
            let package = cli.require_package()?;
            update_package(mirror, &package).await?;
        }
        "-U" => update_all(mirror).await?,
        "-c" => {
            let package = cli.require_package()?;
            check_package(&package).await?;
        }
        "-C" => list_packages().await?,
        other => return Err(anyhow!("unknown action: {other}")),
    }

    Ok(())
}

async fn get_package(mirror_name: &str, package: &str) -> Result<()> {
    let resolved_mirror = resolve_source_kind(mirror_name, package).await?;
    match resolved_mirror.as_str() {
        "github" => install_from_github(package).await,
        "local" => install_from_local_file(package).await,
        _ => install_from_http_mirror(mirror_name, package).await,
    }
}

async fn resolve_source_kind(mirror_name: &str, package: &str) -> Result<String> {
    if mirror_name == "github" || package.contains('/') {
        return Ok("github".to_string());
    }
    if mirror_name == "local" || package.ends_with(".frisk") {
        return Ok("local".to_string());
    }
    Ok("http".to_string())
}

async fn install_from_github(repo_spec: &str) -> Result<()> {
    let (owner, repo) = github::parse_repo_spec(repo_spec)?;
    let release = match github::latest_release(&owner, &repo).await {
        Ok(r) => Some(r),
        Err(_) => None,
    };

    if let Some(release) = release {
        if let Some(asset) = github::pick_best_asset(&release) {
            let cache = cache_dir()?.join(format!("{repo}-{}", sanitize_version(&release.tag_name)));
            recreate_dir(&cache).await?;
            let archive_path = cache.join(&asset.name);
            util::download_to_file(&asset.browser_download_url, &archive_path).await?;

            let record = install_from_archive(
                &archive_path,
                Some(repo.clone()),
                PackageSource::Github {
                    repo: format!("{owner}/{repo}"),
                },
                Some(release.tag_name),
            )
            .await?;

            upsert_package(record).await?;
            println!("Installed {} from GitHub release", repo_spec);
            return Ok(());
        }
    }

    install_from_github_source(&owner, &repo).await
}

async fn install_from_github_source(owner: &str, repo: &str) -> Result<()> {
    let workdir = temp_dir()?.join(format!("src-build-{owner}-{repo}"));
    recreate_dir(&workdir).await?;

    let clone_url = format!("https://github.com/{owner}/{repo}.git");
    util::run_command(
        "git",
        &["clone", "--depth", "1", &clone_url, "."],
        Some(&workdir),
    )
    .await
    .context("failed to clone repository")?;

    util::run_command("cargo", &["build", "--release"], Some(&workdir))
        .await
        .context("failed to build repository with cargo")?;

    let binary_name = repo.to_string();
    let binary_path = workdir.join("target").join("release").join(&binary_name);
    if !binary_path.exists() {
        return Err(anyhow!(
            "build succeeded but expected binary was not found at {}",
            binary_path.display()
        ));
    }

    let record = install_from_directory(
        &binary_path,
        &binary_name,
        PackageSource::Github {
            repo: format!("{owner}/{repo}"),
        },
        Some("source-build".to_string()),
    )
    .await?;

    upsert_package(record).await?;
    println!("Installed {} from GitHub source", repo);
    Ok(())
}

async fn install_from_local_file(path_like: &str) -> Result<()> {
    let path = PathBuf::from(path_like);
    if !path.exists() {
        return Err(anyhow!("local package not found: {}", path.display()));
    }

    let record = install_from_archive(&path, None, PackageSource::Local, None).await?;

    println!("Installed {}", record.name);

    upsert_package(record).await?;
    Ok(())
}

async fn install_from_http_mirror(mirror_name: &str, package: &str) -> Result<()> {
    let config = Config::load().await?;
    let mirror_url = config.resolve_mirror(mirror_name)?;
    let metadata = mirror::fetch_package_metadata(&mirror_url, package).await?;

    let archive_url = mirror::join_url(&mirror_url, &metadata.file);
    let workdir = cache_dir()?.join(format!("{}-{}", metadata.name, sanitize_version(&metadata.version)));
    recreate_dir(&workdir).await?;
    let archive_path = workdir.join(
        Path::new(&metadata.file)
            .file_name()
            .ok_or_else(|| anyhow!("package file path has no file name"))?,
    );

    util::download_to_file(&archive_url, &archive_path).await?;

    let record = install_from_archive(
        &archive_path,
        Some(metadata.name.clone()),
        PackageSource::HttpMirror {
            mirror: mirror_url,
            package: metadata.name.clone(),
        },
        Some(metadata.version),
    )
    .await?;

    upsert_package(record).await?;
    println!("Installed {}", metadata.name);
    Ok(())
}

async fn delete_package(name_or_repo: &str) -> Result<()> {
    let mut db = InstalledDb::load().await?;
    let index = db
        .find_index(name_or_repo)
        .ok_or_else(|| anyhow!("package not installed: {name_or_repo}"))?;
    let package = db.packages.remove(index);
    remove_installed_package(&package).await?;
    db.save().await?;
    println!("Deleted {}", package.name);
    Ok(())
}

async fn update_package(mirror_name: &str, name_or_repo: &str) -> Result<()> {
    let db = InstalledDb::load().await?;
    let pkg = db
        .find(name_or_repo)
        .cloned()
        .ok_or_else(|| anyhow!("package not installed: {name_or_repo}"))?;

    match &pkg.source {
        PackageSource::Github { repo } => get_package("github", repo).await,
        PackageSource::HttpMirror { package, .. } => get_package(mirror_name, package).await,
        PackageSource::Local => {
            Err(anyhow!("local package updates are not automatic; reinstall the .frisk file manually"))
        }
    }
}

async fn update_all(mirror_name: &str) -> Result<()> {
    let db = InstalledDb::load().await?;
    if db.packages.is_empty() {
        println!("No packages installed.");
        return Ok(());
    }

    for pkg in db.packages.clone() {
        let result = match &pkg.source {
            PackageSource::Github { repo } => get_package("github", repo).await,
            PackageSource::HttpMirror { package, .. } => get_package(mirror_name, package).await,
            PackageSource::Local => {
                eprintln!("Skipping {}: local packages cannot be updated automatically", pkg.name);
                continue;
            }
        };

        if let Err(err) = result {
            eprintln!("Failed to update {}: {err}", pkg.name);
        }
    }

    println!("Finished updating packages.");
    Ok(())
}

async fn check_package(name_or_repo: &str) -> Result<()> {
    let db = InstalledDb::load().await?;
    if let Some(pkg) = db.find(name_or_repo) {
        println!("Installed: {} {}", pkg.name, pkg.version.as_deref().unwrap_or("unknown"));
    } else {
        println!("Not installed: {name_or_repo}");
    }
    Ok(())
}

async fn list_packages() -> Result<()> {
    let db = InstalledDb::load().await?;
    if db.packages.is_empty() {
        println!("No packages installed.");
        return Ok(());
    }

    for pkg in db.packages {
        let source = match &pkg.source {
            PackageSource::Github { repo } => format!("github:{repo}"),
            PackageSource::HttpMirror { mirror, .. } => format!("http:{mirror}"),
            PackageSource::Local => "local".to_string(),
        };
        println!(
            "{} {} [{}]",
            pkg.name,
            pkg.version.unwrap_or_else(|| "unknown".to_string()),
            source
        );
    }
    Ok(())
}

async fn upsert_package(package: InstalledPackage) -> Result<()> {
    let mut db = InstalledDb::load().await?;
    if let Some(index) = db.find_index(&package.name) {
        let old = db.packages.remove(index);
        let _ = remove_installed_package(&old).await;
    }
    if let Some(index) = db.find_index_by_source(&package.source) {
        let old = db.packages.remove(index);
        let _ = remove_installed_package(&old).await;
    }
    db.packages.push(package);
    db.save().await
}

async fn recreate_dir(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).await?;
    }
    fs::create_dir_all(path).await?;
    Ok(())
}

fn sanitize_version(version: &str) -> String {
    version.replace('/', "_")
}

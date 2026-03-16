use crate::db::{InstalledPackage, PackageSource};
use crate::extract::extract_by_extension;
use crate::util::{bin_dir, temp_dir};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tokio::fs;
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
struct FriskManifest {
    name: String,
    version: Option<String>,
    bin: Option<Vec<String>>,
}

pub async fn install_from_archive(
    archive_path: &Path,
    fallback_name: Option<String>,
    source: PackageSource,
    fallback_version: Option<String>,
) -> Result<InstalledPackage> {
    let extract_root = temp_dir()?.join(format!(
        "extract-{}",
        archive_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("package")
    ));
    if extract_root.exists() {
        fs::remove_dir_all(&extract_root).await?;
    }
    fs::create_dir_all(&extract_root).await?;
    extract_by_extension(archive_path, &extract_root)?;

    let manifest = read_manifest_if_present(&extract_root).await?;
    let name = manifest
        .as_ref()
        .map(|m| m.name.clone())
        .or(fallback_name)
        .unwrap_or_else(|| infer_name_from_archive(archive_path));
    let version = manifest
        .as_ref()
        .and_then(|m| m.version.clone())
        .or(fallback_version);

    let binaries = collect_binaries(&extract_root, manifest.as_ref())?;
    if binaries.is_empty() {
        return Err(anyhow!("no executable files found in extracted package"));
    }

    install_binaries(name, version, source, binaries).await
}

pub async fn install_from_directory(
    binary_path: &Path,
    package_name: &str,
    source: PackageSource,
    version: Option<String>,
) -> Result<InstalledPackage> {
    if !binary_path.exists() {
        return Err(anyhow!("binary not found: {}", binary_path.display()));
    }
    install_binaries(
        package_name.to_string(),
        version,
        source,
        vec![binary_path.to_path_buf()],
    )
    .await
}

async fn install_binaries(
    package_name: String,
    version: Option<String>,
    source: PackageSource,
    binaries: Vec<PathBuf>,
) -> Result<InstalledPackage> {
    fs::create_dir_all(bin_dir()?).await?;

    let mut installed_files = Vec::new();
    let mut installed_bin_names = Vec::new();

    for binary in binaries {
        let file_name = binary
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| anyhow!("binary has invalid file name: {}", binary.display()))?
            .to_string();
        let destination = bin_dir()?.join(&file_name);
        fs::copy(&binary, &destination)
            .await
            .with_context(|| format!("failed to install {}", file_name))?;

        let mut perms = fs::metadata(&destination).await?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&destination, perms).await?;

        installed_files.push(destination.to_string_lossy().to_string());
        installed_bin_names.push(file_name);
    }

    Ok(InstalledPackage {
        name: package_name,
        version,
        source,
        installed_files,
        installed_bin_names,
    })
}

pub async fn remove_installed_package(package: &InstalledPackage) -> Result<()> {
    for file in &package.installed_files {
        let path = PathBuf::from(file);
        if path.exists() {
            let _ = fs::remove_file(path).await;
        }
    }
    Ok(())
}

async fn read_manifest_if_present(extract_root: &Path) -> Result<Option<FriskManifest>> {
    let candidates = [extract_root.join("manifest.json"), extract_root.join("package").join("manifest.json")];

    for candidate in candidates {
        if candidate.exists() {
            let content = fs::read_to_string(candidate).await?;
            let manifest: FriskManifest = serde_json::from_str(&content)?;
            return Ok(Some(manifest));
        }
    }
    Ok(None)
}

fn collect_binaries(extract_root: &Path, manifest: Option<&FriskManifest>) -> Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    if let Some(manifest) = manifest {
        if let Some(bin_entries) = &manifest.bin {
            for bin in bin_entries {
                let exact = extract_root.join(bin);
                if exact.exists() && exact.is_file() {
                    seen.insert(exact.clone());
                    results.push(exact);
                    continue;
                }

                let payload = extract_root.join("payload").join(bin);
                if payload.exists() && payload.is_file() && seen.insert(payload.clone()) {
                    results.push(payload);
                }
            }
        }
    }

    if !results.is_empty() {
        return Ok(results);
    }

    for entry in WalkDir::new(extract_root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if is_probable_binary(path, name)? && seen.insert(path.to_path_buf()) {
            results.push(path.to_path_buf());
        }
    }

    Ok(results)
}

fn is_probable_binary(path: &Path, name: &str) -> Result<bool> {
    let lower = name.to_lowercase();
    let skip_suffixes = [
        ".txt", ".md", ".json", ".toml", ".yaml", ".yml", ".lock", ".rs", ".c", ".h",
        ".o", ".a", ".so", ".dll", ".dylib", ".html", ".css", ".js",
    ];
    if skip_suffixes.iter().any(|suffix| lower.ends_with(suffix)) {
        return Ok(false);
    }

    let metadata = std::fs::metadata(path)?;
    if metadata.len() == 0 {
        return Ok(false);
    }

    let mode = metadata.permissions().mode();
    if mode & 0o111 != 0 {
        return Ok(true);
    }

    Ok(path.parent().map(|p| p.ends_with("bin") || p.ends_with("payload")).unwrap_or(false))
}

fn infer_name_from_archive(archive_path: &Path) -> String {
    let file_name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("package");

    for suffix in [".tar.gz", ".tgz", ".zip", ".frisk"] {
        if let Some(stripped) = file_name.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    file_name.to_string()
}

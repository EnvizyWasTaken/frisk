use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use std::fs::File;
use std::path::Path;
use tar::Archive;

pub fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive.unpack(destination)?;
    Ok(())
}

pub fn extract_zip(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive_path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    zip.extract(destination)?;
    Ok(())
}

pub fn extract_by_extension(archive_path: &Path, destination: &Path) -> Result<()> {
    let name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("archive path has no valid file name"))?;

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return extract_tar_gz(archive_path, destination);
    }
    if name.ends_with(".zip") || name.ends_with(".frisk") {
        return extract_zip(archive_path, destination);
    }

    Err(anyhow!("unsupported archive format: {name}"))
}

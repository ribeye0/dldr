//! Root-checked filesystem operations used by local and downloaded works.
use crate::{LibraryError, Result};
use std::path::{Path, PathBuf};

pub(super) async fn remove_download_path_from_state(
    path: Option<&str>,
    allowed_roots: &[&Path],
) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };

    remove_existing_download_path(Path::new(path), allowed_roots).await
}

pub(super) async fn remove_existing_download_path(
    path: &Path,
    allowed_roots: &[&Path],
) -> Result<()> {
    if !path.try_exists()? {
        return Ok(());
    }

    let canonical_path = path.canonicalize()?;
    let canonical_roots = allowed_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect::<Vec<_>>();

    if !path_is_download_child_of_any_root(&canonical_path, &canonical_roots) {
        return Err(LibraryError::DownloadPathOutsideRoots(canonical_path));
    }

    let metadata = tokio::fs::metadata(&canonical_path).await?;
    if metadata.is_dir() {
        tokio::fs::remove_dir_all(&canonical_path).await?;
    } else {
        tokio::fs::remove_file(&canonical_path).await?;
    }

    Ok(())
}

pub(super) fn canonicalize_existing_directory(path: &Path) -> Result<PathBuf> {
    let canonical_path = path.canonicalize()?;
    let metadata = std::fs::metadata(&canonical_path)?;

    if !metadata.is_dir() {
        return Err(LibraryError::DownloadPathNotDirectory(canonical_path));
    }

    Ok(canonical_path)
}

pub(super) fn path_is_download_child_of_any_root(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| path != root.as_path() && path.starts_with(root))
}

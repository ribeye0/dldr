use crate::paths::{canonicalize_existing_directory, path_is_download_child_of_any_root};
use crate::{Library, LibraryError, Result};
use dldr_storage::WorkDownloadStatus;
use std::{path::Path, path::PathBuf};

use std::{fs, io};

impl Library {
    /// Copy a completed work without changing its download record or moving its files.
    pub async fn export_work_download(
        &self,
        work_id: &str,
        library_root: &Path,
        destination_root: &Path,
    ) -> Result<PathBuf> {
        let lock = self.work_lock(work_id);
        let guard = lock.try_lock_owned().map_err(|_| {
            LibraryError::RecoveryRequired(format!("Work {work_id} is busy; try again later"))
        })?;
        let state = self.storage.work_download_state(work_id).await?;
        if state.status != WorkDownloadStatus::Downloaded {
            return Err(LibraryError::RecoveryRequired(format!(
                "Work {work_id} is not downloaded"
            )));
        }
        let source = state.local_path.ok_or_else(|| {
            LibraryError::RecoveryRequired(format!("Work {work_id} has no local folder"))
        })?;
        let library_root = library_root.to_owned();
        let destination_root = destination_root.to_owned();
        tokio::task::spawn_blocking(move || {
            // Keep the work protected even if the calling webview closes mid-copy.
            let _guard = guard;
            export_directory(Path::new(&source), &library_root, &destination_root)
        })
        .await
        .map_err(|error| LibraryError::Io(io::Error::other(error)))?
    }
}

fn export_directory(
    source: &Path,
    library_root: &Path,
    destination_root: &Path,
) -> Result<PathBuf> {
    let source = canonicalize_existing_directory(source)?;
    let library_root = canonicalize_existing_directory(library_root)?;
    if !path_is_download_child_of_any_root(&source, &[library_root]) {
        return Err(LibraryError::DownloadPathOutsideRoots(source));
    }
    let destination_root = canonicalize_existing_directory(destination_root)?;
    if destination_root.starts_with(&source) {
        return Err(LibraryError::RecoveryRequired(
            "The export destination must be outside the source work folder".to_owned(),
        ));
    }
    let target = destination_root.join(
        source
            .file_name()
            .ok_or_else(|| io::Error::other("Source work folder has no name"))?,
    );
    // Reserve a fresh directory atomically. Never merge with or overwrite an existing export.
    match fs::create_dir(&target) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(LibraryError::DownloadTargetExists(target));
        }
        Err(error) => return Err(error.into()),
    }
    if let Err(error) = copy_contents(&source, &target) {
        let cause = LibraryError::Io(error);
        if fs::remove_dir_all(&target).is_err() {
            return Err(LibraryError::RetainedDownload {
                source: Box::new(cause),
                retained_path: target,
            });
        }
        return Err(cause);
    }
    Ok(target)
}

fn copy_contents(source: &Path, target: &Path) -> io::Result<()> {
    let mut pending = vec![(source.to_owned(), target.to_owned())];
    while let Some((source, target)) = pending.pop() {
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let destination = target.join(entry.file_name());
            if kind.is_dir() {
                fs::create_dir(&destination)?;
                pending.push((entry.path(), destination));
            } else if kind.is_file() {
                let mut input = fs::File::open(entry.path())?;
                let expected = input.metadata()?.len();
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&destination)?;
                let copied = io::copy(&mut input, &mut output)?;
                output.sync_all()?;
                if copied != expected || input.metadata()?.len() != expected {
                    return Err(io::Error::other("Source changed during export"));
                }
            } else {
                // Do not follow links outside the work or copy device files.
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!(
                        "Cannot export symbolic link or special file: {}",
                        entry.path().display()
                    ),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_nested_unicode_files_without_modifying_source_or_overwriting() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let library = temp.path().join("library");
        let source = library.join("RJ000001 作品");
        fs::create_dir_all(source.join("音軌 empty"))?;
        fs::write(source.join("音軌 empty/track.wav"), b"audio")?;
        fs::write(source.join("script.vtt"), b"WEBVTT")?;
        let destination = temp.path().join("export 日本語");
        fs::create_dir(&destination)?;
        let exported = export_directory(&source, &library, &destination)?;
        assert_eq!(fs::read(exported.join("音軌 empty/track.wav"))?, b"audio");
        assert_eq!(fs::read(exported.join("script.vtt"))?, b"WEBVTT");
        assert_eq!(fs::read(source.join("script.vtt"))?, b"WEBVTT");
        fs::write(exported.join("script.vtt"), b"existing")?;
        assert!(matches!(
            export_directory(&source, &library, &destination),
            Err(LibraryError::DownloadTargetExists(_))
        ));
        assert_eq!(fs::read(exported.join("script.vtt"))?, b"existing");
        Ok(())
    }

    #[test]
    fn rejects_recursive_destinations_and_sources_outside_library() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let library = temp.path().join("library");
        let source = library.join("RJ000001");
        fs::create_dir_all(source.join("nested"))?;
        assert!(export_directory(&source, &library, &source).is_err());
        assert!(export_directory(&source, &library, &source.join("nested")).is_err());
        assert!(export_directory(temp.path(), &library, temp.path()).is_err());
        assert!(export_directory(&library, &library, temp.path()).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_and_removes_incomplete_exports() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let library = temp.path().join("library");
        let source = library.join("RJ000001");
        let destination = temp.path().join("export");
        fs::create_dir_all(&source)?;
        fs::create_dir(&destination)?;
        let private = temp.path().join("private");
        fs::write(&private, b"private")?;
        std::os::unix::fs::symlink(&private, source.join("link"))?;
        assert!(export_directory(&source, &library, &destination).is_err());
        assert!(!destination.join("RJ000001").exists());
        assert_eq!(fs::read(private)?, b"private");
        Ok(())
    }
}

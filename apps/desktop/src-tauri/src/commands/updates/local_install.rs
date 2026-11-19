//! Installation touches only the application distribution, never the profile.
#[cfg(target_os = "linux")]
pub(super) fn replace_appimage(target: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::{fs, io::Write};
    let target = target.canonicalize()?;
    let permissions = fs::metadata(&target)?.permissions();
    let mut staged = tempfile::NamedTempFile::new_in(
        target
            .parent()
            .ok_or_else(|| std::io::Error::other("Missing installation directory"))?,
    )?;
    staged.write_all(bytes)?;
    staged.as_file().set_permissions(permissions)?;
    staged.as_file().sync_all()?;
    staged.persist(&target).map_err(|error| error.error)?;
    // Persist the directory entry as well as the file contents.
    fs::File::open(target.parent().unwrap())?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
pub(super) fn launch_installer(bytes: &[u8]) -> std::io::Result<()> {
    use std::{io::Write, process::Command};
    let directory = tempfile::Builder::new()
        .prefix("dldr-local-update-")
        .tempdir()?;
    let installer = directory.path().join("DLDR-setup.exe");
    let mut file = std::fs::File::create(&installer)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    // DLDR uses a per-user NSIS installer. /R restarts the installed application;
    // /UPDATE preserves the install location and /P keeps progress visible.
    Command::new(&installer)
        .args(["/P", "/UPDATE", "/R"])
        .spawn()?;
    // The installer must outlive this process, like Tauri's standard updater.
    let _workspace = directory.keep();
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn replacement_is_atomic_and_keeps_executable_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("DLDR.AppImage");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        replace_appimage(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(replace_appimage(&directory.path().join("missing"), b"bad").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }
}

//! A single writer/coordinator per application data directory, shared by GUI and CLI.
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

/// GUI and CLI share the same profile: XDG_CONFIG_HOME/dldr on Linux and
/// APPDATA/dldr on Windows. Resolving it never creates or migrates any data.
pub fn profile_dir() -> io::Result<PathBuf> {
    dirs::config_dir()
        .map(|root| root.join("dldr"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "DLDR configuration directory unavailable",
            )
        })
}

pub struct LibraryInstanceGuard {
    _file: File,
}

impl LibraryInstanceGuard {
    pub fn acquire(data_dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(data_dir.join("runtime.lock"))?;
        FileExt::try_lock_exclusive(&file).map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            {
                io::Error::new(io::ErrorKind::WouldBlock,
                    "Another GUI or dldr command is using this library. Close it before logging in, syncing or downloading; search, show and status still work.")
            } else { error }
        })?;
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusive_until_owner_drops_and_independent_per_library() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let first = LibraryInstanceGuard::acquire(a.path()).unwrap();
        assert_eq!(
            LibraryInstanceGuard::acquire(a.path())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        let _other = LibraryInstanceGuard::acquire(b.path()).unwrap();
        drop(first);
        assert!(LibraryInstanceGuard::acquire(a.path()).is_ok());
    }
}

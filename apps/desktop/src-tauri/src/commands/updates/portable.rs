//! Windows portable updates use the verified new executable as a short-lived
//! helper. It waits for the old process handle, backs up files and rolls back on
//! any replacement/startup error. It never changes the profile or media folders.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Component, Path, PathBuf},
};

const EXECUTABLE: &str = "DLDR.Desktop.exe";

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Serialize, Deserialize)]
struct Manifest {
    parent_pid: u32,
    target: PathBuf,
    workspace: PathBuf,
}

fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|part| match part {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                !name.contains([':', '\\']) && !name.ends_with(['.', ' '])
            }
            _ => false,
        })
}

fn extract(bytes: &[u8], workspace: &Path, version: &str) -> io::Result<()> {
    let mut archive = zip::ZipArchive::new(io::Cursor::new(bytes))?;
    let mut names = BTreeSet::new();
    let mut expanded = 0u64;
    if archive.len() > 20_000 {
        return Err(invalid("Too many update files"));
    }
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry
            .enclosed_name()
            .ok_or_else(|| invalid("Unsafe update path"))?;
        let name: PathBuf = name.components().collect();
        if !safe_relative(&name)
            || !name.starts_with("DLDR")
            || entry.is_symlink()
            || !names.insert(name.to_string_lossy().to_lowercase())
        {
            return Err(invalid("Unsafe or duplicate update entry"));
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or_else(|| invalid("Update is too large"))?;
        if expanded > 8 * 1024 * 1024 * 1024 {
            return Err(invalid("Update is too large"));
        }
        let destination = workspace.join(name);
        if entry.is_dir() {
            fs::create_dir_all(destination)?;
        } else {
            fs::create_dir_all(
                destination
                    .parent()
                    .ok_or_else(|| invalid("Invalid update path"))?,
            )?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?;
            io::copy(&mut entry, &mut file)?;
            file.sync_all()?;
        }
    }
    let payload = workspace.join("DLDR");
    for executable in [EXECUTABLE, "dldr.exe"] {
        use io::Read;
        let mut header = [0u8; 2];
        fs::File::open(payload.join(executable))?.read_exact(&mut header)?;
        if header != *b"MZ" {
            return Err(invalid("Invalid Windows executable"));
        }
    }
    let info: serde_json::Value =
        serde_json::from_slice(&fs::read(payload.join("build-info.json"))?)?;
    if info["version"].as_str() != Some(version) {
        return Err(invalid("Update version mismatch"));
    }
    Ok(())
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

fn files_under(root: &Path, directory: &Path, result: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if is_link(&metadata) {
            return Err(invalid("Linked update paths are not allowed"));
        }
        if metadata.is_dir() {
            files_under(root, &path, result)?;
        } else if metadata.is_file() {
            result.push(
                path.strip_prefix(root)
                    .map_err(io::Error::other)?
                    .to_path_buf(),
            );
        } else {
            return Err(invalid("Invalid update file"));
        }
    }
    Ok(())
}

fn check_destination(root: &Path, relative: &Path) -> io::Result<()> {
    if !safe_relative(relative) {
        return Err(invalid("Unsafe update destination"));
    }
    let mut current = root.to_path_buf();
    for part in relative.components() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_link(&metadata) => return Err(invalid("Linked update destination")),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

struct Replacement {
    target: PathBuf,
    backup: PathBuf,
    changes: Vec<(PathBuf, bool)>,
    committed: bool,
}

impl Replacement {
    fn new(target: &Path) -> io::Result<Self> {
        let backup = target.join(format!(".dldr-update-backup-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&backup)?;
        Ok(Self {
            target: target.to_path_buf(),
            backup,
            changes: Vec::new(),
            committed: false,
        })
    }

    fn apply(
        &mut self,
        payload: &Path,
        mut copy: impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut files = Vec::new();
        files_under(payload, payload, &mut files)?;
        files.sort();
        for relative in files {
            check_destination(&self.target, &relative)?;
            let target = self.target.join(&relative);
            let backup = self.backup.join(&relative);
            let existed = target.try_exists()?;
            if existed {
                if !target.is_file() {
                    return Err(invalid("Update destination is not a regular file"));
                }
                fs::create_dir_all(
                    backup
                        .parent()
                        .ok_or_else(|| invalid("Invalid backup path"))?,
                )?;
                fs::rename(&target, backup)?;
            }
            self.changes.push((relative.clone(), existed));
            fs::create_dir_all(
                target
                    .parent()
                    .ok_or_else(|| invalid("Invalid destination"))?,
            )?;
            copy(&payload.join(relative), &target)?;
        }
        Ok(())
    }

    fn rollback(&mut self) -> io::Result<()> {
        while let Some((relative, existed)) = self.changes.last().cloned() {
            let target = self.target.join(&relative);
            if target.try_exists()? {
                fs::remove_file(&target)?;
            }
            if existed {
                fs::rename(self.backup.join(&relative), &target)?;
            }
            self.changes.pop();
        }
        self.changes.clear();
        self.committed = true;
        fs::remove_dir_all(&self.backup)
    }

    fn commit(mut self) {
        self.committed = true;
        let _ = fs::remove_dir_all(&self.backup);
    }
}

impl Drop for Replacement {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.rollback();
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{os::windows::process::CommandExt, process::Command, time::Duration};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };

    struct Parent(HANDLE);
    impl Parent {
        fn open(pid: u32) -> io::Result<Self> {
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            if handle.is_null() {
                Err(io::Error::last_os_error())
            } else {
                Ok(Self(handle))
            }
        }
        fn wait(&self, milliseconds: u32) -> bool {
            unsafe { WaitForSingleObject(self.0, milliseconds) == WAIT_OBJECT_0 }
        }
    }
    impl Drop for Parent {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn cache_dir() -> io::Result<PathBuf> {
        Ok(dldr_library::profile_dir()?.join("updates"))
    }

    pub(crate) async fn launch(bytes: &[u8], version: &str) -> io::Result<()> {
        let cache = cache_dir()?;
        fs::create_dir_all(&cache)?;
        let workspace = tempfile::Builder::new()
            .prefix("update-")
            .tempdir_in(cache)?;
        extract(bytes, workspace.path(), version)?;
        let executable = std::env::current_exe()?.canonicalize()?;
        let target = executable
            .parent()
            .ok_or_else(|| invalid("Missing portable directory"))?
            .to_path_buf();
        let manifest = Manifest {
            parent_pid: std::process::id(),
            target,
            workspace: workspace.path().canonicalize()?,
        };
        let manifest_path = workspace.path().join("update.json");
        fs::write(&manifest_path, serde_json::to_vec(&manifest)?)?;
        let mut helper = Command::new(workspace.path().join("DLDR").join(EXECUTABLE))
            .arg("--dldr-apply-update")
            .arg(&manifest_path)
            .creation_flags(0x08000000)
            .spawn()?;
        for _ in 0..120 {
            if workspace.path().join("ready").is_file() {
                let _ = workspace.keep();
                return Ok(());
            }
            if helper.try_wait()?.is_some() {
                return Err(invalid("Portable update helper could not start"));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let _ = helper.kill();
        let _ = helper.wait();
        Err(invalid("Portable update helper timed out"))
    }

    fn apply(manifest: &Manifest) -> io::Result<()> {
        let payload = manifest.workspace.join("DLDR");
        if std::env::current_exe()?.canonicalize()? != payload.join(EXECUTABLE).canonicalize()?
            || manifest.target == payload
            || !manifest.target.is_dir()
        {
            return Err(invalid("Invalid helper context"));
        }
        // Open the process handle before declaring readiness; PID reuse cannot
        // cause us to wait for a different program or replace running binaries.
        let parent = Parent::open(manifest.parent_pid)?;
        let mut transaction = Replacement::new(&manifest.target)?;
        fs::write(manifest.workspace.join("ready"), b"ready")?;
        if !parent.wait(120_000) {
            return Err(invalid("Old DLDR process did not exit"));
        }
        let result = transaction
            .apply(&payload, |source, target| {
                fs::copy(source, target).map(|_| ())
            })
            .and_then(|_| {
                Command::new(manifest.target.join(EXECUTABLE))
                    .current_dir(&manifest.target)
                    .spawn()
                    .map(|_| ())
            });
        match result {
            Ok(()) => {
                transaction.commit();
                Ok(())
            }
            Err(error) => {
                transaction.rollback()?;
                let _ = Command::new(manifest.target.join(EXECUTABLE))
                    .current_dir(&manifest.target)
                    .spawn();
                Err(error)
            }
        }
    }

    pub(crate) fn run_if_requested() -> bool {
        let mut args = std::env::args_os().skip(1);
        if args.next().as_deref() != Some(std::ffi::OsStr::new("--dldr-apply-update")) {
            return false;
        }
        let Some(path) = args.next() else {
            return true;
        };
        let result = fs::read(&path)
            .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).map_err(io::Error::other))
            .and_then(|manifest| apply(&manifest));
        if let Err(error) = result {
            if let Some(folder) = Path::new(&path).parent() {
                let _ = fs::write(folder.join("error.txt"), error.to_string());
                if folder.join("ready").is_file() {
                    let message:Vec<u16> = "DLDR could not finish updating. The previous files were restored when possible. Check Settings > Logs; any remaining backup is in the application folder.\0".encode_utf16().collect();
                    let title: Vec<u16> = "DLDR update\0".encode_utf16().collect();
                    unsafe {
                        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                            std::ptr::null_mut(),
                            message.as_ptr(),
                            title.as_ptr(),
                            0x10,
                        );
                    }
                }
            }
        }
        true
    }

    pub(crate) fn cleanup(audit: dldr_audit::AuditLogger) {
        // The new app starts just before its helper exits. Retry in the
        // background so Windows can release the helper EXE and DLL handles.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            let Ok(cache) = cache_dir() else {
                return;
            };
            let Ok(entries) = fs::read_dir(cache) else {
                return;
            };
            for entry in entries.flatten() {
                if !entry.file_name().to_string_lossy().starts_with("update-") {
                    continue;
                }
                let folder = entry.path();
                let manifest = fs::read(folder.join("update.json"))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok());
                let Some(manifest) = manifest else {
                    continue;
                };
                if folder.canonicalize().ok().as_ref() != Some(&manifest.workspace)
                    || Parent::open(manifest.parent_pid).is_ok_and(|parent| !parent.wait(0))
                {
                    continue;
                }
                if let Ok(error) = fs::read_to_string(folder.join("error.txt")) {
                    audit.record_now(
                        dldr_audit::AuditEvent::failed(
                            "native.updater",
                            "Portable update failed; previous files were restored when possible",
                        )
                        .with_error(Some("filesystem"), error),
                    );
                }
                for _ in 0..5 {
                    if fs::remove_dir_all(&folder).is_ok() {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        });
    }
}

#[cfg(windows)]
pub(crate) use windows::{cleanup, launch, run_if_requested};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn package(extra: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        for (name, data) in [
            ("DLDR/DLDR.Desktop.exe", b"MZnew".as_slice()),
            ("DLDR/dldr.exe", b"MZcli".as_slice()),
            ("DLDR/build-info.json", br#"{"version":"0.2.0"}"#.as_slice()),
        ]
        .into_iter()
        .chain(extra.iter().copied())
        {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn extracts_a_complete_matching_package_and_rejects_bad_archives() {
        let temporary = tempfile::tempdir().unwrap();
        extract(
            &package(&[("DLDR/third-party/中文 日本語.txt", b"license")]),
            temporary.path(),
            "0.2.0",
        )
        .unwrap();
        assert_eq!(
            fs::read(temporary.path().join("DLDR/third-party/中文 日本語.txt")).unwrap(),
            b"license"
        );
        for bytes in [
            package(&[("../escape", b"bad")]),
            package(&[("DLDR/DLDR.desktop.exe", b"MZduplicate")]),
            b"truncated ZIP".to_vec(),
        ] {
            let temporary = tempfile::tempdir().unwrap();
            assert!(extract(&bytes, temporary.path(), "0.2.0").is_err());
        }
        let temporary = tempfile::tempdir().unwrap();
        assert!(extract(&package(&[]), temporary.path(), "0.3.0").is_err());
    }

    #[test]
    fn rejects_traversal_and_windows_path_aliases() {
        for path in [
            "../app.exe",
            "/app.exe",
            "DLDR/../outside",
            "DLDR/file:stream",
            "DLDR/file.",
            "DLDR/file ",
            "DLDR\\file",
        ] {
            assert!(!safe_relative(Path::new(path)), "{path}");
        }
        assert!(safe_relative(Path::new("third-party/中文 日本語/OFL.txt")));
    }

    #[test]
    fn rolls_back_partial_replacement_and_keeps_unrelated_files() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("installed");
        let payload = temporary.path().join("new");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&payload).unwrap();
        for name in ["a.exe", "b.dll"] {
            fs::write(target.join(name), b"old").unwrap();
            fs::write(payload.join(name), b"new").unwrap();
        }
        fs::write(target.join("personal.txt"), b"keep").unwrap();
        fs::write(payload.join("a-new.txt"), b"new file").unwrap();
        let mut transaction = Replacement::new(&target).unwrap();
        assert!(transaction
            .apply(&payload, |source, target| {
                if source.ends_with("b.dll") {
                    fs::write(target, b"partial")?;
                    return Err(invalid("disk full"));
                }
                fs::copy(source, target).map(|_| ())
            })
            .is_err());
        transaction.rollback().unwrap();
        for name in ["a.exe", "b.dll"] {
            assert_eq!(fs::read(target.join(name)).unwrap(), b"old");
        }
        assert_eq!(fs::read(target.join("personal.txt")).unwrap(), b"keep");
        assert!(!target.join("a-new.txt").exists());
    }

    #[test]
    fn commit_keeps_new_files_and_removes_backup() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("installed");
        let payload = temporary.path().join("new");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(payload.join("licenses")).unwrap();
        fs::write(target.join("app.exe"), b"old").unwrap();
        fs::write(payload.join("app.exe"), b"new").unwrap();
        fs::write(payload.join("licenses/OFL.txt"), b"license").unwrap();
        let mut transaction = Replacement::new(&target).unwrap();
        let backup = transaction.backup.clone();
        transaction
            .apply(&payload, |source, target| {
                fs::copy(source, target).map(|_| ())
            })
            .unwrap();
        transaction.commit();
        assert!(!backup.exists());
        assert_eq!(fs::read(target.join("app.exe")).unwrap(), b"new");
        assert_eq!(
            fs::read(target.join("licenses/OFL.txt")).unwrap(),
            b"license"
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_linked_destination_directories() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("installed");
        let outside = temporary.path().join("outside");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, target.join("licenses")).unwrap();
        assert!(check_destination(&target, Path::new("licenses/OFL.txt")).is_err());
        assert!(!outside.join("OFL.txt").exists());
    }
}

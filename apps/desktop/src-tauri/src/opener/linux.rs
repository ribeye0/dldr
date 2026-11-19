//! Launch host desktop handlers without leaking AppImage libraries into them.
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io,
    path::Path,
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

type Environment = BTreeMap<OsString, OsString>;

fn host_environment(mut environment: Environment) -> Environment {
    let Some(appdir) = environment
        .get(OsStr::new("APPDIR"))
        .filter(|p| !p.is_empty())
        .cloned()
    else {
        return environment;
    };
    let appdir = Path::new(&appdir);
    // AppRun prepends bundle paths and exports GTK/GStreamer module locations.
    // Keep user/desktop paths, display sockets and D-Bus credentials unchanged.
    for key in [
        "PATH",
        "LD_LIBRARY_PATH",
        "PYTHONPATH",
        "PERLLIB",
        "GTK_PATH",
        "GIO_EXTRA_MODULES",
        "GI_TYPELIB_PATH",
        "XDG_DATA_DIRS",
        "XDG_CONFIG_DIRS",
        "GST_PLUGIN_PATH",
        "GST_PLUGIN_SYSTEM_PATH",
        "FONTCONFIG_PATH",
        "QT_PLUGIN_PATH",
        "QML2_IMPORT_PATH",
        "GST_PLUGIN_PATH_1_0",
        "GST_PLUGIN_SYSTEM_PATH_1_0",
    ] {
        if let Some(value) = environment.get(OsStr::new(key)) {
            let paths: Vec<_> = std::env::split_paths(value)
                .filter(|p| !p.as_os_str().is_empty() && !p.starts_with(appdir))
                .collect();
            if paths.is_empty() {
                environment.remove(OsStr::new(key));
            } else if let Ok(value) = std::env::join_paths(paths) {
                environment.insert(key.into(), value);
            }
        }
    }
    for key in [
        "PYTHONHOME",
        "GIO_MODULE_DIR",
        "GTK_EXE_PREFIX",
        "GTK_DATA_PREFIX",
        "GTK_IM_MODULE_FILE",
        "GDK_PIXBUF_MODULE_FILE",
        "GDK_PIXBUF_MODULEDIR",
        "GSETTINGS_SCHEMA_DIR",
        "FONTCONFIG_FILE",
        "GST_PLUGIN_SCANNER",
        "GST_PLUGIN_SCANNER_1_0",
        "GST_PTP_HELPER_1_0",
        "GST_REGISTRY",
        "GST_REGISTRY_1_0",
    ] {
        if environment
            .get(OsStr::new(key))
            .is_some_and(|p| Path::new(p).starts_with(appdir))
        {
            environment.remove(OsStr::new(key));
        }
    }
    if let Some(value) = environment.get(OsStr::new("LD_PRELOAD")) {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let entries: Vec<_> = value
            .as_bytes()
            .split(|b| *b == b':' || b.is_ascii_whitespace())
            .filter(|p| !p.is_empty() && !Path::new(OsStr::from_bytes(p)).starts_with(appdir))
            .collect();
        if entries.is_empty() {
            environment.remove(OsStr::new("LD_PRELOAD"));
        } else {
            environment.insert("LD_PRELOAD".into(), OsString::from_vec(entries.join(&b':')));
        }
    }
    for key in ["APPDIR", "APPIMAGE", "ARGV0", "GTK_THEME"] {
        environment.remove(OsStr::new(key));
    }
    environment
}

pub(super) async fn open(target: &OsStr) -> io::Result<()> {
    launch(
        target,
        &host_environment(std::env::vars_os().collect()),
        Duration::from_secs(10),
    )
    .await
}

async fn launch(target: &OsStr, environment: &Environment, timeout: Duration) -> io::Result<()> {
    let mut failures = Vec::new();
    let mut failure_details = serde_json::json!({});
    // gio hands off to the desktop service; xdg-open can wait for a new window to close.
    for (program, args) in [("gio", &["open"][..]), ("xdg-open", &[][..])] {
        let mut command = Command::new(program);
        command
            .args(args)
            .arg(target)
            .current_dir("/")
            .env_clear()
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                failure_details = serde_json::json!({"osCode": error.raw_os_error()});
                failures.push(format!("{program}: {error}"));
                continue;
            }
        };
        match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) if status.success() => return Ok(()),
            Ok(Ok(status)) => {
                failure_details = serde_json::json!({"exitCode": status.code()});
                failures.push(format!("{program}: {status}"));
            }
            Ok(Err(error)) => {
                failure_details = serde_json::json!({"osCode": error.raw_os_error()});
                failures.push(format!("{program}: {error}"));
            }
            Err(_) => {
                let _ = child.kill().await;
                dldr_audit::record_current_failure(
                    "timeout",
                    serde_json::json!({"failureKind": "opener", "isTimeout": true}),
                );
                // It may already have opened a window; do not dispatch twice.
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "{program} did not finish the desktop open request within {} seconds",
                        timeout.as_secs()
                    ),
                ));
            }
        }
    }
    dldr_audit::record_current_failure("opener", failure_details);
    Err(io::Error::other(format!(
        "No desktop handler accepted the request ({})",
        failures.join("; ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    fn environment(values: &[(&str, &str)]) -> Environment {
        values
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect()
    }

    #[test]
    fn removes_bundle_paths_but_keeps_host_environment_and_similar_prefixes() {
        let original = environment(&[
            ("APPDIR", "/tmp/DLDR.AppDir"),
            ("APPIMAGE", "/home/user/DLDR.AppImage"),
            ("PATH", "/tmp/DLDR.AppDir/usr/bin:/custom/bin:/usr/bin"),
            (
                "LD_LIBRARY_PATH",
                "/tmp/DLDR.AppDir/usr/lib:/tmp/DLDR.AppDir-old/lib:/host/lib:",
            ),
            (
                "LD_PRELOAD",
                "/tmp/DLDR.AppDir/lib/a.so /host/b.so:libcustom.so",
            ),
            ("GIO_MODULE_DIR", "/tmp/DLDR.AppDir//usr/lib/gio/modules"),
            ("PYTHONHOME", "/tmp/DLDR.AppDir/usr/"),
            (
                "PYTHONPATH",
                "/tmp/DLDR.AppDir/usr/share/pyshared:/host/python",
            ),
            ("PERLLIB", "/tmp/DLDR.AppDir/usr/share/perl5:/host/perl"),
            ("GSETTINGS_SCHEMA_DIR", "/tmp/DLDR.AppDir/usr/share/schemas"),
            ("GDK_PIXBUF_MODULE_FILE", "/host/loaders.cache"),
            (
                "GST_PLUGIN_PATH_1_0",
                "/tmp/DLDR.AppDir/usr/lib/gstreamer-1.0",
            ),
            (
                "XDG_DATA_DIRS",
                "/tmp/DLDR.AppDir/usr/share:/usr/share:/custom/share",
            ),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("XDG_CURRENT_DESKTOP", "niri"),
        ]);
        let clean = host_environment(original.clone());
        for (key, value) in [
            ("PATH", "/custom/bin:/usr/bin"),
            ("PYTHONPATH", "/host/python"),
            ("PERLLIB", "/host/perl"),
            ("LD_LIBRARY_PATH", "/tmp/DLDR.AppDir-old/lib:/host/lib"),
            ("LD_PRELOAD", "/host/b.so:libcustom.so"),
            ("XDG_DATA_DIRS", "/usr/share:/custom/share"),
            ("GDK_PIXBUF_MODULE_FILE", "/host/loaders.cache"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("XDG_CURRENT_DESKTOP", "niri"),
        ] {
            assert_eq!(
                clean.get(OsStr::new(key)),
                Some(&OsString::from(value)),
                "{key}"
            );
        }
        for key in [
            "APPDIR",
            "APPIMAGE",
            "PYTHONHOME",
            "GIO_MODULE_DIR",
            "GSETTINGS_SCHEMA_DIR",
            "GST_PLUGIN_PATH_1_0",
        ] {
            assert!(!clean.contains_key(OsStr::new(key)), "{key}");
        }
        let mut native = original;
        native.remove(OsStr::new("APPDIR"));
        assert_eq!(host_environment(native.clone()), native);
    }

    fn helper(directory: &Path, name: &str, script: &str) {
        let path = directory.join(name);
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[tokio::test]
    async fn checks_exit_status_and_falls_back_with_one_literal_unicode_argument() {
        let dir = tempfile::tempdir().unwrap();
        let env = BTreeMap::from([
            (OsString::from("PATH"), dir.path().as_os_str().to_owned()),
            (
                OsString::from("QA_OUTPUT"),
                dir.path().join("args").into_os_string(),
            ),
        ]);
        helper(dir.path(), "gio", "exit 127");
        helper(
            dir.path(),
            "xdg-open",
            "printf '%s\\n' \"$#\" \"$1\" \"$2\" > \"$QA_OUTPUT\"",
        );
        let target = "/tmp/中文 日本語/$(touch unsafe);音聲";
        launch(OsStr::new(target), &env, Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("args")).unwrap(),
            format!("1\n{target}\n\n")
        );
        helper(dir.path(), "xdg-open", "exit 2");
        let error = launch(OsStr::new(target), &env, Duration::from_secs(2))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("127") && error.to_string().contains("2"));
        fs::remove_file(dir.path().join("gio")).unwrap();
        helper(dir.path(), "xdg-open", "exit 0");
        launch(OsStr::new(target), &env, Duration::from_secs(2))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn stuck_launcher_times_out_instead_of_blocking_or_opening_twice() {
        let dir = tempfile::tempdir().unwrap();
        let env = BTreeMap::from([(OsString::from("PATH"), dir.path().as_os_str().to_owned())]);
        helper(dir.path(), "gio", "while :; do :; done");
        helper(dir.path(), "xdg-open", "exit 0");
        let error = launch(OsStr::new("/tmp"), &env, Duration::from_millis(30))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
}

//! Signed application updates. Only the main DLDR webview can invoke these commands.
use crate::AppState;
use base64::Engine;
use serde::Serialize;
use std::time::Duration;
#[cfg(target_os = "linux")]
use tauri::Manager;
use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[cfg(any(windows, test))]
pub(crate) mod portable;

#[derive(Default)]
pub(crate) struct PendingUpdate(tokio::sync::Mutex<Option<Update>>);

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum PackageKind {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    AppImage,
    #[cfg_attr(not(windows), allow(dead_code))]
    WindowsInstaller,
    WindowsPortable,
    Unsupported,
}

#[cfg(target_os = "linux")]
fn package_kind(app: &AppHandle) -> PackageKind {
    if app.env().appimage.is_some() {
        PackageKind::AppImage
    } else {
        PackageKind::Unsupported
    }
}

#[cfg(windows)]
fn package_kind(_app: &AppHandle) -> PackageKind {
    let Ok(executable) = std::env::current_exe() else {
        return PackageKind::Unsupported;
    };
    let Some(directory) = executable.parent() else {
        return PackageKind::Unsupported;
    };
    let name = executable.file_name().unwrap_or_default().to_string_lossy();
    if name.eq_ignore_ascii_case("DLDR.Desktop.exe") && directory.join("build-info.json").is_file()
    {
        PackageKind::WindowsPortable
    } else if name.eq_ignore_ascii_case("dldr-desktop.exe")
        && directory.join("uninstall.exe").is_file()
    {
        PackageKind::WindowsInstaller
    } else {
        PackageKind::Unsupported
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn package_kind(_app: &AppHandle) -> PackageKind {
    PackageKind::Unsupported
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateCheck {
    kind: PackageKind,
    release: Option<Release>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Release {
    current_version: String,
    version: String,
    notes: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub(crate) enum UpdateProgress {
    Progress { downloaded: u64, total: Option<u64> },
    Verifying,
    Installing,
}

#[tauri::command]
pub(crate) async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingUpdate>,
) -> Result<UpdateCheck, String> {
    state
        .audit
        .operation("native.updater")
        .command(async {
            let mut pending = pending.0.try_lock().map_err(|_| "update_busy")?;
            *pending = None;
            let kind = package_kind(&app);
            if kind == PackageKind::Unsupported {
                return Ok(UpdateCheck {
                    kind,
                    release: None,
                });
            }
            let mut builder = app.updater_builder().timeout(Duration::from_secs(30));
            if kind == PackageKind::WindowsPortable {
                builder = builder.target("windows-portable-x86_64");
            }
            let update = builder
                .build()
                .map_err(update_error)?
                .check()
                .await
                .map_err(update_error)?;
            let release = update.as_ref().map(|update| Release {
                current_version: update.current_version.clone(),
                version: update.version.clone(),
                notes: update.body.clone().unwrap_or_default(),
            });
            *pending = update;
            Ok(UpdateCheck { kind, release })
        })
        .await
}

fn update_error(error: tauri_plugin_updater::Error) -> String {
    // Details stay in the native log; the interface receives a stable localized category.
    tracing::warn!(target: "dldr::updates", error = %error, "Application update failed");
    use tauri_plugin_updater::Error;
    match error {
        Error::Minisign(_) | Error::SignatureUtf8(_) | Error::Base64(_) => "update_signature",
        Error::Io(_) => "update_filesystem",
        Error::ReleaseNotFound | Error::TargetsNotFound(_) | Error::TargetNotFound(_) => {
            "update_feed_unavailable"
        }
        _ => "update_network",
    }
    .to_owned()
}

fn signed_version_matches(signature: &str, version: &str) -> bool {
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(signature) else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(&decoded) else {
        return false;
    };
    let Some(comment) = text
        .lines()
        .nth(2)
        .and_then(|line| line.strip_prefix("trusted comment: "))
    else {
        return false;
    };
    comment
        .split('\t')
        .filter_map(|field| field.strip_prefix("version:"))
        .collect::<Vec<_>>()
        == [version]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_replayed_package_with_a_different_announced_version() {
        let signature = |comment: &str| {
            base64::engine::general_purpose::STANDARD.encode(format!(
                "untrusted comment: test\nsignature\ntrusted comment: {comment}\nglobal\n"
            ))
        };
        assert!(signed_version_matches(
            &signature("timestamp:123\tversion:0.2.0"),
            "0.2.0"
        ));
        assert!(!signed_version_matches(
            &signature("timestamp:123\tversion:0.1.0"),
            "0.2.0"
        ));
        assert!(!signed_version_matches(
            &signature("version:0.2.0\tversion:0.1.0"),
            "0.2.0"
        ));
        assert!(!signed_version_matches(
            &signature("timestamp:123"),
            "0.2.0"
        ));
        assert!(!signed_version_matches("invalid", "0.2.0"));
    }
}

#[tauri::command]
pub(crate) async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingUpdate>,
    on_event: Channel<UpdateProgress>,
) -> Result<(), String> {
    state.audit.operation("native.updater").command(async {
        // All download/sync admissions share this lock. Keep it until restart so
        // another Play window cannot start a job between validation and installation.
        let _admission = state.mutation_lock.try_write().map_err(|_| "update_jobs_active")?;
        if state.jobs.list_jobs().iter().any(|job| job.status.is_active()) {
            return Err("update_jobs_active".to_owned());
        }
        let mut pending = pending.0.try_lock().map_err(|_| "update_busy")?;
        let mut update = pending.take().ok_or("update_missing")?;
        // The short metadata timeout must not cut off a large package download.
        update.timeout = Some(Duration::from_secs(60 * 60));
        let mut downloaded = 0u64;
        let bytes = update.download(|chunk, total| {
            downloaded = downloaded.saturating_add(chunk as u64);
            let _ = on_event.send(UpdateProgress::Progress { downloaded, total });
        }, || { let _ = on_event.send(UpdateProgress::Verifying); }).await.map_err(update_error)?;
        // download() verifies the trusted comment as well as the package.
        if !signed_version_matches(&update.signature, &update.version) {
            return Err("update_signature".to_owned());
        }
        let _ = on_event.send(UpdateProgress::Installing);
        #[cfg(windows)]
        if package_kind(&app) == PackageKind::WindowsPortable {
            portable::launch(&bytes, &update.version).await.map_err(|error| {
                tracing::warn!(target: "dldr::updates", %error, "Portable update preparation failed");
                "update_filesystem".to_owned()
            })?;
            state.audit.record_now(dldr_audit::AuditEvent::succeeded("native.updater", "Verified portable update; handing over to replacement helper"));
            state.audit.flush(Duration::from_secs(2));
            app.exit(0);
            return Ok(());
        }
        // download() has verified the signature before any installed bytes change.
        state.audit.record_now(dldr_audit::AuditEvent::succeeded("native.updater", "Verified update package; installing and restarting"));
        state.audit.flush(Duration::from_secs(2));
        update.install(bytes).map_err(update_error)?;
        app.restart();
    }).await
}

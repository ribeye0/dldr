//! Background package preparation and explicit, guarded installation.
use crate::AppState;
use base64::Engine;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
#[cfg(target_os = "linux")]
use tauri::Manager;
use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

#[cfg(any(windows, test))]
pub(crate) mod portable;

mod local_install;
mod packages;
mod session;
use packages::{PreparedPackage, ReadyPackage};
use session::cancellable;
pub(crate) use session::PendingUpdate;

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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Release {
    current_version: String,
    version: String,
    notes: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub(crate) enum UpdateProgress {
    Progress { downloaded: u64, total: Option<u64> },
    Verifying,
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
            let id = uuid::Uuid::new_v4().to_string();
            let cancel = {
                let mut session = pending.0.lock().await;
                let cancel = session.begin(&id)?;
                session.candidate = None;
                session.prepared = None;
                cancel
            };
            let kind = package_kind(&app);
            let result = cancellable(cancel, async {
                if kind == PackageKind::Unsupported {
                    return Ok(None);
                }
                let mut builder = app.updater_builder().timeout(Duration::from_secs(30));
                if kind == PackageKind::WindowsPortable {
                    builder = builder.target("windows-portable-x86_64");
                }
                builder
                    .build()
                    .map_err(update_error)?
                    .check()
                    .await
                    .map_err(update_error)
            })
            .await;
            let mut session = pending.0.lock().await;
            session.finish(&id)?;
            let update = result?;
            let release = update.as_ref().map(|update| Release {
                current_version: update.current_version.clone(),
                version: update.version.clone(),
                notes: update.body.clone().unwrap_or_default(),
            });
            session.candidate = update;
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
pub(crate) async fn download_app_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
    transfer_id: String,
    on_event: Channel<UpdateProgress>,
) -> Result<ReadyPackage, String> {
    let (mut update, cancel) = {
        let mut session = pending.0.lock().await;
        let update = session.candidate.clone().ok_or("update_missing")?;
        let cancel = session.begin(&transfer_id)?;
        session.prepared = None;
        (update, cancel)
    };
    update.timeout = Some(Duration::from_secs(60 * 60));
    let mut downloaded = 0u64;
    let result = cancellable(cancel, async {
        let bytes = update
            .download(
                |chunk, total| {
                    downloaded = downloaded.saturating_add(chunk as u64);
                    let _ = on_event.send(UpdateProgress::Progress { downloaded, total });
                },
                || {
                    let _ = on_event.send(UpdateProgress::Verifying);
                },
            )
            .await
            .map_err(update_error)?;
        if !signed_version_matches(&update.signature, &update.version) {
            return Err("update_signature".into());
        }
        let file_name = update
            .download_url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("DLDR")
            .to_owned();
        let info = packages::info(
            &update.current_version,
            &update.version,
            file_name,
            bytes.len() as u64,
            "online",
            true,
            update.body.clone().unwrap_or_default(),
        )?;
        Ok(PreparedPackage {
            transfer_id: transfer_id.clone(),
            kind: package_kind(&app),
            info,
            bytes,
            automatic: Some(update.clone()),
        })
    })
    .await;
    let mut session = pending.0.lock().await;
    session.finish(&transfer_id)?;
    let package = result?;
    let info = package.info.clone();
    session.prepared = Some(Arc::new(package));
    Ok(info)
}

#[tauri::command]
pub(crate) async fn cancel_app_update(
    pending: State<'_, PendingUpdate>,
    transfer_id: String,
) -> Result<(), String> {
    pending.0.lock().await.cancel(&transfer_id);
    Ok(())
}

#[tauri::command]
pub(crate) async fn prepare_local_app_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
    transfer_id: String,
    path: String,
    version: Option<String>,
) -> Result<ReadyPackage, String> {
    let kind = package_kind(&app);
    if kind == PackageKind::Unsupported {
        return Err("update_unsupported".into());
    }
    let cancel = {
        let mut session = pending.0.lock().await;
        let cancel = session.begin(&transfer_id)?;
        session.prepared = None;
        cancel
    };
    let pubkey = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v["pubkey"].as_str())
        .unwrap_or_default()
        .to_owned();
    let result = cancellable(
        cancel,
        packages::prepare_local(
            transfer_id.clone(),
            path,
            version,
            kind,
            app.package_info().version.to_string(),
            pubkey,
        ),
    )
    .await;
    let mut session = pending.0.lock().await;
    session.finish(&transfer_id)?;
    let package = result?;
    let info = package.info.clone();
    session.prepared = Some(Arc::new(package));
    Ok(info)
}

#[tauri::command]
pub(crate) async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingUpdate>,
    trust_local: bool,
) -> Result<(), String> {
    state
        .audit
        .operation("native.updater")
        .command(async {
            // Only installation blocks library mutations. Preparing a package never does.
            let _admission = state
                .mutation_lock
                .try_write()
                .map_err(|_| "update_jobs_active")?;
            if state
                .jobs
                .list_jobs()
                .iter()
                .any(|job| job.status.is_active())
            {
                return Err("update_jobs_active".into());
            }
            let package = {
                let mut session = pending.0.lock().await;
                let package = session.ready(package_kind(&app), trust_local)?;
                session.installing = true;
                package
            };
            let result = install_package(&app, &package).await;
            if let Err(error) = result {
                pending.0.lock().await.installing = false;
                return Err(error);
            }
            state.audit.record_now(dldr_audit::AuditEvent::succeeded(
                "native.updater",
                "Installed application package; restarting",
            ));
            state.audit.flush(Duration::from_secs(2));
            #[cfg(windows)]
            {
                app.exit(0);
                Ok(())
            }
            #[cfg(not(windows))]
            app.restart()
        })
        .await
}

async fn install_package(_app: &AppHandle, package: &PreparedPackage) -> Result<(), String> {
    #[cfg(windows)]
    if package.kind == PackageKind::WindowsPortable {
        return portable::launch(&package.bytes, &package.info.release.version)
            .await
            .map_err(|error| {
                tracing::warn!(target: "dldr::updates", %error, "Portable installation failed");
                "update_filesystem".into()
            });
    }
    if let Some(update) = &package.automatic {
        return update.install(&package.bytes).map_err(update_error);
    }
    #[cfg(target_os = "linux")]
    if package.kind == PackageKind::AppImage {
        return local_install::replace_appimage(
            std::path::Path::new(&_app.env().appimage.ok_or("update_unsupported")?),
            &package.bytes,
        )
        .map_err(|error| {
            tracing::warn!(target: "dldr::updates", %error, "AppImage replacement failed");
            "update_filesystem".into()
        });
    }
    #[cfg(windows)]
    if package.kind == PackageKind::WindowsInstaller {
        return local_install::launch_installer(&package.bytes).map_err(|error| {
            tracing::warn!(target: "dldr::updates", %error, "Installer launch failed");
            "update_filesystem".into()
        });
    }
    Err("update_unsupported".into())
}

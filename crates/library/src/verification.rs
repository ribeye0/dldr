use super::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationItem {
    pub work_id: String,
    pub path: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub checked: usize,
    pub present: usize,
    pub missing: usize,
    pub issues: Vec<VerificationItem>,
}

impl Library {
    pub async fn verify_downloaded_library(
        &self,
        library_root: &Path,
    ) -> Result<VerificationReport> {
        let root = library_root.canonicalize()?;
        if !root.is_dir() {
            return Err(LibraryError::DownloadPathNotDirectory(root));
        }
        let mut report = VerificationReport {
            checked: 0,
            present: 0,
            missing: 0,
            issues: Vec::new(),
        };
        // Snapshot IDs before updates so changing downloaded membership cannot skip a page.
        let mut ids = Vec::new();
        let mut offset = 0;
        loop {
            let page = self
                .list_products(&ProductListQuery {
                    downloaded_only: true,
                    limit: 500,
                    offset,
                    ..Default::default()
                })
                .await?;
            ids.extend(page.products.into_iter().map(|p| p.work_id));
            offset += 500;
            if u64::from(offset) >= page.total_count {
                break;
            }
        }
        for id in ids {
            let lock = self.work_lock(&id);
            let _guard = lock.lock().await;
            let state = self.storage.work_download_state(&id).await?;
            if state.status != WorkDownloadStatus::Downloaded {
                continue;
            }
            let Some(path) = state.local_path.as_deref() else {
                continue;
            };
            report.checked += 1;
            let local = Path::new(path);
            let status = if self.storage.download_finalization(&id).await?.is_some() {
                "recovery_required"
            } else if !local.is_absolute()
                || local
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                "outside_root"
            } else {
                match tokio::fs::symlink_metadata(local).await {
                    Ok(metadata) if metadata.file_type().is_symlink() => "symlink",
                    Ok(metadata) if !metadata.is_dir() => "not_directory",
                    Ok(_) => match local.canonicalize() {
                        Ok(p) if p != root && p.starts_with(&root) => {
                            report.present += 1;
                            continue;
                        }
                        Ok(_) => "outside_root",
                        Err(_) => "unreadable",
                    },
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        // Do not change records if the configured mount disappeared mid-scan.
                        if !root.try_exists()? {
                            return Err(LibraryError::SyncSource(
                                "Library folder became unavailable; verification stopped".into(),
                            ));
                        }
                        let mut ancestor = local.parent();
                        let mut safe = false;
                        while let Some(p) = ancestor {
                            match p.canonicalize() {
                                Ok(p) => {
                                    safe = p.starts_with(&root);
                                    break;
                                }
                                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                    ancestor = p.parent()
                                }
                                Err(_) => break,
                            }
                        }
                        if safe {
                            self.storage.save_work_download(&WorkDownloadUpdate {
                            work_id: id.clone(), status: WorkDownloadStatus::Failed,
                            local_path: state.local_path.clone(), staging_path: state.staging_path,
                            unpack_policy: state.unpack_policy.unwrap_or_else(|| "manual".into()),
                            bytes_received: state.bytes_received, bytes_total: state.bytes_total,
                            error_code: Some("local_missing".into()), error_message: Some("Downloaded folder is missing; restore/move it back or download again".into()),
                            started_at: state.started_at, completed_at: state.completed_at, updated_at: now_string(),
                        }).await?;
                            report.missing += 1;
                            "missing"
                        } else {
                            "unreadable_or_outside_root"
                        }
                    }
                    Err(_) => "unreadable",
                }
            };
            report.issues.push(VerificationItem {
                work_id: id,
                path: path.to_owned(),
                status: status.into(),
            });
        }
        Ok(report)
    }
}

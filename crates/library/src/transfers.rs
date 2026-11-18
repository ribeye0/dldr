use super::*;
use serde::Serialize;
use std::sync::Mutex;

const SPACE_RESERVE: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskBudget {
    pub path: String,
    pub available_bytes: u64,
    pub required_bytes: u64,
    pub reserved_bytes: u64,
    pub sufficient: bool,
}

#[derive(Default)]
struct TransferState {
    limit: usize,
    active: usize,
    controls: HashMap<String, Arc<Control>>,
    budgets: HashMap<String, u64>,
}

struct Control {
    cancellation: CancellationToken,
    action: Mutex<Option<&'static str>>,
}

#[derive(Clone)]
pub struct TransferCoordinator {
    inner: Arc<Mutex<TransferState>>,
}

impl Default for TransferCoordinator {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TransferState {
                limit: 4,
                ..Default::default()
            })),
        }
    }
}

struct TransferGuard {
    coordinator: TransferCoordinator,
    work_id: String,
    active: bool,
    budgets: Vec<(String, u64)>,
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        let mut state = self.coordinator.inner.lock().expect("transfer coordinator");
        state.controls.remove(&self.work_id);
        if self.active {
            state.active -= 1;
        }
        for (volume, bytes) in &self.budgets {
            if let Some(value) = state.budgets.get_mut(volume) {
                *value = value.saturating_sub(*bytes);
            }
        }
    }
}

impl TransferCoordinator {
    pub fn set_limit(&self, limit: usize) -> Result<()> {
        if !(1..=4).contains(&limit) {
            return Err(LibraryError::InvalidDownloadConcurrency);
        }
        self.inner.lock().expect("transfer coordinator").limit = limit;
        Ok(())
    }

    pub fn has_control(&self, work_id: &str) -> bool {
        self.inner
            .lock()
            .expect("transfer coordinator")
            .controls
            .contains_key(work_id)
    }

    fn stop(&self, work_id: &str, action: &'static str) -> bool {
        let state = self.inner.lock().expect("transfer coordinator");
        if let Some(control) = state.controls.get(work_id) {
            *control.action.lock().expect("transfer action") = Some(action);
            control.cancellation.cancel();
            true
        } else {
            false
        }
    }

    pub fn disk_budget(
        &self,
        library: &Path,
        staging: &Path,
        bytes: Option<u64>,
        unpack: bool,
    ) -> Result<Vec<DiskBudget>> {
        let estimate = bytes
            .unwrap_or(SPACE_RESERVE)
            .saturating_mul(if unpack { 3 } else { 1 });
        let volumes = volume_budgets(library, staging, estimate)?;
        let state = self.inner.lock().expect("transfer coordinator");
        Ok(volumes
            .into_iter()
            .map(|(key, path, available, required)| {
                let reserved = state.budgets.get(&key).copied().unwrap_or(0);
                DiskBudget {
                    path,
                    available_bytes: available,
                    required_bytes: required.saturating_add(SPACE_RESERVE),
                    reserved_bytes: reserved,
                    sufficient: available
                        >= required
                            .saturating_add(SPACE_RESERVE)
                            .saturating_add(reserved),
                }
            })
            .collect())
    }
}

fn existing_ancestor(path: &Path) -> std::io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(std::io::Error::other(
            "Download folders must be absolute paths",
        ));
    }
    let mut current = path;
    loop {
        match std::fs::metadata(current) {
            Ok(metadata) if metadata.is_dir() => return current.canonicalize(),
            Ok(_) => return Err(std::io::Error::other("Download folder is not a directory")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                current = current.parent().ok_or(error)?
            }
            Err(error) => return Err(error),
        }
    }
}

fn volume_key(path: &Path) -> std::io::Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(std::fs::metadata(path)?.dev().to_string())
    }
    #[cfg(not(unix))]
    {
        // Canonical Windows paths have a drive/UNC prefix identifying the volume.
        Ok(path
            .components()
            .next()
            .ok_or_else(|| std::io::Error::other("Missing volume"))?
            .as_os_str()
            .to_string_lossy()
            .to_ascii_lowercase())
    }
}

fn volume_budgets(
    library: &Path,
    staging: &Path,
    bytes: u64,
) -> Result<Vec<(String, String, u64, u64)>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for path in [staging, library] {
        let ancestor = existing_ancestor(path)?;
        let key = volume_key(&ancestor)?;
        if seen.insert(key.clone()) {
            result.push((
                key,
                path.to_string_lossy().into_owned(),
                fs2::available_space(&ancestor)?,
                bytes,
            ));
        }
    }
    Ok(result)
}

impl Library {
    pub fn transfers(&self) -> &TransferCoordinator {
        &self.transfers
    }

    pub async fn stop_queued_work(&self, work_id: &str, cancel: bool) -> Result<()> {
        let entry = self
            .storage
            .download_queue()
            .await?
            .into_iter()
            .find(|e| e.work_id == work_id)
            .ok_or_else(|| LibraryError::SyncSource("Queue entry not found".into()))?;
        if !matches!(
            entry.status.as_str(),
            "queued" | "running" | "pausing" | "cancelling" | "paused"
        ) {
            return Ok(());
        }
        let action = if cancel { "cancelled" } else { "paused" };
        let draining = self.transfers.has_control(work_id);
        let status = if draining {
            if cancel {
                "cancelling"
            } else {
                "pausing"
            }
        } else {
            action
        };
        self.storage
            .request_queue_stop(work_id, &entry.batch_id, status)
            .await?;
        if draining && !self.transfers.stop(work_id, action) {
            self.storage
                .finish_queue_stop(work_id, &entry.batch_id, action)
                .await?;
        }
        Ok(())
    }

    pub async fn download_work_with_source<S>(
        &self,
        request: WorkDownloadRequest<'_>,
        source: &S,
    ) -> Result<WorkDownloadReport>
    where
        S: WorkDownloadSource + Sync,
    {
        if let Some(batch) = request.queue_batch {
            let entry = self.storage.queued_download(request.work_id).await?;
            if !entry.is_some_and(|e| e.batch_id == batch && e.status == "queued") {
                return Err(LibraryError::QueueStopped);
            }
        }
        let control = Arc::new(Control {
            cancellation: request
                .cancellation_token
                .map(CancellationToken::child_token)
                .unwrap_or_default(),
            action: Mutex::new(None),
        });
        let mut guard = {
            let mut state = self.transfers.inner.lock().expect("transfer coordinator");
            if state.controls.contains_key(request.work_id) {
                return Err(LibraryError::QueueStopped);
            }
            state
                .controls
                .insert(request.work_id.into(), control.clone());
            TransferGuard {
                coordinator: self.transfers.clone(),
                work_id: request.work_id.into(),
                active: false,
                budgets: Vec::new(),
            }
        };
        let result = async {
            loop {
                if control.cancellation.is_cancelled() {
                    return Err(LibraryError::Cancelled);
                }
                {
                    let mut state = self.transfers.inner.lock().expect("transfer coordinator");
                    if state.active < state.limit {
                        state.active += 1;
                        guard.active = true;
                        break;
                    }
                }
                tokio::time::sleep(DOWNLOAD_CANCELLATION_POLL_INTERVAL).await;
            }
            if let Some(batch) = request.queue_batch {
                let entry = self.storage.queued_download(request.work_id).await?;
                if !entry.is_some_and(|e| e.batch_id == batch && e.status == "queued") {
                    return Err(LibraryError::QueueStopped);
                }
            }
            let detail = self.product_detail(request.work_id).await?;
            let multiplier = if request.unpack_policy == UnpackPolicy::KeepArchives { 1 } else { 3 };
            let estimate = detail.content_size_bytes.unwrap_or(SPACE_RESERVE).saturating_mul(multiplier);
            let budgets = volume_budgets(request.library_root, request.download_root, estimate)?;
            {
                let mut state = self.transfers.inner.lock().expect("transfer coordinator");
                for (key, path, available, required) in &budgets {
                    let reserved = state.budgets.get(key).copied().unwrap_or(0);
                    let needed = required.saturating_add(SPACE_RESERVE).saturating_add(reserved);
                    if *available < needed {
                        return Err(LibraryError::InsufficientSpace(format!(
                            "Insufficient space at {path}: {available} bytes available, {needed} bytes required including active reservations and reserve"
                        )));
                    }
                }
                for (key, _, _, required) in budgets {
                    let value = state.budgets.entry(key.clone()).or_default();
                    *value = value.saturating_add(required);
                    guard.budgets.push((key, required));
                }
            }
            if let Some(batch) = request.queue_batch {
                self.storage.set_queue_status(request.work_id, batch, "running", None).await?;
            }
            self.download_work_unmanaged(
                WorkDownloadRequest { cancellation_token: Some(&control.cancellation), ..request },
                source,
            ).await
        }.await;
        let action = *control.action.lock().expect("transfer action");
        if let Some(batch) = request.queue_batch {
            let status = if result.is_ok() {
                "completed"
            } else if let Some(action) = action {
                action
            } else if result.as_ref().is_err_and(|e| e.is_cancelled()) {
                "paused"
            } else if matches!(result, Err(LibraryError::QueueStopped)) {
                return result;
            } else {
                "failed"
            };
            let error = result.as_ref().err().map(LibraryError::support_message);
            self.storage
                .set_queue_status(request.work_id, batch, status, error.as_deref())
                .await?;
        }
        if action.is_some() && result.is_err() {
            Err(LibraryError::QueueStopped)
        } else {
            result
        }
    }
}

use super::downloads::start_work_download_inner;
use crate::{dto::*, errors::*, state::*, validation::*};
use dldr_storage::{AppSettingsPatch, UiLocale};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetupFolders {
    library_root: String,
    download_root: String,
}

#[tauri::command]
pub(crate) fn get_setup_folders(app: AppHandle) -> Result<SetupFolders, String> {
    Ok(setup_folders(
        &app.path().download_dir().map_err(command_error)?,
    ))
}

fn setup_folders(downloads: &Path) -> SetupFolders {
    let root = downloads.join("dldr");
    SetupFolders {
        download_root: root.join(".staging").to_string_lossy().into_owned(),
        library_root: root.to_string_lossy().into_owned(),
    }
}

// Probe the directories before committing settings so first-run configuration
// cannot succeed with a file, a relative path, or an unwritable destination.
fn prepare_setup_folders(library: &Path, staging: &Path) -> Result<(), String> {
    use std::io::Write;
    if !library.is_absolute() || !staging.is_absolute() {
        return Err("Choose an absolute folder path.".into());
    }
    std::fs::create_dir_all(library).map_err(command_error)?;
    std::fs::create_dir_all(staging).map_err(command_error)?;
    let library = library.canonicalize().map_err(command_error)?;
    let staging = staging.canonicalize().map_err(command_error)?;
    if library.starts_with(&staging) {
        return Err("The library must be separate from the staging folder.".into());
    }
    for root in [library, staging] {
        let probe = root.join(format!(".dldr-write-check-{}", uuid::Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
            .map_err(command_error)?;
        let result = file.write_all(b"DLDR").and_then(|_| file.sync_all());
        drop(file);
        let cleanup = std::fs::remove_file(&probe);
        result.map_err(command_error)?;
        cleanup.map_err(command_error)?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn configure_download_folders(
    state: State<'_, AppState>,
    library_root: String,
    download_root: Option<String>,
    ui_locale: UiLocale,
) -> Result<AppSettingsDto, String> {
    let _admission = state.mutation_lock.write().await;
    require_idle(&state)?;
    let library = PathBuf::from(library_root.trim());
    let staging = download_root
        .filter(|s| !s.trim().is_empty())
        .map(|s| PathBuf::from(s.trim()))
        .unwrap_or_else(|| library.join(".staging"));
    let (check_library, check_staging) = (library.clone(), staging.clone());
    tauri::async_runtime::spawn_blocking(move || {
        prepare_setup_folders(&check_library, &check_staging)
    })
    .await
    .map_err(command_error)??;
    state
        .storage
        .save_app_settings_patch(&AppSettingsPatch {
            title_language_follows_ui: None,
            library_root: Some(library.to_string_lossy().into_owned()),
            download_root: Some(staging.to_string_lossy().into_owned()),
            ui_locale: Some(ui_locale),
            language_priority: None,
            auto_extract: None,
        })
        .await
        .map(AppSettingsDto::from)
        .map_err(command_error)
}

pub(crate) fn queue_entry(
    work_id: &str,
    batch: &str,
    account_id: Option<String>,
    library: &Path,
    staging: &Path,
    policy: dldr_download::UnpackPolicy,
    replace: bool,
) -> dldr_storage::DownloadQueueEntry {
    dldr_storage::DownloadQueueEntry {
        title: work_id.into(),
        thumbnail_url: None,
        work_id: work_id.into(),
        batch_id: batch.into(),
        position: 0,
        account_id,
        library_root: library.to_string_lossy().into_owned(),
        download_root: staging.to_string_lossy().into_owned(),
        // Queue rows use the database representation, not camelCase IPC/audit labels.
        unpack_policy: match policy {
            dldr_download::UnpackPolicy::KeepArchives => "keep_archives",
            dldr_download::UnpackPolicy::UnpackWhenRecognized => "unpack_when_recognized",
        }
        .into(),
        replace_existing: replace,
        status: "queued".into(),
        error_message: None,
    }
}

fn require_idle(state: &AppState) -> Result<(), String> {
    if state.jobs.list_jobs().iter().any(|j| j.status.is_active()) {
        return Err("Wait for active jobs to finish before maintaining the library".into());
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_transfer_settings(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .storage
        .download_concurrency()
        .await
        .map_err(command_error)
}

#[tauri::command]
pub(crate) async fn set_transfer_concurrency(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<(), String> {
    let _admission = state.mutation_lock.read().await;
    let _queue = state.queue_lock.lock().await;
    if !(1..=4).contains(&limit) {
        return Err("Global simultaneous works must be between 1 and 4".into());
    }
    state
        .storage
        .save_download_concurrency(limit)
        .await
        .map_err(command_error)?;
    state
        .library
        .transfers()
        .set_limit(limit)
        .map_err(command_error)
}

#[tauri::command]
pub(crate) async fn list_download_queue(
    state: State<'_, AppState>,
) -> Result<Vec<dldr_storage::DownloadQueueEntry>, String> {
    state.storage.download_queue().await.map_err(command_error)
}

pub(crate) async fn resume_entry(
    app: AppHandle,
    state: &AppState,
    entry: dldr_storage::DownloadQueueEntry,
) -> Result<StartJobResponse, String> {
    if state.library.transfers().has_control(&entry.work_id) {
        return Err("Work is still stopping; wait for paused/cancelled status".into());
    }
    if !matches!(entry.status.as_str(), "paused" | "failed" | "cancelled") {
        return Err("Only paused, failed or cancelled works can resume".into());
    }
    // A stopped work may still belong to a live parent batch. New generation
    // prevents that parent's unstarted future from executing the resumed request.
    state
        .download_reservations
        .inner
        .lock()
        .expect("download reservations lock")
        .by_work_id
        .remove(&entry.work_id);
    let request = StartWorkDownloadRequest {
        work_id: entry.work_id.clone(),
        account_id: entry.account_id.clone(),
        password: None,
        unpack_policy: Some(if entry.unpack_policy == "keep_archives" {
            UnpackPolicyDto::KeepArchives
        } else {
            UnpackPolicyDto::UnpackWhenRecognized
        }),
        replace_existing: Some(entry.replace_existing), // Keep replacement intent but retain the partial prefix.
    };
    start_work_download_inner(app, state, request, Some(entry)).await
}

#[tauri::command]
pub(crate) async fn control_download_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    work_id: String,
    action: String,
) -> Result<(), String> {
    let _admission = state.mutation_lock.read().await;
    let _queue = state.queue_lock.lock().await;
    match action.as_str() {
        "pause" | "cancel" => state
            .library
            .stop_queued_work(&work_id, action == "cancel")
            .await
            .map_err(command_error),
        "resume" => {
            let entry = state
                .storage
                .download_queue()
                .await
                .map_err(command_error)?
                .into_iter()
                .find(|e| e.work_id == work_id)
                .ok_or("Queue entry not found")?;
            resume_entry(app, &state, entry).await?;
            Ok(())
        }
        "resumeAll" => {
            for entry in state
                .storage
                .download_queue()
                .await
                .map_err(command_error)?
                .into_iter()
                .filter(|e| e.status == "paused")
            {
                resume_entry(app.clone(), &state, entry).await?;
            }
            Ok(())
        }
        _ => Err("Unknown queue action".into()),
    }
}

#[tauri::command]
pub(crate) async fn retry_failed_downloads(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let _admission = state.mutation_lock.read().await;
    let _queue = state.queue_lock.lock().await;
    let mut count = 0;
    for entry in state
        .storage
        .download_queue()
        .await
        .map_err(command_error)?
        .into_iter()
        .filter(|e| e.status == "failed")
    {
        resume_entry(app.clone(), &state, entry).await?;
        count += 1;
    }
    Ok(count)
}

#[tauri::command]
pub(crate) async fn clear_download_queue(state: State<'_, AppState>) -> Result<(), String> {
    let _admission = state.mutation_lock.read().await;
    let _queue = state.queue_lock.lock().await;
    state
        .storage
        .clear_finished_queue()
        .await
        .map_err(command_error)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SpaceReport {
    budgets: Vec<dldr_library::DiskBudget>,
    unknown_count: usize,
}

#[tauri::command]
pub(crate) async fn check_download_space(
    app: AppHandle,
    state: State<'_, AppState>,
    request: BulkWorkDownloadCommandRequest,
) -> Result<SpaceReport, String> {
    let settings = state.storage.app_settings().await.map_err(command_error)?;
    let library = required_library_root(&settings)?;
    let staging = effective_download_root(&app, &settings)?;
    let mut bytes = 0u64;
    let mut unknown_count = 0;
    let query = request.to_query()?;
    let ids = request.selected_work_ids()?;
    let candidates = state
        .library
        .select_bulk_candidates(
            &query,
            request.skip_downloaded.unwrap_or(true),
            ids.as_deref(),
        )
        .await
        .map_err(command_error)?;
    for id in candidates.work_ids {
        let detail = state
            .library
            .product_detail(&id)
            .await
            .map_err(command_error)?;
        match detail.content_size_bytes {
            Some(size) => bytes = bytes.saturating_add(size),
            None => {
                unknown_count += 1;
                bytes = bytes.saturating_add(256 * 1024 * 1024);
            }
        }
    }
    let budgets = state
        .library
        .transfers()
        .disk_budget(
            &library,
            &staging,
            Some(bytes),
            !matches!(request.unpack_policy, Some(UnpackPolicyDto::KeepArchives)),
        )
        .map_err(command_error)?;
    Ok(SpaceReport {
        budgets,
        unknown_count,
    })
}

#[tauri::command]
pub(crate) async fn verify_library(
    state: State<'_, AppState>,
) -> Result<dldr_library::VerificationReport, String> {
    let _admission = state.mutation_lock.try_write().map_err(|_| {
        "Another library update is in progress; retry maintenance when it finishes".to_owned()
    })?;
    require_idle(&state)?;
    let settings = state.storage.app_settings().await.map_err(command_error)?;
    let root = required_library_root(&settings)?;
    state
        .library
        .verify_downloaded_library(&root)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub(crate) async fn export_library_backup(
    state: State<'_, AppState>,
    path: String,
) -> Result<usize, String> {
    let _admission = state.mutation_lock.try_write().map_err(|_| {
        "Another library update is in progress; retry maintenance when it finishes".to_owned()
    })?;
    require_idle(&state)?;
    let backup = state
        .storage
        .export_library_backup()
        .await
        .map_err(command_error)?;
    let count = backup.tables.values().map(Vec::len).sum();
    let bytes = serde_json::to_vec_pretty(&backup).map_err(command_error)?;
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err("Choose an absolute backup path".into());
    }
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(command_error)?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(command_error(error));
    }
    Ok(count)
}

#[tauri::command]
pub(crate) async fn restore_library_backup(
    state: State<'_, AppState>,
    path: String,
) -> Result<usize, String> {
    let _admission = state.mutation_lock.try_write().map_err(|_| {
        "Another library update is in progress; retry maintenance when it finishes".to_owned()
    })?;
    require_idle(&state)?;
    let file = std::fs::File::open(path).map_err(command_error)?;
    if file.metadata().map_err(command_error)?.len() > 128 * 1024 * 1024 {
        return Err("Backup exceeds 128 MiB".into());
    }
    let backup: dldr_storage::LibraryBackup =
        serde_json::from_reader(file).map_err(command_error)?;
    let count = state
        .storage
        .restore_library_backup(&backup)
        .await
        .map_err(command_error)?;
    state
        .download_rate_limiter
        .set_limit(
            state
                .storage
                .download_rate_limit()
                .await
                .map_err(command_error)?,
        )
        .map_err(str::to_owned)?;
    state
        .library
        .transfers()
        .set_limit(
            state
                .storage
                .download_concurrency()
                .await
                .map_err(command_error)?,
        )
        .map_err(command_error)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use dldr_storage::Storage;
    #[test]
    fn first_run_defaults_stay_inside_downloads_without_creating_them() {
        let temp = tempfile::tempdir().unwrap();
        let downloads = temp.path().join("Downloads");
        let defaults = super::setup_folders(&downloads);
        assert_eq!(PathBuf::from(defaults.library_root), downloads.join("dldr"));
        assert_eq!(
            PathBuf::from(defaults.download_root),
            downloads.join("dldr/.staging")
        );
        assert!(!downloads.exists());
    }

    #[test]
    fn first_run_creates_writable_unicode_folders_and_preserves_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("中文 日本語/dldr");
        let staging = library.join(".staging");
        super::prepare_setup_folders(&library, &staging).unwrap();
        std::fs::write(library.join("keep.txt"), "keep").unwrap();
        super::prepare_setup_folders(&library, &staging).unwrap();
        assert_eq!(std::fs::read(library.join("keep.txt")).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&library).unwrap().count(), 2);
    }

    #[test]
    fn first_run_rejects_relative_file_and_overlapping_destinations() {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("library");
        assert!(super::prepare_setup_folders(Path::new("relative"), &library).is_err());
        assert!(super::prepare_setup_folders(&library, &library).is_err());
        assert!(super::prepare_setup_folders(&library.join("inside"), &library).is_err());
        let file = temp.path().join("file");
        std::fs::write(&file, "keep").unwrap();
        assert!(super::prepare_setup_folders(&file, &library).is_err());
        assert!(super::prepare_setup_folders(&library, &file).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
    }
    use super::*;

    #[tokio::test]
    async fn gui_queue_entries_persist_both_policies_for_single_bulk_and_requeue() {
        use dldr_download::UnpackPolicy::{KeepArchives, UnpackWhenRecognized};
        let storage = Storage::open_in_memory().await.unwrap();
        storage.run_migrations().await.unwrap();
        let mut transaction = storage.begin_write().await.unwrap();
        transaction
            .execute(
                "INSERT INTO works(work_id,title_json,raw_json,last_detail_sync_at) VALUES
            ('RJ000001','{}','{}','2026-11-16'),('RJ000002','{}','{}','2026-11-16'),
            ('RJ000003','{}','{}','2026-11-16'),('RJ000004','{}','{}','2026-11-16')",
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        for (id, policy) in [
            ("RJ000001", KeepArchives),
            ("RJ000002", UnpackWhenRecognized),
        ] {
            let entry = queue_entry(
                id,
                "single",
                None,
                Path::new("/library"),
                Path::new("/staging"),
                policy,
                false,
            );
            storage.enqueue_downloads(&[entry]).await.unwrap();
        }
        let entries = [
            queue_entry(
                "RJ000003",
                "bulk",
                None,
                Path::new("/library"),
                Path::new("/staging"),
                KeepArchives,
                true,
            ),
            queue_entry(
                "RJ000004",
                "bulk",
                None,
                Path::new("/library"),
                Path::new("/staging"),
                UnpackWhenRecognized,
                false,
            ),
        ];
        storage.enqueue_downloads(&entries).await.unwrap();
        let queue = storage.download_queue().await.unwrap();
        assert_eq!(queue.len(), 4);
        for (id, policy) in [
            ("RJ000001", "keep_archives"),
            ("RJ000002", "unpack_when_recognized"),
            ("RJ000003", "keep_archives"),
            ("RJ000004", "unpack_when_recognized"),
        ] {
            let entry = queue.iter().find(|entry| entry.work_id == id).unwrap();
            assert_eq!(entry.unpack_policy, policy);
            assert_eq!(entry.status, "queued");
        }
        storage
            .set_queue_status("RJ000001", "single", "failed", Some("Synthetic failure"))
            .await
            .unwrap();
        let retry = queue_entry(
            "RJ000001",
            "retry",
            None,
            Path::new("/library"),
            Path::new("/staging"),
            UnpackWhenRecognized,
            true,
        );
        storage.enqueue_downloads(&[retry]).await.unwrap();
        let retry = storage.queued_download("RJ000001").await.unwrap().unwrap();
        assert_eq!(retry.status, "queued");
        assert_eq!(retry.unpack_policy, "unpack_when_recognized");
        assert!(retry.replace_existing);
        assert!(retry.error_message.is_none());
    }
}

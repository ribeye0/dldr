use super::*;
use dldr_api::{
    Credentials, DownloadFile, DownloadFileKind, DownloadPlan, DownloadStreamRequest, LoginOutcome,
    SessionSnapshot, TwoFactorChallenge, WorkId,
};
use dldr_credentials::{CredentialRef, CredentialStore, InMemoryCredentialStore};
use dldr_download::{
    DownloadFileMetadata, DownloadJobRequest, DownloadPhase, DownloadProgress, DownloadedFile,
    DownloadedWork,
};
use dldr_library::{DlsiteAuthSource, WorkDownloadSource};
use dldr_storage::{AccountSyncCommit, AccountUpsert, AccountWork, CachedWork, Storage};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Default)]
struct State {
    active: AtomicUsize,
    peak: AtomicUsize,
    started: AtomicUsize,
    resumed: AtomicBool,
    wait_for_cancel: bool,
    fail_first: bool,
}
struct FakeSource(Arc<State>);

#[async_trait::async_trait]
impl DlsiteAuthSource for FakeSource {
    async fn restore_session(&self, _: &SessionSnapshot) -> dldr_library::Result<()> {
        Ok(())
    }
    async fn validate_session(&self) -> dldr_library::Result<bool> {
        Ok(true)
    }
    async fn begin_login(&self, _: &Credentials) -> dldr_library::Result<LoginOutcome> {
        Ok(LoginOutcome::Authorized(SessionSnapshot {
            cookies_json: "fixture-session".into(),
        }))
    }
    async fn complete_two_factor(
        &self,
        _: &TwoFactorChallenge,
        _: &str,
    ) -> dldr_library::Result<SessionSnapshot> {
        unreachable!()
    }
}

#[async_trait::async_trait]
impl WorkDownloadSource for FakeSource {
    async fn download_plan(&self, id: &WorkId) -> dldr_library::Result<DownloadPlan> {
        Ok(DownloadPlan {
            work_id: id.clone(),
            files: vec![DownloadFile {
                kind: DownloadFileKind::Direct,
                stream_request: DownloadStreamRequest {
                    url: "https://fixture.invalid/archive".parse().unwrap(),
                },
            }],
            serial_numbers: vec![],
        })
    }
    async fn download_file_metadata(
        &self,
        index: usize,
        file: &DownloadFile,
    ) -> dldr_library::Result<DownloadFileMetadata> {
        Ok(DownloadFileMetadata {
            file_index: index,
            file_kind: file.kind.clone(),
            file_name: "payload.bin".into(),
            expected_size: Some(6),
            final_url: file.stream_request.url.clone(),
        })
    }
    async fn download_files(
        &self,
        job: &DownloadJobRequest,
        _: &DownloadPlan,
        cancellation: &dldr_download::CancellationToken,
        progress: &mut (dyn FnMut(DownloadProgress) + Send),
    ) -> dldr_library::Result<DownloadedWork> {
        self.0.started.fetch_add(1, Ordering::SeqCst);
        let active = self.0.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.0.peak.fetch_max(active, Ordering::SeqCst);
        let result = async {
            let target = job.target_root.join(job.work_id.as_ref());
            tokio::fs::create_dir_all(&target).await?;
            let path = target.join("payload.bin");
            let resumed = path.exists();
            if resumed {
                assert_eq!(tokio::fs::read(&path).await?, b"pre");
                self.0.resumed.store(true, Ordering::SeqCst);
            } else {
                tokio::fs::write(&path, b"pre").await?;
            }
            progress(DownloadProgress {
                phase: DownloadPhase::Downloading,
                file_index: Some(0),
                file_kind: Some(DownloadFileKind::Direct),
                bytes_received: 3,
                bytes_total: Some(6),
            });
            if self.0.wait_for_cancel {
                while !cancellation.is_cancelled() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                // Simulate a transfer needing time to close files after cancellation.
                tokio::time::sleep(Duration::from_millis(30)).await;
                return Err(dldr_library::LibraryError::Cancelled);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
            if self.0.fail_first && job.work_id.as_ref() == "RJ000001" {
                return Err(dldr_library::LibraryError::SyncSource(
                    "fixture transfer failure".into(),
                ));
            }
            tokio::fs::write(&path, b"preend").await?;
            progress(DownloadProgress {
                phase: DownloadPhase::Downloading,
                file_index: Some(0),
                file_kind: Some(DownloadFileKind::Direct),
                bytes_received: 6,
                bytes_total: Some(6),
            });
            Ok(DownloadedWork {
                work_id: job.work_id.clone(),
                target_dir: target,
                files: vec![DownloadedFile {
                    file_name: "payload.bin".into(),
                    path,
                    bytes_written: 6,
                    resumed_from: if resumed { 3 } else { 0 },
                }],
                archive_extraction: None,
            })
        }
        .await;
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

async fn fixture(root: &std::path::Path) -> (Context, Vec<DownloadQueueEntry>) {
    let storage = Storage::open_in_memory().await.unwrap();
    storage.run_migrations().await.unwrap();
    storage
        .save_account(&AccountUpsert {
            id: "test".into(),
            label: "Test".into(),
            login_name: Some("fixture@example.invalid".into()),
            credential_ref: Some("fixture-password".into()),
            enabled: true,
        })
        .await
        .unwrap();
    let works = (1..=4)
        .map(|i| CachedWork {
            work_id: format!("RJ{i:06}"),
            title_json: json!({"ja_JP":format!("Fixture {i}")}).to_string(),
            maker_id: None,
            maker_json: None,
            work_type: Some("SOU".into()),
            age_category: Some("r18".into()),
            thumbnail_url: None,
            registered_at: None,
            published_at: None,
            updated_at: None,
            raw_json: json!({"content_size":6}).to_string(),
            last_detail_sync_at: "2026-11-16T00:00:00Z".into(),
        })
        .collect();
    storage
        .commit_account_sync(&AccountSyncCommit {
            sync_run_id: "test-sync".into(),
            account_id: "test".into(),
            started_at: "2026-11-16T00:00:00Z".into(),
            completed_at: "2026-11-16T00:00:01Z".into(),
            works,
            account_works: (1..=4)
                .map(|i| AccountWork {
                    work_id: format!("RJ{i:06}"),
                    purchased_at: None,
                })
                .collect(),
        })
        .await
        .unwrap();
    let credentials = Arc::new(InMemoryCredentialStore::new());
    credentials
        .save_password(&CredentialRef::new("fixture-password").unwrap(), "fixture")
        .unwrap();
    let context = Context {
        library: dldr_library::Library::new(storage, credentials),
        data_dir: root.into(),
        _instance: None,
    };
    let library = root.join("library");
    let temp = root.join("temp");
    std::fs::create_dir_all(&library).unwrap();
    std::fs::create_dir_all(&temp).unwrap();
    let entries = (1..=4)
        .map(|i| DownloadQueueEntry {
            work_id: format!("RJ{i:06}"),
            title: format!("Fixture {i}"),
            thumbnail_url: None,
            batch_id: "old-batch".into(),
            position: i,
            account_id: Some("test".into()),
            library_root: library.to_str().unwrap().into(),
            download_root: temp.to_str().unwrap().into(),
            unpack_policy: "keep_archives".into(),
            replace_existing: true,
            status: "paused".into(),
            error_message: None,
        })
        .collect();
    (context, entries)
}
fn cli() -> Cli {
    Cli {
        data_dir: None,
        json: true,
        quiet: true,
        command: None,
        download: Default::default(),
    }
}
fn options() -> TransferOptions {
    TransferOptions {
        jobs: Some(2),
        limit_rate: Some(0),
    }
}

#[tokio::test]
async fn parallel_failures_do_not_abort_other_works_or_exceed_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let (context, entries) = fixture(dir.path()).await;
    let state = Arc::new(State {
        fail_first: true,
        ..Default::default()
    });
    let code = execute_with_factory(
        &context,
        &cli(),
        &options(),
        entries,
        0,
        CancellationToken::new(),
        |_| Ok(FakeSource(state.clone())),
    )
    .await
    .unwrap();
    assert_eq!(code, 1);
    assert_eq!(state.peak.load(Ordering::SeqCst), 2);
    assert_eq!(state.active.load(Ordering::SeqCst), 0);
    let queue = context.storage().download_queue().await.unwrap();
    assert_eq!(queue.iter().filter(|e| e.status == "completed").count(), 3);
    assert_eq!(queue.iter().filter(|e| e.status == "failed").count(), 1);
    for entry in queue.iter().filter(|e| e.status == "completed") {
        let path = context
            .storage()
            .work_download_state(&entry.work_id)
            .await
            .unwrap()
            .local_path
            .unwrap();
        assert_eq!(
            std::fs::read(PathBuf::from(path).join("payload.bin")).unwrap(),
            b"preend"
        );
    }
}

#[tokio::test]
async fn interruption_drains_active_transfers_then_resumes_the_existing_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let (context, entries) = fixture(dir.path()).await;
    let cancellation = CancellationToken::new();
    let state = Arc::new(State {
        wait_for_cancel: true,
        ..Default::default()
    });
    let observer = state.clone();
    let stop = cancellation.clone();
    let cancel = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(3), async {
            while observer.active.load(Ordering::SeqCst) < 2 {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        stop.cancel();
    });
    let code = execute_with_factory(
        &context,
        &cli(),
        &options(),
        entries,
        0,
        cancellation,
        |_| Ok(FakeSource(state.clone())),
    )
    .await
    .unwrap();
    cancel.await.unwrap();
    assert_eq!(code, 130);
    assert_eq!(state.active.load(Ordering::SeqCst), 0);
    assert_eq!(state.started.load(Ordering::SeqCst), 2);
    let queue = context.storage().download_queue().await.unwrap();
    assert!(queue.iter().all(|e| e.status == "paused"));
    let next = Arc::new(State::default());
    assert_eq!(
        execute_with_factory(
            &context,
            &cli(),
            &options(),
            queue,
            0,
            CancellationToken::new(),
            |_| Ok(FakeSource(next.clone()))
        )
        .await
        .unwrap(),
        0
    );
    assert!(next.resumed.load(Ordering::SeqCst));
    assert!(context
        .storage()
        .download_queue()
        .await
        .unwrap()
        .iter()
        .all(|e| e.status == "completed"));
}

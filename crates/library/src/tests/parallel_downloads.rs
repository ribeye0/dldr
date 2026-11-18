use super::*;
use std::sync::atomic::AtomicUsize;
use tokio::sync::{mpsc, Semaphore};

struct Transfers {
    started: mpsc::UnboundedSender<String>,
    gates: BTreeMap<String, Semaphore>,
    active: AtomicUsize,
    peak: AtomicUsize,
    created: AtomicUsize,
    fail: Option<String>,
    cancel: bool,
    wait_plan: bool,
}

struct ControlledSource {
    transfers: Arc<Transfers>,
    // Deliberately not shared between source instances, like a real cookie jar.
    login: Mutex<Option<String>>,
}

#[async_trait]
impl DlsiteAuthSource for ControlledSource {
    async fn restore_session(&self, _: &SessionSnapshot) -> Result<()> {
        Ok(())
    }
    async fn validate_session(&self) -> Result<bool> {
        Ok(false)
    }
    async fn begin_login(&self, credentials: &Credentials) -> Result<LoginOutcome> {
        *self.login.lock().unwrap() = Some(credentials.username.clone());
        Ok(LoginOutcome::Authorized(fake_session()))
    }
    async fn complete_two_factor(
        &self,
        _: &TwoFactorChallenge,
        _: &str,
    ) -> Result<SessionSnapshot> {
        unreachable!()
    }
}

struct ActiveTransfer<'a>(&'a AtomicUsize);
impl Drop for ActiveTransfer<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[async_trait]
impl WorkDownloadSource for ControlledSource {
    async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan> {
        if self.transfers.wait_plan {
            self.transfers.started.send(work_id.to_string()).unwrap();
            std::future::pending::<()>().await;
        }
        FakeDownloadSource.download_plan(work_id).await
    }
    async fn download_file_metadata(
        &self,
        index: usize,
        file: &DownloadFile,
    ) -> Result<DownloadFileMetadata> {
        FakeDownloadSource.download_file_metadata(index, file).await
    }
    async fn download_files(
        &self,
        job: &DownloadJobRequest,
        plan: &DownloadPlan,
        cancellation: &dldr_download::CancellationToken,
        progress: &mut (dyn FnMut(DownloadProgress) + Send),
    ) -> Result<DownloadedWork> {
        let id = job.work_id.as_ref();
        let expected = if id.ends_with(['2', '4', '6']) {
            "second@example.test"
        } else {
            "user@example.test"
        };
        assert_eq!(self.login.lock().unwrap().as_deref(), Some(expected));
        let active = self.transfers.active.fetch_add(1, Ordering::SeqCst) + 1;
        let _guard = ActiveTransfer(&self.transfers.active);
        self.transfers.peak.fetch_max(active, Ordering::SeqCst);
        progress(DownloadProgress {
            phase: DownloadPhase::Downloading,
            file_index: Some(0),
            file_kind: None,
            bytes_received: 1,
            bytes_total: Some(10),
        });
        self.transfers.started.send(id.into()).unwrap();
        loop {
            if cancellation.is_cancelled() {
                return Err(LibraryError::Cancelled);
            }
            tokio::select! {
                permit = self.transfers.gates[id].acquire() => { permit.unwrap().forget(); break; },
                _ = tokio::time::sleep(Duration::from_millis(5)) => {},
            }
        }
        assert_eq!(
            self.login.lock().unwrap().as_deref(),
            Some(expected),
            "another work changed this source's login"
        );
        if self.transfers.fail.as_deref() == Some(id) {
            return Err(if self.transfers.cancel {
                LibraryError::TwoFactorCancelled("account-b".into())
            } else {
                LibraryError::SyncSource("synthetic download failure".into())
            });
        }
        FakeDownloadSource
            .download_files(job, plan, cancellation, progress)
            .await
    }
}

fn transfers(
    count: usize,
    fail: Option<&str>,
    cancel: bool,
) -> (Arc<Transfers>, mpsc::UnboundedReceiver<String>) {
    let (started, receiver) = mpsc::unbounded_channel();
    (
        Arc::new(Transfers {
            started,
            gates: (1..=count)
                .map(|i| (format!("RJ{i:06}"), Semaphore::new(0)))
                .collect(),
            active: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            created: AtomicUsize::new(0),
            fail: fail.map(str::to_owned),
            cancel,
            wait_plan: false,
        }),
        receiver,
    )
}

fn source(state: &Arc<Transfers>) -> ControlledSource {
    state.created.fetch_add(1, Ordering::SeqCst);
    ControlledSource {
        transfers: state.clone(),
        login: Mutex::new(None),
    }
}

async fn library_with_works(count: usize) -> Result<Library> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .save_account(SaveAccountRequest {
            id: Some("account-b".into()),
            login_name: Some("second@example.test".into()),
            ..save_account_request(true)
        })
        .await?;
    for (account, parity) in [("account-a", 1), ("account-b", 0)] {
        let works = (1..=count)
            .filter(|i| i % 2 == parity)
            .map(|i| {
                cached_work_from_api(
                    work(
                        &format!("RJ{i:06}"),
                        &format!("Work {i:02}"),
                        "Maker",
                        "2026-01-01T00:00:00Z",
                    ),
                    "2026-11-15",
                )
            })
            .collect::<Result<Vec<_>>>()?;
        library
            .storage
            .commit_account_sync(&AccountSyncCommit {
                sync_run_id: format!("sync-{account}"),
                account_id: account.into(),
                started_at: "2026-11-15".into(),
                completed_at: "2026-11-15".into(),
                account_works: works
                    .iter()
                    .map(|w| AccountWork {
                        work_id: w.work_id.clone(),
                        purchased_at: None,
                    })
                    .collect(),
                works,
            })
            .await?;
    }
    Ok(library)
}

#[tokio::test]
async fn bounded_parallelism_refills_slots_preserves_scope_and_counts_out_of_order_completion(
) -> Result<()> {
    let library = library_with_works(6).await?;
    let root = tempfile::tempdir()?;
    let staging = root.path().join("staging");
    let final_root = root.path().join("library");
    let (state, mut started) = transfers(6, None, false);
    let sink = RecordingBulkDownloadProgressSink::default();
    let mut request = BulkWorkDownloadRequest::new(
        ProductListQuery {
            sort: ProductSort::TitleAsc,
            ..Default::default()
        },
        &final_root,
        &staging,
    );
    request.work_ids = Some((1..=5).map(|i| format!("RJ{i:06}")).collect());
    request.progress_sink = Some(&sink);
    let download = library.download_products_with_source_factory(request, 2, || Ok(source(&state)));
    let control = async {
        let mut first = vec![started.recv().await.unwrap(), started.recv().await.unwrap()];
        first.sort();
        assert_eq!(first, ["RJ000001", "RJ000002"]);
        state.gates["RJ000002"].add_permits(1);
        assert_eq!(started.recv().await.unwrap(), "RJ000003");
        // Work 1 is still waiting. Slot 2 is immediately reused by work 3.
        assert_eq!(state.peak.load(Ordering::SeqCst), 2);
        for id in ["RJ000001", "RJ000003", "RJ000004", "RJ000005"] {
            state.gates[id].add_permits(1);
        }
    };
    let (report, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(download, control)
    })
    .await
    .unwrap();
    let report = report?;
    assert_eq!(report.succeeded_count, 5);
    assert_eq!(report.failed_count, 0);
    assert_eq!(report.succeeded_works[0].work_id, "RJ000002");
    assert_eq!(state.created.load(Ordering::SeqCst), 5);
    assert_eq!(state.active.load(Ordering::SeqCst), 0);
    assert_eq!(state.peak.load(Ordering::SeqCst), 2);
    assert!(!final_root.join("RJ000006").exists());
    let completed: Vec<_> = sink
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|p| match p {
            BulkWorkDownloadProgress::WorkCompleted { current, .. } => Some(*current),
            _ => None,
        })
        .collect();
    assert_eq!(completed, [1, 2, 3, 4, 5]);
    Ok(())
}

#[tokio::test]
async fn ordinary_failure_continues_siblings_and_pending_works() -> Result<()> {
    let library = library_with_works(4).await?;
    let root = tempfile::tempdir()?;
    let staging = root.path().join("staging");
    let final_root = root.path().join("library");
    let (state, _receiver) = transfers(4, Some("RJ000002"), false);
    for gate in state.gates.values() {
        gate.add_permits(1);
    }
    let report = library
        .download_products_with_source_factory(
            BulkWorkDownloadRequest::new(ProductListQuery::default(), &final_root, &staging),
            2,
            || Ok(source(&state)),
        )
        .await?;
    assert_eq!(report.succeeded_count, 3);
    assert_eq!(report.failed_count, 1);
    assert_eq!(report.failed_works[0].work_id, "RJ000002");
    assert_eq!(state.created.load(Ordering::SeqCst), 4);
    assert_eq!(state.active.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn external_and_internal_cancellation_drain_all_started_works_before_returning() -> Result<()>
{
    for internal in [false, true] {
        let library = library_with_works(4).await?;
        let root = tempfile::tempdir()?;
        let staging = root.path().join("staging");
        let final_root = root.path().join("library");
        let (state, mut started) = transfers(4, Some("RJ000002"), internal);
        let token = CancellationToken::new();
        let mut request = BulkWorkDownloadRequest::new(
            ProductListQuery {
                sort: ProductSort::TitleAsc,
                ..Default::default()
            },
            &final_root,
            &staging,
        );
        request.cancellation_token = Some(&token);
        let download =
            library.download_products_with_source_factory(request, 2, || Ok(source(&state)));
        let control = async {
            started.recv().await.unwrap();
            started.recv().await.unwrap();
            if internal {
                state.gates["RJ000002"].add_permits(1);
            } else {
                token.cancel();
            }
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(download, control)
        })
        .await
        .unwrap();
        let error = result.unwrap_err();
        assert!(error.is_cancelled());
        assert!(matches!(error, LibraryError::BulkInterrupted { .. }));
        assert_eq!(
            state.created.load(Ordering::SeqCst),
            2,
            "must not dispatch queued works"
        );
        assert_eq!(
            state.active.load(Ordering::SeqCst),
            0,
            "must await sibling cleanup"
        );
        for id in ["RJ000001", "RJ000002"] {
            assert_eq!(
                library.storage.work_download_state(id).await?.status,
                WorkDownloadStatus::Cancelled
            );
        }
        assert!(!final_root.join("RJ000003").exists());
    }
    Ok(())
}

#[tokio::test]
async fn cancellation_retains_completed_siblings_and_stops_queued_dispatch() -> Result<()> {
    struct CancelOnCompletion(CancellationToken);
    impl BulkWorkDownloadProgressSink for CancelOnCompletion {
        fn emit(&self, progress: BulkWorkDownloadProgress) {
            if matches!(progress, BulkWorkDownloadProgress::WorkCompleted { .. }) {
                self.0.cancel();
            }
        }
    }
    let library = library_with_works(4).await?;
    let root = tempfile::tempdir()?;
    let staging = root.path().join("staging");
    let final_root = root.path().join("library");
    let (state, mut started) = transfers(4, None, false);
    let token = CancellationToken::new();
    let sink = CancelOnCompletion(token.clone());
    let mut request = BulkWorkDownloadRequest::new(
        ProductListQuery {
            sort: ProductSort::TitleAsc,
            ..Default::default()
        },
        &final_root,
        &staging,
    );
    request.cancellation_token = Some(&token);
    request.progress_sink = Some(&sink);
    let download = library.download_products_with_source_factory(request, 2, || Ok(source(&state)));
    let control = async {
        started.recv().await.unwrap();
        started.recv().await.unwrap();
        state.gates["RJ000002"].add_permits(1);
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(download, control)
    })
    .await
    .unwrap();
    let LibraryError::BulkInterrupted { report, .. } = result.unwrap_err() else {
        panic!("partial report required")
    };
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(report.failed_count, 0);
    assert_eq!(report.succeeded_works[0].work_id, "RJ000002");
    assert!(report.succeeded_works[0].local_path.is_dir());
    assert_eq!(state.created.load(Ordering::SeqCst), 2);
    assert_eq!(state.active.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn cancellation_interrupts_stalled_plan_requests_and_drains_the_batch() -> Result<()> {
    let library = library_with_works(4).await?;
    let root = tempfile::tempdir()?;
    let staging = root.path().join("staging");
    let final_root = root.path().join("library");
    let (mut state, mut started) = transfers(4, None, false);
    Arc::get_mut(&mut state).unwrap().wait_plan = true;
    let token = CancellationToken::new();
    let mut request =
        BulkWorkDownloadRequest::new(ProductListQuery::default(), &final_root, &staging);
    request.cancellation_token = Some(&token);
    let download = library.download_products_with_source_factory(request, 2, || Ok(source(&state)));
    let control = async {
        started.recv().await.unwrap();
        started.recv().await.unwrap();
        token.cancel();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(download, control)
    })
    .await
    .unwrap();
    assert!(result.unwrap_err().is_cancelled());
    assert_eq!(state.created.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn sequential_limit_and_invalid_limits_are_enforced() -> Result<()> {
    let library = library_with_works(4).await?;
    let root = tempfile::tempdir()?;
    let staging = root.path().join("staging");
    let final_root = root.path().join("library");
    let (state, _receiver) = transfers(4, None, false);
    for gate in state.gates.values() {
        gate.add_permits(1);
    }
    for limit in [0, MAX_PARALLEL_WORK_DOWNLOADS + 1] {
        let error = library
            .download_products_with_source_factory(
                BulkWorkDownloadRequest::new(ProductListQuery::default(), &final_root, &staging),
                limit,
                || Ok(source(&state)),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, LibraryError::InvalidDownloadConcurrency));
    }
    assert_eq!(state.created.load(Ordering::SeqCst), 0);
    let report = library
        .download_products_with_source_factory(
            BulkWorkDownloadRequest::new(ProductListQuery::default(), &final_root, &staging),
            1,
            || Ok(source(&state)),
        )
        .await?;
    assert_eq!(report.succeeded_count, 4);
    assert_eq!(state.peak.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn global_limit_covers_two_batches_and_a_single_work_and_can_be_lowered_live() -> Result<()> {
    let library = library_with_works(5).await?;
    library.transfers().set_limit(2)?;
    let root = tempfile::tempdir()?;
    let staging = root.path().join("staging");
    let final_root = root.path().join("library");
    let (state, mut started) = transfers(5, None, false);
    let mut a = BulkWorkDownloadRequest::new(ProductListQuery::default(), &final_root, &staging);
    a.work_ids = Some(vec!["RJ000001".into(), "RJ000002".into()]);
    let mut b = a.clone();
    b.work_ids = Some(vec!["RJ000003".into(), "RJ000004".into()]);
    let single_source = source(&state);
    let control = async {
        let first = started.recv().await.unwrap();
        started.recv().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), started.recv())
                .await
                .is_err()
        );
        library.transfers().set_limit(1).unwrap();
        state.gates[&first].add_permits(1);
        assert!(
            tokio::time::timeout(Duration::from_millis(120), started.recv())
                .await
                .is_err()
        );
        for gate in state.gates.values() {
            gate.add_permits(1);
        }
    };
    let (a, b, s, ()) = tokio::time::timeout(Duration::from_secs(6), async {
        tokio::join!(
            library.download_products_with_source_factory(a, 2, || Ok(source(&state))),
            library.download_products_with_source_factory(b, 2, || Ok(source(&state))),
            library.download_work_with_source(
                WorkDownloadRequest::new("RJ000005", &final_root, &staging),
                &single_source
            ),
            control
        )
    })
    .await
    .unwrap();
    assert_eq!(a?.succeeded_count, 2);
    assert_eq!(b?.succeeded_count, 2);
    s?;
    assert_eq!(state.peak.load(Ordering::SeqCst), 2);
    assert_eq!(state.active.load(Ordering::SeqCst), 0);
    Ok(())
}

fn queued(
    work: &str,
    batch: &str,
    library: &Path,
    staging: &Path,
) -> dldr_storage::DownloadQueueEntry {
    dldr_storage::DownloadQueueEntry {
        work_id: work.into(),
        title: work.into(),
        thumbnail_url: None,
        batch_id: batch.into(),
        position: 0,
        account_id: None,
        library_root: library.to_string_lossy().into_owned(),
        download_root: staging.to_string_lossy().into_owned(),
        unpack_policy: "unpack_when_recognized".into(),
        replace_existing: false,
        status: "queued".into(),
        error_message: None,
    }
}

#[tokio::test]
async fn pausing_one_work_drains_it_without_cancelling_siblings_and_resume_has_a_new_generation(
) -> Result<()> {
    let library = library_with_works(3).await?;
    let root = tempfile::tempdir()?;
    let staging = root.path().join("staging");
    let final_root = root.path().join("library");
    let entries = (1..=3)
        .map(|i| queued(&format!("RJ{i:06}"), "batch", &final_root, &staging))
        .collect::<Vec<_>>();
    library.storage.enqueue_downloads(&entries).await?;
    let (state, mut started) = transfers(3, None, false);
    let mut request = BulkWorkDownloadRequest::new(
        ProductListQuery {
            sort: ProductSort::TitleAsc,
            ..Default::default()
        },
        &final_root,
        &staging,
    );
    request.queue_batch = Some("batch");
    let control = async {
        started.recv().await.unwrap();
        started.recv().await.unwrap();
        library.stop_queued_work("RJ000001", false).await.unwrap();
        state.gates["RJ000002"].add_permits(1);
        state.gates["RJ000003"].add_permits(1);
    };
    let (report, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            library.download_products_with_source_factory(request, 2, || Ok(source(&state))),
            control
        )
    })
    .await
    .unwrap();
    let report = report?;
    assert_eq!(
        (
            report.succeeded_count,
            report.failed_count,
            report.deferred_count
        ),
        (2, 0, 1)
    );
    assert_eq!(state.active.load(Ordering::SeqCst), 0);
    let saved = library.storage.download_queue().await?;
    assert_eq!(saved[0].status, "paused");
    assert_eq!(saved[1].status, "completed");
    assert!(!library.transfers().has_control("RJ000001"));
    std::fs::create_dir_all(staging.join("RJ000001"))?;
    std::fs::write(staging.join("RJ000001/partial-prefix"), b"keep me")?;
    library
        .storage
        .enqueue_downloads(&[queued("RJ000001", "resumed", &final_root, &staging)])
        .await?;
    let mut old = WorkDownloadRequest::new("RJ000001", &final_root, &staging);
    old.queue_batch = Some("batch");
    assert!(matches!(
        library
            .download_work_with_source(old, &FakeDownloadSource)
            .await,
        Err(LibraryError::QueueStopped)
    ));
    let mut resumed = WorkDownloadRequest::new("RJ000001", &final_root, &staging);
    resumed.queue_batch = Some("resumed");
    resumed.replace_existing = true;
    resumed.resume_partial = true;
    library
        .download_work_with_source(resumed, &FakeDownloadSource)
        .await?;
    assert_eq!(
        std::fs::read(final_root.join("RJ000001/partial-prefix"))?,
        b"keep me"
    );
    assert_eq!(
        library
            .storage
            .download_queue()
            .await?
            .iter()
            .find(|e| e.work_id == "RJ000001")
            .unwrap()
            .status,
        "completed"
    );
    Ok(())
}

#[tokio::test]
async fn queued_pause_cancel_and_restart_preserve_order_and_low_space_blocks_before_login(
) -> Result<()> {
    let library = library_with_works(3).await?;
    let root = tempfile::tempdir()?;
    let staging = root.path().join("staging");
    let final_root = root.path().join("library");
    library
        .storage
        .enqueue_downloads(
            &(1..=3)
                .map(|i| queued(&format!("RJ{i:06}"), "batch", &final_root, &staging))
                .collect::<Vec<_>>(),
        )
        .await?;
    library.stop_queued_work("RJ000002", false).await?;
    library.stop_queued_work("RJ000003", true).await?;
    library.storage.recover_download_queue().await?;
    let saved = library.storage.download_queue().await?;
    assert_eq!(
        saved.iter().map(|e| e.status.as_str()).collect::<Vec<_>>(),
        ["paused", "paused", "cancelled"]
    );
    assert!(saved.windows(2).all(|w| w[0].position < w[1].position));
    let budgets = library
        .transfers()
        .disk_budget(&final_root, &staging, Some(u64::MAX), true)?;
    assert_eq!(budgets.len(), 1);
    assert!(!budgets[0].sufficient);
    // Large cached capacity exercises the real admission path without a network source.
    let mut huge = work("RJ000001", "Huge", "Maker", "2026-01-01T00:00:00Z");
    huge.extra.insert("content_size".into(), json!(u64::MAX));
    let cached = cached_work_from_api(huge, "2026-01-01")?;
    library
        .storage
        .commit_account_sync(&AccountSyncCommit {
            sync_run_id: "huge".into(),
            account_id: "account-a".into(),
            started_at: "2026-01-01".into(),
            completed_at: "2026-01-01".into(),
            works: vec![cached],
            account_works: vec![AccountWork {
                work_id: "RJ000001".into(),
                purchased_at: None,
            }],
        })
        .await?;
    let (state, _) = transfers(3, None, false);
    let source = source(&state);
    let result = library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &final_root, &staging),
            &source,
        )
        .await;
    assert!(matches!(result, Err(LibraryError::InsufficientSpace(_))));
    assert!(source.login.lock().unwrap().is_none());
    assert!(library.transfers().set_limit(0).is_err());
    assert!(library.transfers().set_limit(5).is_err());
    Ok(())
}

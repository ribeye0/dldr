use super::*;

#[tokio::test]
async fn downloads_owned_work_and_records_local_path() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("download-owned-work");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let sink = RecordingDownloadProgressSink::default();

    let report = library
        .download_work_with_source(
            WorkDownloadRequest {
                progress_sink: Some(&sink),
                ..WorkDownloadRequest::new("RJ000001", &library_root, &download_root)
            },
            &FakeDownloadSource,
        )
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;
    let product = page
        .products
        .iter()
        .find(|product| product.work_id == "RJ000001")
        .expect("downloaded work in product list");
    let events = sink.events.lock().expect("download events lock");

    assert_eq!(report.work_id, "RJ000001");
    assert_eq!(report.account_id, "account-a");
    assert_eq!(report.file_count, 1);
    assert_eq!(report.download_state.status, WorkDownloadStatus::Downloaded);
    assert!(library_root.join("RJ000001/RJ000001.txt").exists());
    assert!(!library_root
        .join(format!("RJ000001/{SERIAL_INFORMATION_FILE_NAME}"))
        .exists());
    assert!(!download_root.join("RJ000001").exists());
    assert_eq!(product.download.status, WorkDownloadStatus::Downloaded);
    assert_eq!(
        product.download.local_path,
        Some(
            library_root
                .join("RJ000001")
                .canonicalize()?
                .to_string_lossy()
                .into_owned()
        )
    );
    assert!(matches!(
        events.first(),
        Some(WorkDownloadProgress::LoggingIn)
    ));
    assert!(matches!(
        events.last(),
        Some(WorkDownloadProgress::Completed)
    ));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn downloads_serial_work_and_writes_serial_information_file() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("download-serial-work");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    let serial_path = library_root
        .join("RJ000001")
        .join(SERIAL_INFORMATION_FILE_NAME);
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let report = library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &SerialDownloadSource,
        )
        .await?;
    let serial_content = tokio::fs::read_to_string(&serial_path).await?;

    assert_eq!(report.download_state.status, WorkDownloadStatus::Downloaded);
    assert!(serial_content.starts_with(SERIAL_INFORMATION_MARKER));
    assert!(serial_content.contains("Work ID: RJ000001"));
    assert!(serial_content.contains("Serial number"));
    assert!(serial_content.contains("ABC-123-SECRET"));
    assert!(!download_root.join("RJ000001").exists());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn download_cancellation_reaches_download_source_without_progress() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("download-cancellation-forwarding");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    let source = WaitingDownloadSource::new();
    let cancellation = CancellationToken::new();
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let download = library.download_work_with_source(
            WorkDownloadRequest {
                cancellation_token: Some(&cancellation),
                ..WorkDownloadRequest::new("RJ000001", &library_root, &download_root)
            },
            &source,
        );
        let cancel = async {
            while !source.entered_download() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            cancellation.cancel();
        };
        let (result, ()) = tokio::join!(download, cancel);
        result
    })
    .await
    .expect("download cancellation should complete promptly");
    let err = match result {
        Ok(_) => panic!("download should have been cancelled"),
        Err(error) => error,
    };

    assert!(matches!(
        err,
        LibraryError::Cancelled | LibraryError::Download(dldr_download::DownloadError::Cancelled)
    ));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn removes_downloaded_work_and_clears_state() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("remove-downloaded-work");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;

    let state = library
        .remove_work_download(WorkDownloadRemovalRequest::new(
            "RJ000001",
            &library_root,
            &download_root,
        ))
        .await?;

    assert_eq!(state.status, WorkDownloadStatus::NotDownloaded);
    assert!(!library_root.join("RJ000001").exists());
    assert!(!download_root.join("RJ000001").exists());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn marks_existing_library_folder_as_downloaded() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("mark-downloaded-work");
    let library_root = root.join("library");
    let local_path = library_root.join("manual").join("RJ000001");
    std::fs::create_dir_all(&local_path).unwrap();
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let state = library
        .mark_work_downloaded(WorkDownloadMarkRequest::new(
            "RJ000001",
            &library_root,
            &local_path,
        ))
        .await?;

    assert_eq!(state.status, WorkDownloadStatus::Downloaded);
    assert_eq!(
        state.local_path,
        Some(
            local_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )
    );
    assert_eq!(state.staging_path, None);
    assert_eq!(state.unpack_policy, Some("manual".to_owned()));

    let page = library.list_products(&ProductListQuery::default()).await?;
    let product = page
        .products
        .iter()
        .find(|product| product.work_id == "RJ000001")
        .expect("marked work in product list");
    assert_eq!(product.download.status, WorkDownloadStatus::Downloaded);

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn rejects_manual_download_folder_outside_library_root() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("mark-downloaded-outside-root");
    let library_root = root.join("library");
    let outside_path = root.join("outside").join("RJ000001");
    std::fs::create_dir_all(&library_root).unwrap();
    std::fs::create_dir_all(&outside_path).unwrap();
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let err = library
        .mark_work_downloaded(WorkDownloadMarkRequest::new(
            "RJ000001",
            &library_root,
            &outside_path,
        ))
        .await
        .unwrap_err();

    assert!(matches!(err, LibraryError::DownloadPathOutsideRoots(_)));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn redownload_replaces_existing_local_work_after_staging() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("redownload-replaces-work");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    let local_file = library_root.join("RJ000001/RJ000001.txt");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;
    tokio::fs::write(&local_file, b"user edit").await?;

    let report = library
        .download_work_with_source(
            WorkDownloadRequest {
                replace_existing: true,
                ..WorkDownloadRequest::new("RJ000001", &library_root, &download_root)
            },
            &FakeDownloadSource,
        )
        .await?;

    assert_eq!(report.download_state.status, WorkDownloadStatus::Downloaded);
    assert_eq!(tokio::fs::read(&local_file).await?, b"downloaded");
    assert!(!download_root.join("RJ000001").exists());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn replacement_db_failure_restores_old_bytes_and_complete_record() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("replacement-db-rollback");
    let library_root = root.join("library");
    let staging_root = root.join("staging");
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &staging_root),
            &FakeDownloadSource,
        )
        .await?;
    tokio::fs::write(library_root.join("RJ000001/RJ000001.txt"), b"old bytes!").await?;
    let previous = library.storage.work_download_state("RJ000001").await?;
    let mut tx = library.storage.begin_write().await?;
    tx.execute("CREATE TRIGGER reject_download BEFORE UPDATE ON work_downloads BEGIN SELECT RAISE(ABORT, 'injected DB failure'); END").await?;
    tx.commit().await?;
    let error = library
        .download_work_with_source(
            WorkDownloadRequest {
                replace_existing: true,
                ..WorkDownloadRequest::new("RJ000001", &library_root, &staging_root)
            },
            &FakeDownloadSource,
        )
        .await
        .unwrap_err();
    assert_eq!(error.failure_code(), "storage");
    assert_eq!(
        library.storage.work_download_state("RJ000001").await?,
        previous
    );
    assert_eq!(
        tokio::fs::read(library_root.join("RJ000001/RJ000001.txt")).await?,
        b"old bytes!"
    );
    assert!(staging_root.join("RJ000001/RJ000001.txt").is_file());
    assert!(library
        .storage
        .download_finalization("RJ000001")
        .await?
        .is_none());
    Ok(())
}

#[tokio::test]
async fn replacement_prompt_cancel_preserves_download_and_concurrent_consumers_share_login(
) -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("replacement-auth-cancel");
    let library_root = root.join("library");
    let staging = root.join("staging");
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &staging),
            &FakeDownloadSource,
        )
        .await?;
    let previous = library.storage.work_download_state("RJ000001").await?;
    let source = TwoFactorSyncSource::new();
    let cancel = ScriptedTwoFactorPrompt::with_responses([TwoFactorPromptResponse::Cancelled]);
    let result = library
        .download_work_with_source(
            WorkDownloadRequest {
                replace_existing: true,
                two_factor_prompt: Some(&cancel),
                ..WorkDownloadRequest::new("RJ000001", &library_root, &staging)
            },
            &source,
        )
        .await;
    assert!(result.unwrap_err().is_cancelled());
    assert_eq!(
        library.storage.work_download_state("RJ000001").await?,
        previous
    );
    assert_eq!(
        std::fs::read(library_root.join("RJ000001/RJ000001.txt"))?,
        b"downloaded"
    );

    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let source = TwoFactorSyncSource::new();
    let (sync, download) = tokio::join!(
        library.sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source
        ),
        library.download_work_with_source(
            WorkDownloadRequest {
                two_factor_prompt: Some(&prompt),
                ..WorkDownloadRequest::new("RJ000002", &library_root, &staging)
            },
            &source
        )
    );
    sync?;
    download?;
    assert_eq!(source.state().login_count, 1);
    assert_eq!(prompt.requests().len(), 1);
    Ok(())
}

#[tokio::test]
async fn replacement_rejects_staging_overlap_with_manual_content_before_deletion() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("manual-staging-overlap");
    let library_root = root.join("library");
    let staging = library_root.join("staging");
    let old = staging.join("RJ000001");
    std::fs::create_dir_all(&old)?;
    std::fs::write(old.join("content"), b"keep")?;
    library
        .mark_work_downloaded(WorkDownloadMarkRequest::new(
            "RJ000001",
            &library_root,
            &old,
        ))
        .await?;
    let previous = library.storage.work_download_state("RJ000001").await?;
    let result = library
        .download_work_with_source(
            WorkDownloadRequest {
                replace_existing: true,
                ..WorkDownloadRequest::new("RJ000001", &library_root, &staging)
            },
            &FakeDownloadSource,
        )
        .await;
    assert!(matches!(
        result,
        Err(LibraryError::DownloadPathOutsideRoots(_))
    ));
    assert_eq!(std::fs::read(old.join("content"))?, b"keep");
    assert_eq!(
        library.storage.work_download_state("RJ000001").await?,
        previous
    );
    Ok(())
}

#[tokio::test]
async fn manual_replacement_interrupted_before_intent_tracks_staging_for_later_delete() -> Result<()>
{
    struct InterruptedTransfer {
        cancel: bool,
    }
    impl_fake_auth_source!(InterruptedTransfer);
    #[async_trait]
    impl WorkDownloadSource for InterruptedTransfer {
        async fn download_plan(&self, id: &WorkId) -> Result<DownloadPlan> {
            FakeDownloadSource.download_plan(id).await
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
            sink: &mut (dyn FnMut(DownloadProgress) + Send),
        ) -> Result<DownloadedWork> {
            FakeDownloadSource
                .download_files(job, plan, cancellation, sink)
                .await?;
            if self.cancel {
                Err(LibraryError::Download(
                    dldr_download::DownloadError::Cancelled,
                ))
            } else {
                Err(LibraryError::SyncSource(
                    "injected transfer failure".to_owned(),
                ))
            }
        }
    }
    for cancel in [false, true] {
        let root = test_dir("manual-transfer-staging");
        std::fs::create_dir_all(&root)?;
        let storage_path = root.join("library.sqlite");
        let storage = Storage::open(&storage_path).await?;
        storage.run_migrations().await?;
        let library = Library::new(storage, Arc::new(InMemoryCredentialStore::new()));
        library.save_account(save_account_request(true)).await?;
        library
            .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
            .await?;
        let library_root = root.join("library");
        let staging_root = root.join("staging");
        let old = library_root.join("RJ000001");
        std::fs::create_dir_all(&old)?;
        std::fs::write(old.join("content"), b"manual old")?;
        let mut previous = library
            .mark_work_downloaded(WorkDownloadMarkRequest::new(
                "RJ000001",
                &library_root,
                &old,
            ))
            .await?;
        assert!(previous.staging_path.is_none());
        let error = library
            .download_work_with_source(
                WorkDownloadRequest {
                    replace_existing: true,
                    ..WorkDownloadRequest::new("RJ000001", &library_root, &staging_root)
                },
                &InterruptedTransfer { cancel },
            )
            .await
            .unwrap_err();
        assert_eq!(error.is_cancelled(), cancel);
        assert!(library
            .storage
            .download_finalization("RJ000001")
            .await?
            .is_none());
        assert_eq!(std::fs::read(old.join("content"))?, b"manual old");
        let staging = staging_root.join("RJ000001").canonicalize()?;
        previous.staging_path = Some(staging.to_string_lossy().into_owned());
        assert_eq!(
            library.storage.work_download_state("RJ000001").await?,
            previous
        );
        drop(library);
        let reopened = Library::new(
            Storage::open(&storage_path).await?,
            Arc::new(InMemoryCredentialStore::new()),
        );
        reopened
            .remove_work_download(WorkDownloadRemovalRequest::new(
                "RJ000001",
                &library_root,
                &staging_root,
            ))
            .await?;
        assert!(!staging.exists());
        assert!(!old.exists());
    }
    Ok(())
}

#[tokio::test]
async fn cancelling_replacement_during_transfer_or_preparation_keeps_old_state() -> Result<()> {
    struct CancelProgress {
        token: CancellationToken,
        at_finalization: bool,
    }
    impl WorkDownloadProgressSink for CancelProgress {
        fn emit(&self, progress: WorkDownloadProgress) {
            if (self.at_finalization && matches!(progress, WorkDownloadProgress::Finalizing))
                || (!self.at_finalization && matches!(progress, WorkDownloadProgress::Download(_)))
            {
                self.token.cancel();
            }
        }
    }
    for at_finalization in [false, true] {
        let library = migrated_library().await?;
        library.save_account(save_account_request(true)).await?;
        library
            .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
            .await?;
        let root = test_dir("replacement-cancel-boundary");
        let library_root = root.join("library");
        let staging = root.join("staging");
        library
            .download_work_with_source(
                WorkDownloadRequest::new("RJ000001", &library_root, &staging),
                &FakeDownloadSource,
            )
            .await?;
        std::fs::write(library_root.join("RJ000001/RJ000001.txt"), b"old content")?;
        let old = library.storage.work_download_state("RJ000001").await?;
        let token = CancellationToken::new();
        let sink = CancelProgress {
            token: token.clone(),
            at_finalization,
        };
        let error = library
            .download_work_with_source(
                WorkDownloadRequest {
                    replace_existing: true,
                    cancellation_token: Some(&token),
                    progress_sink: Some(&sink),
                    ..WorkDownloadRequest::new("RJ000001", &library_root, &staging)
                },
                &FakeDownloadSource,
            )
            .await
            .unwrap_err();
        assert!(error.is_cancelled());
        assert_eq!(library.storage.work_download_state("RJ000001").await?, old);
        assert_eq!(
            std::fs::read(library_root.join("RJ000001/RJ000001.txt"))?,
            b"old content"
        );
        assert!(staging.join("RJ000001/RJ000001.txt").is_file());
    }
    Ok(())
}

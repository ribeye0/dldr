use super::*;

#[tokio::test]
async fn saves_account_and_password_reference() -> Result<()> {
    let library = migrated_library().await?;

    let account = library.save_account(save_account_request(true)).await?;

    assert_eq!(account.id, "account-a");
    assert_eq!(
        account.credential_ref,
        Some("account:account-a:password".to_owned())
    );
    assert!(library.account_has_saved_password(&account)?);

    Ok(())
}

#[tokio::test]
async fn save_account_preserves_enabled_state_for_existing_accounts() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library.set_account_enabled("account-a", false).await?;

    let account = library.save_account(save_account_request(true)).await?;
    assert!(
        !account.enabled,
        "saving details must not re-enable an account"
    );

    library.set_account_enabled("account-a", true).await?;
    let mut request = save_account_request(true);
    request.enabled = false;
    let account = library.save_account(request).await?;
    assert!(
        account.enabled,
        "only the dedicated toggle changes existing state"
    );
    Ok(())
}

#[tokio::test]
async fn save_account_uses_initial_enabled_state_for_new_accounts() -> Result<()> {
    let library = migrated_library().await?;
    let account = library
        .save_account(SaveAccountRequest::new("New account"))
        .await?;
    assert!(account.enabled);

    let mut request = SaveAccountRequest::new("Initially disabled");
    request.enabled = false;
    assert!(!library.save_account(request).await?.enabled);
    Ok(())
}

#[tokio::test]
async fn save_account_queued_after_disable_during_auth_keeps_account_disabled() -> Result<()> {
    use std::future::Future;
    use std::task::Poll;

    #[derive(Default)]
    struct GatedPrompt {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    #[async_trait]
    impl TwoFactorPrompt for GatedPrompt {
        async fn request_code(&self, _: TwoFactorPromptRequest) -> Result<TwoFactorPromptResponse> {
            self.entered.notify_one();
            self.release.notified().await;
            Ok(TwoFactorPromptResponse::Code(
                VALID_TWO_FACTOR_CODE.to_owned(),
            ))
        }
    }

    tokio::time::timeout(Duration::from_secs(2), async {
        let library = migrated_library().await?;
        let account = library.save_account(save_account_request(true)).await?;
        let source = TwoFactorSyncSource::new();
        let prompt = GatedPrompt::default();
        let mut authentication = Box::pin(library.authenticate_account(
            &account,
            None,
            Some(&prompt),
            None,
            &source,
        ));
        tokio::select! {
            result = &mut authentication => panic!("authentication finished before gate: {result:?}"),
            () = prompt.entered.notified() => {}
        }

        // Poll each mutation once while auth holds the lock, establishing FIFO order
        // without sleeps or assumptions about task scheduling.
        let mut disable = Box::pin(library.set_account_enabled(&account.id, false));
        std::future::poll_fn(|cx| {
            assert!(disable.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        let mut request = save_account_request(true);
        request.password = None;
        request.label = "Updated label".to_owned();
        assert!(request.enabled); // The adapter's creation default, not current state.
        let mut save = Box::pin(library.save_account(request));
        std::future::poll_fn(|cx| {
            assert!(save.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;

        prompt.release.notify_one();
        authentication.await?;
        let (disabled, saved) = tokio::join!(disable, save);
        disabled?;
        let saved = saved?;
        assert_eq!(saved.label, "Updated label");
        assert!(!saved.enabled, "queued save undid the completed disable");
        assert!(!library.find_account(&account.id).await?.enabled);
        Ok(())
    })
    .await
    .expect("gated account mutations must finish")
}

#[tokio::test]
async fn removes_account_and_saved_credential() -> Result<()> {
    let storage = Storage::open_in_memory().await?;
    storage.run_migrations().await?;
    let credentials = Arc::new(InMemoryCredentialStore::new());
    let library = Library::new(storage, credentials.clone());

    let account = library.save_account(save_account_request(true)).await?;
    let credential_ref = CredentialRef::account_password(&account.id)?;

    assert!(credentials.load_password(&credential_ref)?.is_some());

    let report = library.remove_account(&account.id).await?;

    assert_eq!(report.account_id, "account-a");
    assert_eq!(report.label, "Account A");
    assert!(report.credential_deleted);
    assert!(credentials.load_password(&credential_ref)?.is_none());
    assert_eq!(library.accounts().await?, Vec::new());

    Ok(())
}

#[tokio::test]
async fn cancelled_two_factor_download_records_cancellation_and_stops_bulk() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("two-factor-cancel-download");
    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::with_responses([TwoFactorPromptResponse::Cancelled]);
    let error = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                two_factor_prompt: Some(&prompt),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &root.join("library"),
                    &root.join("staging"),
                )
            },
            &source,
        )
        .await
        .expect_err("cancelled bulk must stop");
    assert_eq!(error.failure_code(), "cancelled");
    assert_eq!(prompt.requests().len(), 1);
    let page = library.list_products(&ProductListQuery::default()).await?;
    assert_eq!(
        page.products
            .iter()
            .filter(|p| p.download.status == WorkDownloadStatus::Cancelled)
            .count(),
        1
    );
    assert_eq!(
        page.products
            .iter()
            .filter(|p| p.download.status == WorkDownloadStatus::NotDownloaded)
            .count(),
        1
    );
    assert_eq!(std::fs::read_dir(root.join("library"))?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn authentication_http_waits_cancel_and_release_account_lock() -> Result<()> {
    struct PendingSource {
        phase: &'static str,
        entered: tokio::sync::Notify,
    }
    impl PendingSource {
        async fn wait(&self, phase: &str) {
            if self.phase == phase {
                self.entered.notify_one();
                std::future::pending::<()>().await;
            }
        }
    }
    #[async_trait]
    impl DlsiteAuthSource for PendingSource {
        async fn restore_session(&self, _: &SessionSnapshot) -> Result<()> {
            self.wait("restore").await;
            Ok(())
        }
        async fn validate_session(&self) -> Result<bool> {
            self.wait("validate").await;
            Ok(false)
        }
        async fn begin_login(&self, _: &Credentials) -> Result<LoginOutcome> {
            self.wait("login").await;
            Ok(LoginOutcome::TwoFactorRequired(
                TwoFactorSyncSource::challenge(),
            ))
        }
        async fn complete_two_factor(
            &self,
            _: &TwoFactorChallenge,
            _: &str,
        ) -> Result<SessionSnapshot> {
            self.wait("submit").await;
            panic!("unexpected code submission")
        }
    }
    for phase in ["restore", "validate", "login", "submit"] {
        let library = migrated_library().await?;
        let account = library.save_account(save_account_request(true)).await?;
        let session_ref = CredentialRef::account_session(&account.id)?;
        library
            .credentials
            .save_password(&session_ref, "old-session")?;
        let token = CancellationToken::new();
        let source = PendingSource {
            phase,
            entered: tokio::sync::Notify::new(),
        };
        let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                library.authenticate_account(&account, None, Some(&prompt), Some(&token), &source),
                async {
                    source.entered.notified().await;
                    token.cancel();
                }
            )
            .0
        })
        .await
        .expect("authentication must observe cancellation");
        assert!(result
            .expect_err("pending request must cancel")
            .is_cancelled());
        assert!(library.login_lock(&account.id).try_lock().is_ok());
        assert!(prompt.requests().len() <= 1);
        if matches!(phase, "restore" | "validate") {
            assert_eq!(
                library.credentials.load_password(&session_ref)?.as_deref(),
                Some("old-session")
            );
        } else {
            assert!(library.credentials.load_password(&session_ref)?.is_none());
        }
        library
            .authenticate_account(&account, None, None, None, &sync_source())
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn exhausted_two_factor_retries_remain_failed() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new(["000000", "000000", "000000"]);
    let error = library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await
        .unwrap_err();
    assert!(!error.is_cancelled());
    assert_eq!(error.failure_code(), "two_factor_rejected");
    assert_eq!(
        library.storage.sync_runs_for_account("account-a").await?[0].status,
        dldr_storage::SyncRunStatus::Failed
    );
    assert_eq!(prompt.requests().len(), 3);
    Ok(())
}

#[tokio::test]
async fn session_restore_distinguishes_invalid_data_from_validation_network_errors() -> Result<()> {
    struct Probe {
        outcome: &'static str,
        logins: std::sync::atomic::AtomicUsize,
    }
    #[async_trait]
    impl DlsiteAuthSource for Probe {
        async fn restore_session(&self, _: &SessionSnapshot) -> Result<()> {
            if self.outcome == "malformed" {
                return Err(LibraryError::Json(
                    serde_json::from_str::<Value>("invalid").unwrap_err(),
                ));
            }
            Ok(())
        }
        async fn validate_session(&self) -> Result<bool> {
            if self.outcome == "network" {
                return Err(LibraryError::SyncSource("offline".to_owned()));
            }
            Ok(self.outcome == "valid")
        }
        async fn begin_login(&self, _: &Credentials) -> Result<LoginOutcome> {
            self.logins
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(LoginOutcome::Authorized(SessionSnapshot {
                cookies_json: "fresh".to_owned(),
            }))
        }
        async fn complete_two_factor(
            &self,
            _: &TwoFactorChallenge,
            _: &str,
        ) -> Result<SessionSnapshot> {
            unreachable!()
        }
    }
    for outcome in ["valid", "expired", "malformed", "network"] {
        let library = migrated_library().await?;
        let account = library.save_account(save_account_request(true)).await?;
        let session_ref = CredentialRef::account_session(&account.id)?;
        library.credentials.save_password(&session_ref, "stored")?;
        // New Library instance, same durable stores; no process-local auth state reused.
        let library = Library::new(library.storage.clone(), library.credentials.clone());
        let source = Probe {
            outcome,
            logins: std::sync::atomic::AtomicUsize::new(0),
        };
        let result = library
            .authenticate_account(&account, None, None, None, &source)
            .await;
        let should_login = matches!(outcome, "expired" | "malformed");
        assert_eq!(result.is_err(), outcome == "network");
        assert_eq!(
            source.logins.load(std::sync::atomic::Ordering::SeqCst),
            usize::from(should_login)
        );
        assert_eq!(
            library.credentials.load_password(&session_ref)?.as_deref(),
            Some(if should_login { "fresh" } else { "stored" })
        );
    }
    Ok(())
}

#[tokio::test]
async fn cli_login_verifies_before_saving_reuses_session_and_logout_keeps_works() -> Result<()> {
    let library = migrated_library().await?;
    let mut request = save_account_request(true);
    request.id = None;
    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new(["000000", VALID_TWO_FACTOR_CODE]);
    let account = library
        .login_account_with_source(request, Some(&prompt), None, &source)
        .await?;
    assert!(account.last_login_at.is_some());
    assert!(account.enabled);
    let password_ref = CredentialRef::account_password(&account.id)?;
    let session_ref = CredentialRef::account_session(&account.id)?;
    assert_eq!(
        library.credentials.load_password(&password_ref)?.as_deref(),
        Some("secret")
    );
    assert_eq!(
        library.credentials.load_password(&session_ref)?.as_deref(),
        Some(TWO_FACTOR_SESSION)
    );
    library
        .sync_account_with_source(AccountSyncRequest::new(&account.id), &source)
        .await?;
    assert_eq!(source.state().login_count, 1);
    let work = library.product_detail("RJ000001").await?;
    library.logout_account(&account.id).await?;
    let logged_out = library.find_account(&account.id).await?;
    assert!(!logged_out.enabled);
    assert!(logged_out.credential_ref.is_none());
    assert!(library.credentials.load_password(&password_ref)?.is_none());
    assert!(library.credentials.load_password(&session_ref)?.is_none());
    assert_eq!(
        library.product_detail("RJ000001").await?.download,
        work.download
    );
    let mut again = save_account_request(true);
    again.id = Some(account.id.clone());
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let account = library
        .login_account_with_source(again, Some(&prompt), None, &source)
        .await?;
    assert!(account.enabled);
    assert_eq!(
        library.product_detail("RJ000001").await?.owners[0].account_id,
        account.id
    );
    Ok(())
}

#[tokio::test]
async fn cli_rejected_or_cancelled_login_preserves_existing_identity_and_vault() -> Result<()> {
    let library = migrated_library().await?;
    let account = library.save_account(save_account_request(true)).await?;
    let session_ref = CredentialRef::account_session(&account.id)?;
    library
        .credentials
        .save_password(&session_ref, "previous-session")?;
    for responses in [
        vec![TwoFactorPromptResponse::Code("bad".into()); 3],
        vec![TwoFactorPromptResponse::Cancelled],
    ] {
        let mut request = save_account_request(true);
        request.label = "must not overwrite".into();
        let prompt = ScriptedTwoFactorPrompt::with_responses(responses);
        assert!(library
            .login_account_with_source(request, Some(&prompt), None, &TwoFactorSyncSource::new())
            .await
            .is_err());
        assert_eq!(library.find_account(&account.id).await?, account);
        assert_eq!(
            library.credentials.load_password(&session_ref)?.as_deref(),
            Some("previous-session")
        );
        assert_eq!(
            library
                .credentials
                .load_password(&CredentialRef::account_password(&account.id)?)?
                .as_deref(),
            Some("secret")
        );
    }
    let mut request = save_account_request(true);
    request.id = None;
    let token = CancellationToken::new();
    token.cancel();
    let source = TwoFactorSyncSource::new();
    assert!(library
        .login_account_with_source(request, None, Some(&token), &source)
        .await
        .unwrap_err()
        .is_cancelled());
    assert_eq!(source.state().login_count, 0);
    assert_eq!(library.accounts().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn two_factor_sync_completes_with_a_prompted_code() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let report = library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await?;

    assert_eq!(report.cached_work_count, 2);
    assert_eq!(source.state().submitted_codes, vec![VALID_TWO_FACTOR_CODE]);

    let requests = prompt.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].account_id, "account-a");
    assert_eq!(requests[0].attempt, 1);
    assert!(!requests[0].previous_code_rejected);

    Ok(())
}

#[tokio::test]
async fn rejected_two_factor_code_is_prompted_again() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new(["000000", VALID_TWO_FACTOR_CODE]);

    library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await?;

    assert_eq!(
        source.state().submitted_codes,
        vec!["000000", VALID_TWO_FACTOR_CODE]
    );

    let requests = prompt.requests();
    assert_eq!(requests.len(), 2);
    assert!(!requests[0].previous_code_rejected);
    assert_eq!(requests[1].attempt, 2);
    assert!(requests[1].previous_code_rejected);

    Ok(())
}

#[tokio::test]
async fn cancelled_two_factor_prompt_reports_a_cancelled_sync() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::with_responses([TwoFactorPromptResponse::Cancelled]);
    let result = library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await;

    let error = result.expect_err("cancelling the prompt must not sync");

    assert!(matches!(
        &error,
        LibraryError::TwoFactorCancelled(account_id) if account_id == "account-a"
    ));
    // `dldr-jobs` recognizes only this exact code as a cancellation, so the job it belongs
    // to finishes as Cancelled rather than Failed.
    assert_eq!(error.failure_code(), "cancelled");
    assert_eq!(
        error.support_details()["failureKind"],
        "two_factor_cancelled"
    );
    assert!(source.state().submitted_codes.is_empty());

    let runs = library.storage().sync_runs_for_account("account-a").await?;
    assert_eq!(runs[0].status, dldr_storage::SyncRunStatus::Cancelled);

    Ok(())
}

#[tokio::test]
async fn timed_out_two_factor_prompt_fails_sync() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::with_responses([TwoFactorPromptResponse::TimedOut]);
    let result = library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await;

    let error = result.expect_err("an unanswered prompt must not sync");

    assert!(matches!(
        &error,
        LibraryError::TwoFactorTimedOut(account_id) if account_id == "account-a"
    ));
    // Unlike cancellation, nobody chose this outcome, so it stays a visible failure.
    assert_eq!(error.failure_code(), "two_factor_timed_out");
    assert!(source.state().submitted_codes.is_empty());

    Ok(())
}

#[tokio::test]
async fn cancelling_a_job_waiting_on_the_login_lock_does_not_wait_for_the_dialog() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    // Hold the account's login lock the way a job sitting on an open dialog would.
    let login_lock = library.login_lock("account-a");
    let held = login_lock.lock().await;

    let cancellation = CancellationToken::new();
    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let queued = library.sync_account_with_source(
        AccountSyncRequest {
            two_factor_prompt: Some(&prompt),
            cancellation_token: Some(&cancellation),
            ..AccountSyncRequest::new("account-a")
        },
        &source,
    );

    let result = tokio::join!(queued, async {
        // Let the queued sync reach the lock before cancelling it.
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancellation.cancel();
    })
    .0;

    assert!(matches!(result, Err(LibraryError::Cancelled)));
    // The lock was never released, so the queued job gave up on its own.
    assert!(prompt.requests().is_empty());
    drop(held);

    Ok(())
}

#[tokio::test]
async fn two_factor_challenge_without_a_prompt_is_reported() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let result = library
        .sync_account_with_source(
            AccountSyncRequest::new("account-a"),
            &TwoFactorSyncSource::new(),
        )
        .await;

    assert!(matches!(
        result,
        Err(LibraryError::TwoFactorPromptUnavailable(account_id)) if account_id == "account-a"
    ));

    Ok(())
}

#[tokio::test]
async fn stored_session_skips_the_two_factor_prompt() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let request = || AccountSyncRequest {
        two_factor_prompt: Some(&prompt),
        ..AccountSyncRequest::new("account-a")
    };

    library.sync_account_with_source(request(), &source).await?;
    library.sync_account_with_source(request(), &source).await?;

    // The second sync reused the stored session instead of logging in again.
    assert_eq!(prompt.requests().len(), 1);
    assert_eq!(source.state().login_count, 1);
    assert_eq!(
        source.state().restored_session.as_deref(),
        Some(TWO_FACTOR_SESSION)
    );

    Ok(())
}

#[tokio::test]
async fn session_is_not_stored_when_the_account_does_not_remember_its_password() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(false)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE, VALID_TWO_FACTOR_CODE]);
    let request = || AccountSyncRequest {
        password: Some("secret"),
        two_factor_prompt: Some(&prompt),
        ..AccountSyncRequest::new("account-a")
    };

    library.sync_account_with_source(request(), &source).await?;

    // The session cookie is as good as the password, so an account that keeps neither
    // leaves nothing behind for the next job to reuse.
    let session_ref = CredentialRef::account_session("account-a")?;

    assert!(library.credentials.load_password(&session_ref)?.is_none());

    library.sync_account_with_source(request(), &source).await?;

    assert_eq!(prompt.requests().len(), 2);
    assert_eq!(source.state().login_count, 2);

    Ok(())
}

#[tokio::test]
async fn session_is_stored_when_the_account_remembers_its_password() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);

    library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await?;

    let session_ref = CredentialRef::account_session("account-a")?;

    assert_eq!(
        library.credentials.load_password(&session_ref)?.as_deref(),
        Some(TWO_FACTOR_SESSION)
    );

    Ok(())
}

#[tokio::test]
async fn changing_the_password_forgets_the_stored_session() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE, VALID_TWO_FACTOR_CODE]);
    let request = || AccountSyncRequest {
        two_factor_prompt: Some(&prompt),
        ..AccountSyncRequest::new("account-a")
    };

    library.sync_account_with_source(request(), &source).await?;
    library.save_account(save_account_request(true)).await?;
    library.sync_account_with_source(request(), &source).await?;

    assert_eq!(prompt.requests().len(), 2);
    assert_eq!(source.state().login_count, 2);

    Ok(())
}

#[tokio::test]
async fn missing_password_blocks_sync() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(false)).await?;

    assert!(matches!(
        library
            .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
            .await,
        Err(LibraryError::MissingPassword(account_id)) if account_id == "account-a"
    ));

    Ok(())
}

use super::*;

impl Library {
    pub async fn accounts(&self) -> Result<Vec<Account>> {
        Ok(self.storage.accounts().await?)
    }

    pub async fn save_account(&self, request: SaveAccountRequest) -> Result<Account> {
        let account_id = request
            .id
            .clone()
            .unwrap_or_else(|| format!("account-{}", Uuid::new_v4()));
        let lock = self.login_lock(&account_id);
        let _guard = lock.lock().await;
        self.save_account_unlocked(request, account_id).await
    }

    pub(super) async fn save_account_unlocked(
        &self,
        request: SaveAccountRequest,
        account_id: String,
    ) -> Result<Account> {
        let existing_account = match request.id.as_deref() {
            Some(account_id) => match self.find_account(account_id).await {
                Ok(account) => Some(account),
                Err(LibraryError::AccountNotFound(_)) => None,
                Err(error) => return Err(error),
            },
            None => None,
        };
        let credential_ref = self.save_account_credential(
            &account_id,
            request.password.as_deref(),
            request.remember_password,
            existing_account.as_ref().and_then(|account| {
                account
                    .credential_ref
                    .as_deref()
                    .and_then(|value| CredentialRef::new(value.to_owned()).ok())
            }),
        )?;

        let account = AccountUpsert {
            id: account_id.clone(),
            label: request.label,
            login_name: request.login_name,
            credential_ref: credential_ref.map(|value| value.to_string()),
            enabled: existing_account
                .as_ref()
                .map_or(request.enabled, |account| account.enabled),
        };

        // Changed sign-in details invalidate whatever session was stored for this account.
        let identity_changed = request.password.is_some()
            || existing_account
                .as_ref()
                .is_none_or(|existing| existing.login_name != account.login_name);

        if identity_changed || !request.remember_password {
            self.forget_account_session(&account_id)?;
        }

        self.storage.save_account(&account).await?;
        self.find_account(&account_id).await
    }

    pub async fn set_account_enabled(&self, account_id: &str, enabled: bool) -> Result<()> {
        let lock = self.login_lock(account_id);
        let _guard = lock.lock().await;
        Ok(self
            .storage
            .set_account_enabled(account_id, enabled)
            .await?)
    }

    pub async fn remove_account(&self, account_id: &str) -> Result<AccountRemovalReport> {
        let lock = self.login_lock(account_id);
        let _guard = lock.lock().await;
        let account = self.find_account(account_id).await?;
        let credential_ref = account
            .credential_ref
            .as_deref()
            .map(|value| CredentialRef::new(value.to_owned()))
            .transpose()?;
        let credential_deleted = if let Some(credential_ref) = credential_ref {
            self.credentials.delete_password(&credential_ref)?;
            true
        } else {
            false
        };

        self.forget_account_session(account_id)?;
        self.storage.delete_account(account_id).await?;

        Ok(AccountRemovalReport {
            account_id: account.id,
            label: account.label,
            credential_deleted,
        })
    }

    pub fn account_has_saved_password(&self, account: &Account) -> Result<bool> {
        let Some(credential_ref) = account.credential_ref.as_deref() else {
            return Ok(false);
        };
        let credential_ref = CredentialRef::new(credential_ref.to_owned())?;

        Ok(self.credentials.load_password(&credential_ref)?.is_some())
    }

    pub async fn sync_account_with_source<S>(
        &self,
        request: AccountSyncRequest<'_>,
        source: &S,
    ) -> Result<AccountSyncReport>
    where
        S: AccountSyncSource + Sync,
    {
        let account = self.find_account(request.account_id).await?;

        if !account.enabled {
            return Err(LibraryError::AccountDisabled(account.id));
        }

        let sync_run_id = format!("sync-{}", Uuid::new_v4());
        let started_at = now_string();
        let result = self
            .sync_account_inner(&account, &sync_run_id, &started_at, request, source)
            .await;

        if let Err(error) = &result {
            let completed_at = now_string();
            if error.is_cancelled() {
                let _ = self
                    .storage
                    .record_sync_cancellation(&SyncCancellation {
                        sync_run_id,
                        account_id: account.id,
                        started_at,
                        completed_at,
                    })
                    .await;
            } else {
                let _ = self
                    .storage
                    .record_sync_failure(&SyncFailure {
                        sync_run_id,
                        account_id: account.id,
                        started_at,
                        completed_at,
                        error_code: Some(error.failure_code().to_owned()),
                        error_message: Some(error.support_message()),
                    })
                    .await;
            }
        }

        result
    }

    pub(super) async fn sync_account_inner<S>(
        &self,
        account: &Account,
        sync_run_id: &str,
        started_at: &str,
        request: AccountSyncRequest<'_>,
        source: &S,
    ) -> Result<AccountSyncReport>
    where
        S: AccountSyncSource + Sync,
    {
        request.check_cancelled()?;
        request.emit(SyncProgress::LoggingIn);

        self.authenticate_account(
            account,
            request.password,
            request.two_factor_prompt,
            request.cancellation_token,
            source,
        )
        .await?;

        request.check_cancelled()?;
        request.emit(SyncProgress::LoadingCount);
        let count = source.content_count().await?;

        request.check_cancelled()?;
        request.emit(SyncProgress::LoadingPurchases);
        let purchases = source.purchases().await?;
        let purchased_ids = purchases
            .iter()
            .map(|purchase| purchase.id.clone())
            .collect::<Vec<_>>();

        request.check_cancelled()?;
        request.emit(SyncProgress::LoadingWorks {
            work_count: purchased_ids.len(),
        });
        let works = source.works(&purchased_ids).await?;

        let completed_at = now_string();
        let storage_sync = build_storage_sync(
            &account.id,
            sync_run_id,
            started_at,
            &completed_at,
            purchases,
            works,
        )?;

        request.check_cancelled()?;
        request.emit(SyncProgress::Committing {
            work_count: storage_sync.commit.works.len(),
        });
        self.storage
            .commit_account_sync(&storage_sync.commit)
            .await?;

        let report = AccountSyncReport {
            account_id: account.id.clone(),
            sync_run_id: sync_run_id.to_owned(),
            purchased_count: storage_sync.commit.account_works.len(),
            cached_work_count: storage_sync.commit.works.len(),
            missing_detail_count: storage_sync.missing_detail_count,
            page_limit: count.page_limit,
            concurrency: count.concurrency,
        };

        request.emit(SyncProgress::Completed {
            sync_run_id: sync_run_id.to_owned(),
            cached_work_count: report.cached_work_count,
        });

        Ok(report)
    }

    pub(super) async fn find_account(&self, account_id: &str) -> Result<Account> {
        self.storage
            .accounts()
            .await?
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| LibraryError::AccountNotFound(account_id.to_owned()))
    }
}

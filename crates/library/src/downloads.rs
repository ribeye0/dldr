use super::*;

impl Library {
    pub(super) fn work_lock(&self, work_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.work_locks
            .lock()
            .expect("work lock registry")
            .entry(work_id.to_owned())
            .or_default()
            .clone()
    }

    pub(super) async fn recover_download(&self, work_id: &str, roots: &[&Path]) -> Result<()> {
        self.recover_download_inner(work_id, roots, false).await
    }

    pub(super) async fn recover_download_inner(
        &self,
        work_id: &str,
        roots: &[&Path],
        remove_previous_staging: bool,
    ) -> Result<()> {
        let Some(record) = self.storage.download_finalization(work_id).await? else {
            return Ok(());
        };
        let installation = finalization::Installation { record };
        installation
            .validate(roots)
            .map_err(|error| installation.required(&error.support_message()))?;
        let result = if installation.record.committed {
            installation.cleanup().await
        } else {
            installation.rollback().await
        };
        result.map_err(|error| installation.required(&error.support_message()))?;
        if Path::new(&installation.record.staging_path).try_exists()? {
            let state = self.storage.work_download_state(work_id).await?;
            if state.status == WorkDownloadStatus::NotDownloaded {
                return Err(installation.required("missing download row for staging removal"));
            }
            if state.staging_path.as_deref() != Some(&installation.record.staging_path) {
                // Keep intent until the row durably tracks restored staging. Only deletion
                // may remove a different staging path before replacing its reference.
                if let Some(previous) = state.staging_path.as_deref().map(Path::new) {
                    if previous.try_exists()? {
                        let previous =
                            finalization::checked_child(&previous.canonicalize()?, roots)?;
                        let staging = Path::new(&installation.record.staging_path);
                        if previous != staging {
                            if !remove_previous_staging {
                                return Err(installation.required(&format!(
                                    "previous staging still exists at {}; deletion with root authority is required",
                                    previous.display()
                                )));
                            }
                            for path in [
                                Some(staging),
                                Some(Path::new(&installation.record.final_path)),
                                Some(Path::new(&installation.record.temporary_path)),
                                installation.record.old_path.as_deref().map(Path::new),
                            ]
                            .into_iter()
                            .flatten()
                            {
                                if previous.starts_with(path) || path.starts_with(&previous) {
                                    return Err(installation
                                        .required("previous staging overlaps recovery material"));
                                }
                            }
                            remove_existing_download_path(&previous, roots).await?;
                        }
                    }
                }
                self.save_download_staging(
                    work_id,
                    &state,
                    Path::new(&installation.record.staging_path),
                )
                .await?;
            }
        }
        self.storage
            .clear_download_finalization(work_id, &installation.record.operation_id)
            .await?;
        Ok(())
    }

    pub(super) async fn save_download_staging(
        &self,
        work_id: &str,
        state: &WorkDownloadState,
        staging: &Path,
    ) -> Result<()> {
        let incomplete = || {
            LibraryError::RecoveryRequired(format!(
                "incomplete download row for {work_id}; retained staging: {}",
                staging.display()
            ))
        };
        self.storage
            .save_work_download(&WorkDownloadUpdate {
                work_id: work_id.to_owned(),
                status: state.status,
                local_path: state.local_path.clone(),
                staging_path: Some(staging.to_string_lossy().into_owned()),
                unpack_policy: state.unpack_policy.clone().ok_or_else(incomplete)?,
                bytes_received: state.bytes_received,
                bytes_total: state.bytes_total,
                error_code: state.error_code.clone(),
                error_message: state.error_message.clone(),
                started_at: state.started_at.clone(),
                completed_at: state.completed_at.clone(),
                updated_at: state.updated_at.clone().ok_or_else(incomplete)?,
            })
            .await?;
        Ok(())
    }

    pub(super) async fn download_work_unmanaged<S>(
        &self,
        request: WorkDownloadRequest<'_>,
        source: &S,
    ) -> Result<WorkDownloadReport>
    where
        S: WorkDownloadSource + Sync,
    {
        let lock = self.work_lock(request.work_id);
        let _guard = acquire_login_lock(&lock, request.cancellation_token).await?;
        self.recover_download(
            request.work_id,
            &[request.library_root, request.download_root],
        )
        .await?;
        let previous = self.storage.work_download_state(request.work_id).await?;
        let preserve_previous = previous.status == WorkDownloadStatus::Downloaded;
        let (staging_dir, final_dir) =
            finalization::paths(request.work_id, request.library_root, request.download_root)?;
        if let Some(old) = previous
            .local_path
            .as_deref()
            .map(Path::new)
            .filter(|p| p.exists())
        {
            if std::fs::symlink_metadata(old)?.file_type().is_symlink() {
                return Err(LibraryError::DownloadPathOutsideRoots(old.to_owned()));
            }
            let old = old.canonicalize()?;
            finalization::checked_child(&old, &[request.library_root, request.download_root])?;
            if old.starts_with(&staging_dir) || staging_dir.starts_with(&old) {
                return Err(LibraryError::DownloadPathOutsideRoots(old));
            }
        }
        let account = self
            .storage
            .download_account_for_work(request.work_id, request.account_id)
            .await
            .map_err(|error| match error {
                StorageError::NotFound { .. } => {
                    LibraryError::DownloadAccountNotFound(request.work_id.to_owned())
                }
                error => LibraryError::Storage(error),
            })?;
        let started_at = now_string();
        let work_id = WorkId::from(request.work_id.to_owned());
        let unpack_policy = request.unpack_policy;
        let result = self
            .download_work_inner(
                &account,
                &work_id,
                &staging_dir,
                &final_dir,
                &started_at,
                request,
                source,
            )
            .await;

        if let Err(error) = &result {
            if preserve_previous {
                return result;
            }
            let completed_at = now_string();
            let status = if error.is_cancelled() {
                WorkDownloadStatus::Cancelled
            } else {
                WorkDownloadStatus::Failed
            };
            let _ = self
                .storage
                .save_work_download(&WorkDownloadUpdate {
                    work_id: request.work_id.to_owned(),
                    status,
                    local_path: Some(final_dir.to_string_lossy().into_owned()),
                    staging_path: Some(staging_dir.to_string_lossy().into_owned()),
                    unpack_policy: unpack_policy_storage_value(unpack_policy).to_owned(),
                    bytes_received: 0,
                    bytes_total: None,
                    error_code: Some(error.failure_code().to_owned()),
                    error_message: Some(error.support_message()),
                    started_at: Some(started_at),
                    completed_at: Some(completed_at.clone()),
                    updated_at: completed_at,
                })
                .await;
        }

        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn download_work_inner<S>(
        &self,
        account: &Account,
        work_id: &WorkId,
        staging_dir: &Path,
        final_dir: &Path,
        started_at: &str,
        request: WorkDownloadRequest<'_>,
        source: &S,
    ) -> Result<WorkDownloadReport>
    where
        S: WorkDownloadSource + Sync,
    {
        request.check_cancelled()?;
        request.emit(WorkDownloadProgress::LoggingIn);

        self.authenticate_account(
            account,
            request.password,
            request.two_factor_prompt,
            request.cancellation_token,
            source,
        )
        .await?;

        request.check_cancelled()?;
        request.emit(WorkDownloadProgress::ResolvingPlan);
        let plan =
            auth::cancellable(source.download_plan(work_id), request.cancellation_token).await?;

        request.check_cancelled()?;
        let previous = self.storage.work_download_state(request.work_id).await?;
        if previous.status != WorkDownloadStatus::Downloaded {
            self.storage
                .save_work_download(&WorkDownloadUpdate {
                    work_id: request.work_id.to_owned(),
                    status: WorkDownloadStatus::Downloading,
                    local_path: Some(final_dir.to_string_lossy().into_owned()),
                    staging_path: Some(staging_dir.to_string_lossy().into_owned()),
                    unpack_policy: unpack_policy_storage_value(request.unpack_policy).to_owned(),
                    bytes_received: 0,
                    bytes_total: None,
                    error_code: None,
                    error_message: None,
                    started_at: Some(started_at.to_owned()),
                    completed_at: None,
                    updated_at: now_string(),
                })
                .await?;
        } else if previous.staging_path.as_deref() != staging_dir.to_str() {
            // Track operational staging before any transfer writes, without changing
            // the usable content's status, paths, bytes, policy, errors, or timestamps.
            if let Some(path) = previous.staging_path.as_deref().map(Path::new) {
                if path.try_exists()? && path.canonicalize()? != staging_dir {
                    return Err(LibraryError::RecoveryRequired(format!(
                        "previous staging still exists at {}; retain it before starting staging at {}",
                        path.display(), staging_dir.display()
                    )));
                }
            }
            self.save_download_staging(request.work_id, &previous, staging_dir)
                .await?;
        }

        if request.replace_existing && !request.resume_partial {
            remove_existing_download_path(staging_dir, &[request.download_root]).await?;
        }

        let download_cancellation = dldr_download::CancellationToken::new();
        let _download_cancellation_forwarder =
            request.forward_download_cancellation(&download_cancellation);
        let job = DownloadJobRequest {
            work_id: work_id.clone(),
            target_root: request.download_root.to_path_buf(),
            unpack_policy: request.unpack_policy,
        };
        let downloaded = source
            .download_files(&job, &plan, &download_cancellation, &mut |progress| {
                if request.is_cancelled() {
                    download_cancellation.cancel();
                }

                request.emit(WorkDownloadProgress::Download(progress));
            })
            .await?;

        request.check_cancelled()?;
        request.emit(WorkDownloadProgress::Finalizing);
        write_serial_information_file(
            staging_dir,
            work_id.as_ref(),
            &plan.serial_numbers,
            &now_string(),
        )
        .await?;

        let completed_at = now_string();
        let bytes_received = downloaded
            .files
            .iter()
            .map(|file| file.bytes_written)
            .sum::<u64>();

        let update = WorkDownloadUpdate {
            work_id: request.work_id.to_owned(),
            status: WorkDownloadStatus::Downloaded,
            local_path: Some(final_dir.to_string_lossy().into_owned()),
            staging_path: Some(staging_dir.to_string_lossy().into_owned()),
            unpack_policy: unpack_policy_storage_value(request.unpack_policy).to_owned(),
            bytes_received,
            bytes_total: Some(bytes_received),
            error_code: None,
            error_message: None,
            started_at: Some(started_at.to_owned()),
            completed_at: Some(completed_at.clone()),
            updated_at: completed_at,
        };
        let old_path = previous
            .local_path
            .as_deref()
            .map(Path::new)
            .filter(|p| p.is_dir())
            .map(std::fs::canonicalize)
            .transpose()?;
        let installation = finalization::Installation::new(
            request.work_id,
            staging_dir,
            final_dir,
            old_path.as_deref(),
            request.replace_existing,
        )?;
        installation.validate(&[request.library_root, request.download_root])?;
        self.storage
            .begin_download_finalization(&installation.record)
            .await?;
        let result = async {
            installation.prepare().await?;
            request.check_cancelled()?;
            // Once swapping starts, finish installation/commit or rollback before cancellation.
            installation.install().await?;
            self.storage
                .commit_download_finalization(&installation.record.operation_id, &update)
                .await?;
            Ok::<(), LibraryError>(())
        }
        .await;
        if let Err(error) = result {
            // A commit error may have an uncertain outcome. Inspect the durable marker
            // before deciding whether it is safe to undo filesystem installation.
            match self.storage.download_finalization(request.work_id).await {
                Ok(Some(record)) if record.committed => {}
                Ok(Some(_)) => {
                    if let Err(recovery) = self
                        .recover_download(
                            request.work_id,
                            &[request.library_root, request.download_root],
                        )
                        .await
                    {
                        return Err(installation.required(&format!(
                            "{}; rollback: {}",
                            error.support_message(),
                            recovery.support_message()
                        )));
                    }
                    if Path::new(&installation.record.temporary_path).exists() {
                        return Err(LibraryError::RetainedDownload {
                            source: Box::new(error),
                            retained_path: PathBuf::from(&installation.record.temporary_path),
                        });
                    }
                    return Err(error);
                }
                _ => {
                    return Err(installation.required(&format!(
                        "{}; commit outcome unavailable",
                        error.support_message()
                    )))
                }
            }
        }
        let mut warnings = Vec::new();
        if let Err(error) = self
            .recover_download(
                request.work_id,
                &[request.library_root, request.download_root],
            )
            .await
        {
            request.emit(WorkDownloadProgress::Warning(error.support_message()));
            warnings.push(error.support_message());
        }

        request.emit(WorkDownloadProgress::Completed);

        Ok(WorkDownloadReport {
            work_id: request.work_id.to_owned(),
            account_id: account.id.clone(),
            local_path: final_dir.to_path_buf(),
            file_count: downloaded.files.len(),
            archive_extracted: downloaded.archive_extraction.is_some(),
            download_state: update.into(),
            warnings,
        })
    }
}

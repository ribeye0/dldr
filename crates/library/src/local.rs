use super::*;

impl Library {
    pub async fn remove_work_download(
        &self,
        request: WorkDownloadRemovalRequest<'_>,
    ) -> Result<WorkDownloadState> {
        let lock = self.work_lock(request.work_id);
        let _guard = lock.lock().await;
        self.recover_download_inner(
            request.work_id,
            &[request.library_root, request.download_root],
            true,
        )
        .await?;
        let state = self.storage.work_download_state(request.work_id).await?;
        let allowed_roots = [request.library_root, request.download_root];

        remove_download_path_from_state(state.local_path.as_deref(), &allowed_roots).await?;
        remove_download_path_from_state(state.staging_path.as_deref(), &allowed_roots).await?;
        self.storage.delete_work_download(request.work_id).await?;
        Ok(self.storage.work_download_state(request.work_id).await?)
    }

    pub async fn mark_work_downloaded(
        &self,
        request: WorkDownloadMarkRequest<'_>,
    ) -> Result<WorkDownloadState> {
        let lock = self.work_lock(request.work_id);
        let _guard = lock.lock().await;
        self.recover_download(request.work_id, &[request.library_root])
            .await?;
        let canonical_path = canonicalize_existing_directory(request.local_path)?;
        let canonical_root = request.library_root.canonicalize()?;

        if !path_is_download_child_of_any_root(&canonical_path, &[canonical_root]) {
            return Err(LibraryError::DownloadPathOutsideRoots(canonical_path));
        }

        let completed_at = now_string();
        self.storage
            .save_work_download(&WorkDownloadUpdate {
                work_id: request.work_id.to_owned(),
                status: WorkDownloadStatus::Downloaded,
                local_path: Some(canonical_path.to_string_lossy().into_owned()),
                staging_path: None,
                unpack_policy: "manual".to_owned(),
                bytes_received: 0,
                bytes_total: None,
                error_code: None,
                error_message: None,
                started_at: Some(completed_at.clone()),
                completed_at: Some(completed_at.clone()),
                updated_at: completed_at,
            })
            .await?;

        Ok(self.storage.work_download_state(request.work_id).await?)
    }

    pub async fn import_local_work_downloads(
        &self,
        request: LocalWorkImportRequest<'_>,
    ) -> Result<LocalWorkImportReport> {
        self.import_local_work_downloads_internal(request, None)
            .await
    }

    pub async fn import_local_work_downloads_with_metadata_source<S>(
        &self,
        request: LocalWorkImportRequest<'_>,
        metadata_source: &S,
    ) -> Result<LocalWorkImportReport>
    where
        S: LocalWorkMetadataSource,
    {
        self.import_local_work_downloads_internal(
            request,
            Some(metadata_source as &dyn LocalWorkMetadataSource),
        )
        .await
    }

    pub(super) async fn import_local_work_downloads_internal(
        &self,
        request: LocalWorkImportRequest<'_>,
        metadata_source: Option<&dyn LocalWorkMetadataSource>,
    ) -> Result<LocalWorkImportReport> {
        let library_root = canonicalize_existing_directory(request.library_root)?;
        let mut entries = tokio::fs::read_dir(&library_root).await?;
        let scanned_at = now_string();
        let mut imports = Vec::new();
        let mut imported_works = Vec::new();
        let mut metadata_candidate_ids = BTreeSet::<WorkId>::new();
        let mut scanned_directories = 0;
        let mut skipped_no_id = 0;
        let mut skipped_ambiguous = 0;
        let mut skipped_non_utf8 = 0;
        let mut skipped_existing = 0;
        let mut recovery_errors = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;

            if !file_type.is_dir() {
                continue;
            }

            scanned_directories += 1;

            let folder_name = match entry.file_name().into_string() {
                Ok(folder_name) => folder_name,
                Err(_) => {
                    skipped_non_utf8 += 1;
                    continue;
                }
            };
            let detected = detect_work_ids_in_text(&folder_name);
            if folder_name.starts_with(finalization::PREFIX) {
                continue;
            }

            if detected.is_empty() {
                skipped_no_id += 1;
                continue;
            }

            if detected.len() > 1 {
                skipped_ambiguous += 1;
                continue;
            }

            let work_id = detected[0].clone();
            let work_id_string = work_id.as_ref().to_owned();
            let lock = self.work_lock(&work_id_string);
            let _guard = lock.lock().await;
            if let Err(error) = self
                .recover_download(&work_id_string, &[&library_root])
                .await
            {
                recovery_errors.push(error.support_message());
                continue;
            }
            if self
                .storage
                .work_download_state(&work_id_string)
                .await?
                .status
                != WorkDownloadStatus::NotDownloaded
            {
                skipped_existing += 1;
                if self
                    .storage
                    .work_needs_local_metadata_hydration(&work_id_string)
                    .await?
                {
                    metadata_candidate_ids.insert(work_id);
                }
                continue;
            }

            let local_path = entry.path().canonicalize()?;
            let work = cached_work_from_local_folder(&work_id_string, &folder_name, &scanned_at)?;
            let download = WorkDownloadUpdate {
                work_id: work_id_string.clone(),
                status: WorkDownloadStatus::Downloaded,
                local_path: Some(local_path.to_string_lossy().into_owned()),
                staging_path: None,
                unpack_policy: "manual".to_owned(),
                bytes_received: 0,
                bytes_total: None,
                error_code: None,
                error_message: None,
                started_at: Some(scanned_at.clone()),
                completed_at: Some(scanned_at.clone()),
                updated_at: scanned_at.clone(),
            };

            metadata_candidate_ids.insert(work_id);
            imports.push(LocalWorkDownloadImport { work, download });
            imported_works.push(LocalWorkImportItem {
                work_id: work_id_string,
                local_path,
            });
        }

        let metadata_candidate_count = if metadata_source.is_some() {
            metadata_candidate_ids.len()
        } else {
            0
        };
        let mut metadata_by_id = BTreeMap::<String, CachedWork>::new();
        let mut metadata_error = None;

        if let Some(source) = metadata_source.filter(|_| !metadata_candidate_ids.is_empty()) {
            let ids = metadata_candidate_ids.iter().cloned().collect::<Vec<_>>();

            match source.works(&ids).await {
                Ok(works) => {
                    for work in works {
                        if !metadata_candidate_ids.contains(&work.id) {
                            continue;
                        }

                        let cached = cached_work_from_public_api(work, &scanned_at)?;
                        metadata_by_id.insert(cached.work_id.clone(), cached);
                    }
                }
                Err(error) => {
                    metadata_error = Some(error.support_message());
                }
            }
        }

        let metadata_works = metadata_by_id.values().cloned().collect::<Vec<_>>();

        imported_works.clear();
        for import in imports {
            let work_id = &import.work.work_id;
            let lock = self.work_lock(work_id);
            let _guard = lock.lock().await;
            if let Err(error) = self.recover_download(work_id, &[&library_root]).await {
                recovery_errors.push(error.support_message());
                continue;
            }
            if self.storage.work_download_state(work_id).await?.status
                != WorkDownloadStatus::NotDownloaded
            {
                skipped_existing += 1;
                continue;
            }
            let local_path =
                PathBuf::from(import.download.local_path.as_ref().expect("import path"));
            if !local_path.is_dir() {
                continue;
            }
            self.storage
                .import_local_work_downloads_with_metadata(std::slice::from_ref(&import), &[])
                .await?;
            imported_works.push(LocalWorkImportItem {
                work_id: work_id.clone(),
                local_path,
            });
        }
        self.storage
            .import_local_work_downloads_with_metadata(&[], &metadata_works)
            .await?;

        Ok(LocalWorkImportReport {
            scanned_directories,
            imported_count: imported_works.len(),
            skipped_no_id,
            skipped_ambiguous,
            skipped_non_utf8,
            skipped_existing,
            metadata_candidate_count,
            metadata_updated_count: metadata_by_id.len(),
            metadata_missing_count: metadata_candidate_count.saturating_sub(metadata_by_id.len()),
            metadata_error,
            recovery_errors,
            imported_works,
        })
    }
}

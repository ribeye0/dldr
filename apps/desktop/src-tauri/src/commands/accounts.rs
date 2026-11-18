use crate::two_factor::JobTwoFactorPrompt;
use crate::{dto::*, errors::*, progress::*, state::*, validation::*};
use dldr_audit::AuditEvent;
use dldr_jobs::{JobFailure, JobMetadata, JobProgress};
use dldr_library::{
    AccountSyncRequest, DlsitePublicMetadataSource, DlsiteSyncSource, LocalWorkImportRequest,
};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) async fn list_accounts(state: State<'_, AppState>) -> Result<Vec<AccountDto>, String> {
    let diagnostic_operation = state.audit.operation("list_accounts");
    diagnostic_operation
        .command(async {
            let accounts = state.library.accounts().await.map_err(command_error)?;
            let mut dtos = Vec::with_capacity(accounts.len());

            for account in accounts {
                let has_credential = state
                    .library
                    .account_has_saved_password(&account)
                    .map_err(command_error)?;

                dtos.push(AccountDto::from_account(account, has_credential));
            }

            Ok(dtos)
        })
        .await
}

#[tauri::command]
pub(crate) async fn save_account(
    state: State<'_, AppState>,
    request: SaveAccountCommandRequest,
) -> Result<AccountDto, String> {
    let _admission = state.mutation_lock.read().await;
    let diagnostic_operation = state.audit.operation("save_account");
    diagnostic_operation
        .command(async {
            let request = match request.into_library_request() {
                Ok(request) => request,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("account.save", "Failed to validate account")
                            .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let details = json!({
                "accountId": request.id.clone(),
                "hasLoginName": request.login_name.is_some(),
                "hasPassword": request.password.is_some(),
            });
            let result = state.library.save_account(request).await;

            match result {
                Ok(account) => {
                    let has_credential = state
                        .library
                        .account_has_saved_password(&account)
                        .map_err(command_error)?;

                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded("account.save", "Saved account").with_details(
                            json!({
                                "accountId": account.id.clone(),
                                "label": account.label.clone(),
                                "hasCredential": has_credential,
                                "enabled": account.enabled,
                            }),
                        ),
                    )
                    .await;
                    Ok(AccountDto::from_account(account, has_credential))
                }
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("account.save", "Failed to save account")
                            .with_error(Some("library"), message.clone())
                            .with_details(details),
                    )
                    .await;
                    Err(message)
                }
            }
        })
        .await
}

#[tauri::command]
pub(crate) async fn set_account_enabled(
    state: State<'_, AppState>,
    request: SetAccountEnabledRequest,
) -> Result<(), String> {
    let _admission = state.mutation_lock.read().await;
    let diagnostic_operation = state.audit.operation("set_account_enabled");
    diagnostic_operation
        .command(async {
            let account_id = match normalize_required_id(request.account_id) {
                Ok(account_id) => account_id,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "account.setEnabled",
                            "Failed to validate account toggle",
                        )
                        .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let result = state
                .library
                .set_account_enabled(&account_id, request.enabled)
                .await;

            match result {
                Ok(()) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded(
                            "account.setEnabled",
                            "Updated account enabled state",
                        )
                        .with_details(json!({
                            "accountId": account_id,
                            "enabled": request.enabled,
                        })),
                    )
                    .await;
                    Ok(())
                }
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "account.setEnabled",
                            "Failed to update account enabled state",
                        )
                        .with_error(Some("library"), message.clone())
                        .with_details(json!({
                            "accountId": account_id,
                            "enabled": request.enabled,
                        })),
                    )
                    .await;
                    Err(message)
                }
            }
        })
        .await
}

#[tauri::command]
pub(crate) async fn remove_account(
    state: State<'_, AppState>,
    request: RemoveAccountRequest,
) -> Result<AccountRemovalReportDto, String> {
    let _admission = state.mutation_lock.read().await;
    let diagnostic_operation = state.audit.operation("remove_account");
    diagnostic_operation
        .command(async {
            let account_id = match normalize_required_id(request.account_id) {
                Ok(account_id) => account_id,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("account.remove", "Failed to validate account removal")
                            .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };

            match state.library.remove_account(&account_id).await {
                Ok(report) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded("account.remove", "Removed account").with_details(
                            json!({
                                "accountId": report.account_id.clone(),
                                "label": report.label.clone(),
                                "credentialDeleted": report.credential_deleted,
                            }),
                        ),
                    )
                    .await;
                    Ok(AccountRemovalReportDto::from(report))
                }
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("account.remove", "Failed to remove account")
                            .with_error(Some("library"), message.clone())
                            .with_details(json!({ "accountId": account_id })),
                    )
                    .await;
                    Err(message)
                }
            }
        })
        .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubmitTwoFactorCodeRequest {
    pub(crate) request_id: String,
    pub(crate) code: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CancelTwoFactorRequest {
    pub(crate) request_id: String,
}

/// Hands a verification code to the job waiting on `requestId`.
///
/// The code is never logged or audited; only the fact that a code was supplied is.
#[tauri::command]
pub(crate) async fn submit_two_factor_code(
    state: State<'_, AppState>,
    request: SubmitTwoFactorCodeRequest,
) -> Result<(), String> {
    let diagnostic_operation = state.audit.operation("submit_two_factor_code");
    diagnostic_operation
        .command(async {
            let code = request.code.trim().to_owned();

            if code.is_empty() {
                return Err("Verification code must not be empty".to_owned());
            }

            if !state
                .two_factor_prompts
                .answer(&request.request_id, Some(code))
            {
                return Err("This verification request is no longer waiting for a code".to_owned());
            }

            record_audit(
                &state.audit,
                AuditEvent::succeeded("account.login.twoFactor", "Submitted two-factor code")
                    .with_details(json!({ "requestId": request.request_id })),
            )
            .await;

            Ok(())
        })
        .await
}

#[tauri::command]
pub(crate) async fn cancel_two_factor(
    state: State<'_, AppState>,
    request: CancelTwoFactorRequest,
) -> Result<(), String> {
    let diagnostic_operation = state.audit.operation("cancel_two_factor");
    diagnostic_operation
        .command(async {
            state.two_factor_prompts.answer(&request.request_id, None);

            record_audit(
                &state.audit,
                AuditEvent::succeeded(
                    "account.login.twoFactor",
                    "Cancelled two-factor verification",
                )
                .with_details(json!({ "requestId": request.request_id })),
            )
            .await;

            Ok(())
        })
        .await
}

#[tauri::command]
pub(crate) async fn start_account_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartAccountSyncRequest,
) -> Result<StartJobResponse, String> {
    let _admission = state.mutation_lock.read().await;
    let diagnostic_operation = state.audit.operation("start_account_sync");
    diagnostic_operation
        .command(async {
            let account_id = match normalize_required_id(request.account_id) {
                Ok(account_id) => account_id,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("account.sync.queue", "Failed to validate account sync")
                            .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let password = match normalize_secret(request.password) {
                Ok(password) => password,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "account.sync.queue",
                            "Failed to validate account sync secret",
                        )
                        .with_error(Some("validation"), error.clone())
                        .with_details(json!({ "accountId": account_id })),
                    )
                    .await;
                    return Err(error);
                }
            };
            let library = state.library.clone();
            let (local_scan_library_root, local_scan_skip_reason) =
                match state.storage.app_settings().await {
                    Ok(settings) => match required_library_root(&settings) {
                        Ok(root) => (Some(root), None),
                        Err(error) => (None, Some(error)),
                    },
                    Err(error) => (None, Some(command_error(error))),
                };
            let mut metadata = JobMetadata::new();

            metadata.insert("accountId".to_owned(), json!(account_id.clone()));
            metadata.insert(
                "localScanPlanned".to_owned(),
                json!(local_scan_library_root.is_some()),
            );
            if let Some(reason) = &local_scan_skip_reason {
                metadata.insert("localScanSkipReason".to_owned(), json!(reason));
            }

            let job_account_id = account_id.clone();
            let two_factor_prompts = state.two_factor_prompts.clone();
            let job_id = state.jobs.spawn(
                "accountSync",
                format!("Sync {job_account_id}"),
                metadata,
                move |context| async move {
                    context.info("Preparing account sync");
                    let client =
                        dldr_api::DlsiteClient::new(dldr_api::DlsiteClientConfig::default())
                            .map_err(|error| {
                                JobFailure::with_code("api_client", error.to_string())
                            })?;
                    let source = DlsiteSyncSource::new(client.clone());
                    let metadata_source = DlsitePublicMetadataSource::new(client);
                    let progress_sink = JobSyncProgressSink {
                        context: context.clone(),
                    };
                    let two_factor_prompt =
                        JobTwoFactorPrompt::new(app, two_factor_prompts, context.clone());
                    let report = library
                        .sync_account_with_source(
                            AccountSyncRequest {
                                account_id: &job_account_id,
                                password: password.as_deref(),
                                cancellation_token: Some(context.cancellation_token()),
                                progress_sink: Some(&progress_sink),
                                two_factor_prompt: Some(&two_factor_prompt),
                            },
                            &source,
                        )
                        .await
                        .map_err(account_sync_failure)?;
                    let local_scan_output = match &local_scan_library_root {
                        Some(library_root) => {
                            context.set_phase("scanningLocalDownloads");
                            context.clear_progress();
                            context.info("Scanning local downloads");

                            let result = library
                                .import_local_work_downloads_with_metadata_source(
                                    LocalWorkImportRequest::new(library_root),
                                    &metadata_source,
                                )
                                .await;
                            post_sync_scan_output(&context, result)
                        }
                        None => {
                            let reason = local_scan_skip_reason
                                .clone()
                                .unwrap_or_else(|| "Library folder is required".to_owned());

                            context.info(format!("Skipping local scan: {reason}"));
                            json!({
                                "status": "skipped",
                                "reason": reason,
                            })
                        }
                    };
                    let mut output = JobMetadata::new();

                    output.insert("accountId".to_owned(), json!(report.account_id));
                    output.insert("syncRunId".to_owned(), json!(report.sync_run_id));
                    output.insert("purchasedCount".to_owned(), json!(report.purchased_count));
                    output.insert(
                        "cachedWorkCount".to_owned(),
                        json!(report.cached_work_count),
                    );
                    output.insert(
                        "missingDetailCount".to_owned(),
                        json!(report.missing_detail_count),
                    );
                    output.insert("pageLimit".to_owned(), json!(report.page_limit));
                    output.insert("concurrency".to_owned(), json!(report.concurrency));
                    output.insert("localScan".to_owned(), local_scan_output);
                    if report.missing_detail_count > 0 {
                        context.warn(format!(
                            "{} purchased works were missing details from content/works",
                            report.missing_detail_count
                        ));
                    }
                    context.set_phase("completed");
                    context.set_progress(JobProgress::items(
                        Some(report.cached_work_count as u64),
                        Some(report.cached_work_count as u64),
                    ));
                    context.info(format!("Synced {} works", report.cached_work_count));

                    Ok(output)
                },
            );

            record_audit(
                &state.audit,
                AuditEvent::queued("account.sync", "Queued account sync").with_details(json!({
                    "accountId": account_id,
                    "jobId": job_id.to_string(),
                })),
            )
            .await;

            Ok(StartJobResponse {
                job_id: job_id.to_string(),
            })
        })
        .await
}

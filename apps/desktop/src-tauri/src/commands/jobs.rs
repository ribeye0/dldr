use crate::{dto::*, errors::*, state::*, validation::*};
use dldr_audit::AuditEvent;
use dldr_jobs::{JobId, JobLogPage};
use tauri::State;

#[tauri::command]
pub(crate) async fn list_jobs(
    state: State<'_, AppState>,
) -> Result<Vec<dldr_jobs::JobSnapshot>, String> {
    Ok(state.jobs.list_jobs())
}

#[tauri::command]
pub(crate) async fn get_job(
    state: State<'_, AppState>,
    request: JobIdRequest,
) -> Result<dldr_jobs::JobSnapshot, String> {
    let diagnostic_operation = state.audit.operation("get_job");
    diagnostic_operation
        .command(async {
            let job_id = normalize_required_id(request.job_id)?;

            state
                .jobs
                .get_job(&JobId::from(job_id))
                .ok_or_else(|| "job not found".to_owned())
        })
        .await
}

#[tauri::command]
pub(crate) async fn cancel_job(
    state: State<'_, AppState>,
    request: JobIdRequest,
) -> Result<dldr_jobs::CancelJobResult, String> {
    let diagnostic_operation = state.audit.operation("cancel_job");
    diagnostic_operation
        .command(async {
            let job_id = normalize_required_id(request.job_id)?;

            state
                .jobs
                .cancel_job(&JobId::from(job_id))
                .map_err(command_error)
        })
        .await
}

#[tauri::command]
pub(crate) async fn get_job_logs(
    state: State<'_, AppState>,
    request: JobLogsRequest,
) -> Result<JobLogPage, String> {
    let diagnostic_operation = state.audit.operation("get_job_logs");
    diagnostic_operation
        .command(async {
            let job_id = normalize_required_id(request.job_id)?;

            state
                .jobs
                .job_logs(&JobId::from(job_id), request.after_sequence, request.limit)
                .map_err(command_error)
        })
        .await
}

#[tauri::command]
pub(crate) async fn clear_finished_jobs(
    state: State<'_, AppState>,
) -> Result<ClearFinishedJobsResponse, String> {
    let diagnostic_operation = state.audit.operation("clear_finished_jobs");
    diagnostic_operation
        .command(async {
            Ok(ClearFinishedJobsResponse {
                removed_count: state.jobs.clear_finished(),
            })
        })
        .await
}

#[tauri::command]
pub(crate) async fn list_audit_events(
    state: State<'_, AppState>,
    request: ListAuditEventsRequest,
) -> Result<Vec<AuditEvent>, String> {
    if request
        .run_id
        .as_deref()
        .is_none_or(|id| id == state.audit.run_id())
    {
        return state
            .audit
            .recent_events(request.limit.unwrap_or(500))
            .await
            .map_err(command_error);
    }
    let logger = state.audit.clone();
    tauri::async_runtime::spawn_blocking(move || {
        logger.events_for_run(
            request.run_id.as_deref(),
            request.limit.unwrap_or(500),
            None,
        )
    })
    .await
    .map_err(command_error)?
    .map_err(command_error)
}

#[tauri::command]
pub(crate) async fn get_audit_log_dir(
    state: State<'_, AppState>,
) -> Result<AuditLogDirDto, String> {
    Ok(AuditLogDirDto {
        path: state.audit.log_dir().to_string_lossy().into_owned(),
    })
}

use crate::{errors::*, progress::*, state::*, validation::*};
use dldr_audit::AuditEvent;
use serde_json::json;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) async fn open_download_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let operation = state.audit.operation("open_download_folder");
    operation
        .command(async {
            let settings = state.storage.app_settings().await.map_err(command_error)?;
            let path = required_library_root(&settings).map_err(|_| {
                "Set a Library folder in Settings before opening downloaded works".to_owned()
            })?;
            tokio::fs::create_dir_all(&path).await.map_err(|error| {
                format!(
                    "Could not create or access the library folder: {}",
                    command_error(error)
                )
            })?;
            crate::opener::open_path(&app, &state.audit, &path).await
        })
        .await
}

#[tauri::command]
pub(crate) async fn open_audit_log_dir(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let diagnostic_operation = state.audit.operation("open_audit_log_dir");
    diagnostic_operation
        .command(async {
            let path = state.audit.log_dir().to_path_buf();

            match crate::opener::open_path(&app, &state.audit, &path).await {
                Ok(()) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded("audit.openLogDir", "Opened audit log directory")
                            .with_details(json!({
                                "path": path.to_string_lossy().to_string(),
                            })),
                    )
                    .await;
                    Ok(())
                }
                Err(message) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "audit.openLogDir",
                            "Failed to open audit log directory",
                        )
                        .with_error(Some("opener"), message.clone())
                        .with_details(json!({
                            "path": path.to_string_lossy().to_string(),
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
pub(crate) async fn open_external_url(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    state
        .audit
        .operation("external.open")
        .command(crate::opener::open_url(&app, &state.audit, &url))
        .await
}

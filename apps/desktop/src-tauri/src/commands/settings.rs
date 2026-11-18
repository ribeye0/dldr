use crate::{dto::*, errors::*, progress::*, state::*};
use dldr_audit::AuditEvent;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;

#[tauri::command]
pub(crate) async fn get_settings(state: State<'_, AppState>) -> Result<AppSettingsDto, String> {
    let diagnostic_operation = state.audit.operation("get_settings");
    diagnostic_operation
        .command(async {
            state
                .storage
                .app_settings()
                .await
                .map(AppSettingsDto::from)
                .map_err(command_error)
        })
        .await
}

#[tauri::command]
pub(crate) async fn list_name_languages(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .audit
        .operation("list_name_languages")
        .command(async {
            state
                .library
                .available_name_languages()
                .await
                .map_err(command_error)
        })
        .await
}

#[tauri::command]
pub(crate) async fn save_settings(
    state: State<'_, AppState>,
    settings: SaveSettingsRequest,
) -> Result<AppSettingsDto, String> {
    let _admission = state.mutation_lock.read().await;
    let diagnostic_operation = state.audit.operation("save_settings");
    diagnostic_operation
        .command(async {
            let settings = match settings.into_app_settings_patch() {
                Ok(settings) => settings,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("settings.save", "Failed to validate settings")
                            .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let result = state.storage.save_app_settings_patch(&settings).await;

            match result {
                Ok(settings) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded("settings.save", "Saved settings").with_details(
                            json!({
                                "libraryRootSet": settings.library_root.is_some(),
                                "downloadRootSet": settings.download_root.is_some(),
                            }),
                        ),
                    )
                    .await;
                    Ok(AppSettingsDto::from(settings))
                }
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("settings.save", "Failed to save settings")
                            .with_error(Some("storage"), message.clone()),
                    )
                    .await;
                    Err(message)
                }
            }
        })
        .await
}

#[tauri::command]
pub(crate) async fn get_download_rate_limit(
    state: State<'_, AppState>,
) -> Result<DownloadRateLimitDto, String> {
    Ok(DownloadRateLimitDto {
        bytes_per_second: state.download_rate_limiter.limit(),
    })
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadRateLimitDto {
    pub(crate) bytes_per_second: Option<u64>,
}

impl DownloadRateLimitDto {
    pub(crate) fn validated_limit(&self) -> Result<Option<u64>, String> {
        let rate = self.bytes_per_second.filter(|rate| *rate > 0);
        dldr_download::validate_download_rate(rate).map_err(str::to_owned)?;
        Ok(rate)
    }
}

#[tauri::command]
pub(crate) async fn set_download_rate_limit(
    state: State<'_, AppState>,
    request: DownloadRateLimitDto,
) -> Result<DownloadRateLimitDto, String> {
    let _admission = state.mutation_lock.read().await;
    let operation = state.audit.operation("set_download_rate_limit");
    operation
        .command(async {
            let rate = request.validated_limit()?;
            let _guard = state.download_rate_settings_lock.lock().await;
            state
                .storage
                .save_download_rate_limit(rate)
                .await
                .map_err(command_error)?;
            state
                .download_rate_limiter
                .set_limit(rate)
                .map_err(str::to_owned)?;
            Ok(DownloadRateLimitDto {
                bytes_per_second: rate,
            })
        })
        .await
}

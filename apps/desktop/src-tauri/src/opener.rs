//! Dispatch external desktop requests with platform-appropriate launch isolation.
use dldr_audit::{AuditLogger, AuditOutcome};
use serde_json::json;
use std::path::Path;
use tauri::AppHandle;
#[cfg(not(target_os = "linux"))]
use tauri_plugin_opener::OpenerExt;
#[cfg(target_os = "linux")]
mod linux;

pub async fn open_path(_app: &AppHandle, audit: &AuditLogger, path: &Path) -> Result<(), String> {
    let mut operation = audit.operation("desktop.launch");
    let result = operation
        .scope(async {
            audit.desktop_path(path);
            record_environment(audit);
            #[cfg(target_os = "linux")]
            {
                linux::open(path.canonicalize()?.as_os_str())
                    .await
                    .map_err(tauri_plugin_opener::Error::from)
            }
            #[cfg(not(target_os = "linux"))]
            {
                _app.opener()
                    .open_path(path.to_string_lossy().into_owned(), None::<String>)
            }
        })
        .await;
    record_result(&mut operation, &result);
    result.map_err(|error| format!("System open request failed: {error}"))
}

pub async fn open_url(_app: &AppHandle, audit: &AuditLogger, url: &str) -> Result<(), String> {
    if !tauri::Url::parse(url)
        .is_ok_and(|v| matches!(v.scheme(), "http" | "https") && v.host_str().is_some())
        || url.contains(['\r', '\n', '\0'])
    {
        return Err("Only HTTP and HTTPS links are supported".into());
    }
    let mut operation = audit.operation("desktop.launch");
    let result = operation
        .scope(async {
            record_environment(audit);
            #[cfg(target_os = "linux")]
            {
                linux::open(std::ffi::OsStr::new(url))
                    .await
                    .map_err(tauri_plugin_opener::Error::from)
            }
            #[cfg(not(target_os = "linux"))]
            {
                _app.opener().open_url(url, None::<String>)
            }
        })
        .await;
    record_result(&mut operation, &result);
    result.map_err(|error| format!("System browser request failed: {error}"))
}

fn record_environment(audit: &AuditLogger) {
    audit.opener_environment(
        &std::env::current_dir().unwrap_or_default(),
        audit.environment(&crate::diagnostics::environment()),
    );
}

fn record_result(
    operation: &mut dldr_audit::Operation,
    result: &Result<(), tauri_plugin_opener::Error>,
) {
    let os_code = match result {
        Err(tauri_plugin_opener::Error::Io(error)) => error.raw_os_error(),
        _ => None,
    };
    let outcome = if result.is_ok() {
        AuditOutcome::Succeeded
    } else {
        AuditOutcome::Failed
    };
    operation.finish(
        outcome,
        json!({
            "failureKind": if result.is_err() { Some("opener") } else { None },
            "osCode": os_code,
        }),
    );
}

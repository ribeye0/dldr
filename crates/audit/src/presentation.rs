//! Human-readable diagnostics built from typed, sanitized fields, never raw exceptions.
use crate::{AuditEvent, AuditOutcome};
use serde_json::Value;

pub fn operation_label(operation: &str) -> &'static str {
    match operation {
        "app.startup" => "Start application",
        "save_settings" | "settings.save" => "Save settings",
        "get_settings" => "Read settings",
        "list_products" => "Load library",
        "get_product_detail" => "Load work details",
        "get_product_page_content" => "Load product page",
        "list_product_filter_facets" => "Load search filters",
        "save_product_cover" => "Save cover",
        "save_account" | "account.save" => "Save account",
        "remove_account" | "account.remove" => "Remove account",
        "set_account_enabled" | "account.setEnabled" => "Change account status",
        "list_accounts" => "Read account",
        "start_account_sync" | "account.sync.queue" => "Queue library sync",
        "account.sync" => "Sync purchased works",
        "account.login.twoFactor" | "submit_two_factor_code" => "Verify login",
        "cancel_two_factor" => "Cancel login verification",
        "start_work_download" | "work.download.queue" => "Queue download",
        "start_bulk_work_download" => "Queue batch download",
        "work.download" => "Download work",
        "work.bulkDownload" => "Download batch",
        "preview_bulk_work_download" | "work.bulkDownload.preview" | "work.bulkDownloadPreview" => {
            "Preview downloads"
        }
        "delete_work_download" | "work.download.delete" => "Delete local download",
        "mark_work_downloaded" | "work.download.mark" => "Mark work as downloaded",
        "scan_local_work_downloads" | "work.local.scan" => "Scan local works",
        "open_work_download" | "work.open" => "Open work folder",
        "open_download_folder" => "Open library folder",
        "open_audit_log_dir" | "audit.openLogDir" => "Open log folder",
        "open_external_url" | "external.open" => "Open external link",
        "desktop.launch" => "Launch desktop handler",
        "open_player" => "Open player",
        "control_player" => "Control playback",
        "player_subtitle" => "Read subtitles",
        "player.playback" => "Play audio",
        "open_dlsite_play" => "Open DLsite Play",
        "list_playlists" => "Load playlists",
        "edit_playlist" => "Edit playlist",
        "cancel_job" | "control_download_queue" => "Control download queue",
        "retry_failed_downloads" => "Retry downloads",
        "clear_finished_jobs" | "clear_download_queue" => "Clear completed tasks",
        "job" => "Background task",
        "job.phase" => "Task stage changed",
        "job.events" => "Deliver task updates",
        "verify_library" => "Verify local works",
        "export_library_backup" => "Export library backup",
        "restore_library_backup" => "Restore library backup",
        "configure_download_folders" => "Configure download folders",
        "set_transfer_concurrency" => "Set parallel downloads",
        "set_download_rate_limit" => "Set download speed limit",
        "diagnostics.export" => "Export diagnostics",
        "command" => "Operation error",
        "frontend.error" | "frontend.rejection" => "Interface error",
        "native.dialog" => "Open system dialog",
        "native.listener" => "Connect event listener",
        "native.appInfo" => "Read application version",
        "native.path" => "Resolve system directory",
        "native.updater" => "Check for updates",
        "native.relaunch" => "Restart application",
        "runtime" => "Runtime diagnostic",
        _ => "Application operation",
    }
}

pub(crate) fn describe(event: &mut AuditEvent) {
    let action = operation_label(&event.operation);
    let status = if event.kind == "started" {
        "started"
    } else {
        match event.outcome {
            AuditOutcome::Queued => "queued",
            AuditOutcome::Succeeded => "completed",
            AuditOutcome::Failed => "failed",
            AuditOutcome::Cancelled => "cancelled",
            AuditOutcome::Unknown => "result not confirmed",
        }
    };
    event.message = format!("{action}: {status}");
    // Preserve established desktop launch semantics: a successful request does not
    // guarantee that the external application displayed its window.
    if matches!(
        event.operation.as_str(),
        "open_work_download"
            | "open_audit_log_dir"
            | "open_external_url"
            | "work.open"
            | "audit.openLogDir"
            | "external.open"
            | "desktop.launch"
    ) && event.outcome == AuditOutcome::Succeeded
    {
        event.message = "Open request sent".into();
    } else if event.operation == "desktop.launch" && event.kind == "finished" {
        event.message = if event.outcome == AuditOutcome::Failed {
            "Open request failed"
        } else {
            "Open result not confirmed"
        }
        .into();
    }
    let reason = field(&event.details, "errorCode").or(event.error_code.as_deref());
    if event.outcome == AuditOutcome::Failed || event.stage == "cause" {
        let kind = field(&event.details, "playerErrorKind")
            .or_else(|| field(&event.details, "databaseKind"))
            .or_else(|| field(&event.details, "archiveErrorKind"))
            .or_else(|| field(&event.details, "apiErrorKind"))
            .or_else(|| field(&event.details, "ioKind"))
            .or(reason)
            .or_else(|| field(&event.details, "failureKind"));
        let (cause, hint) = failure_description(kind, &event.details);
        event.error_message = Some(cause.into());
        event.hint = Some(hint.into());
    } else {
        event.error_message = None;
        event.hint = None;
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str).or_else(|| {
        ["errorDetails", "details", "output"]
            .iter()
            .find_map(|k| value.get(k).and_then(|v| field(v, key)))
    })
}
fn number(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .or_else(|| value.get("errorDetails").and_then(|v| number(v, key)))
}
fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
        || value.get("errorDetails").is_some_and(|v| flag(v, key))
}
fn failure_description(kind: Option<&str>, details: &Value) -> (&'static str, &'static str) {
    let exported = "Export this operation's diagnostics with the steps needed to reproduce it.";
    if number(details, "osCode") == Some(28) || kind == Some("disk_full") {
        return (
            "There is not enough free disk space.",
            "Free space in both the library and download directories, then retry.",
        );
    }
    if number(details, "osCode") == Some(13)
        || kind == Some("PermissionDenied")
        || kind == Some("permission_denied")
    {
        return (
            "Access to a file or directory was denied.",
            "Check that the download directories are writable by your user.",
        );
    }
    if flag(details, "isTimeout") || kind == Some("timeout") {
        return (
            "The network request timed out.",
            "Check your connection and retry. Reduce parallel downloads if this repeats.",
        );
    }
    if flag(details, "isConnect") {
        return (
            "Could not connect to DLsite.",
            "Check your network, proxy and DNS settings, then retry.",
        );
    }
    match number(details,"httpStatus") {
        Some(401|403) => return ("DLsite refused this request.", "Verify the saved account and sign in again. Confirm that the work is owned by this account."),
        Some(429) => return ("DLsite is limiting requests.", "Wait before retrying and reduce parallel downloads."),
        Some(500..=599) => return ("DLsite returned a server error.", "Retry later. If the problem persists, export this operation's diagnostics."),
        Some(400..=499) => return ("DLsite returned an unexpected HTTP status.", "Check the account and work availability, then retry."),
        _ => (),
    }
    match kind.unwrap_or("") {
        "CheckViolation" => (
            "SQLite rejected a value because it violates a CHECK constraint.",
            exported,
        ),
        "UniqueViolation" => ("SQLite rejected a duplicate record.", exported),
        "ForeignKeyViolation" => ("SQLite found a missing related record.", exported),
        "NotNullViolation" => ("SQLite rejected a missing required value.", exported),
        "PoolTimedOut" | "PoolClosed" => (
            "The database connection is unavailable.",
            "Restart DLDR. If this repeats, export diagnostics; keep your profile database.",
        ),
        "Migration" => (
            "The database schema could not be updated.",
            "Keep your profile database and export diagnostics before making any changes.",
        ),
        "storage" | "database" => (
            "The local database operation failed.",
            "Check the SQLite code below and export diagnostics. Keep your profile database.",
        ),
        "engine_unavailable" => (
            "The libmpv playback engine could not be loaded.",
            "Install libmpv or reinstall the DLDR package, then restart DLDR.",
        ),
        "engine_initialization" => (
            "The playback engine could not initialize.",
            "Restart DLDR and check that libmpv is available.",
        ),
        "invalid_subtitle" => (
            "The subtitle file is invalid or exceeds the size limit.",
            "Use a valid UTF-8 VTT file smaller than 8 MiB.",
        ),
        "invalid_path" | "path_outside_roots" | "download_path_outside_roots" => (
            "The file is outside the allowed work directories.",
            "Check the configured directories and rescan the local work.",
        ),
        "archive" | "zip_invalid" | "rar_corrupt" | "zip_io" => (
            "The archive could not be read or extracted.",
            "Check disk space and archive integrity. Retry the download if the file is incomplete.",
        ),
        "playback" => (
            "The audio file could not be played.",
            "Verify that the file exists and can be played in another player.",
        ),
        "NotFound" | "not_found" | "not_downloaded" => (
            "The requested local file or record was not found.",
            "Check the work folder and rescan local works.",
        ),
        "credentials" | "missing_password" | "invalid_credentials" | "not_authorized" => (
            "The saved account could not be used to sign in.",
            "Check the account name and password in Settings, then synchronize again.",
        ),
        "download_target_exists" | "target_exists" => (
            "The download destination already exists.",
            "Open the existing work folder and verify it before downloading again.",
        ),
        "download_recovery_required" => (
            "An interrupted download needs recovery.",
            "Resume the work from the download queue. Keep the temporary files.",
        ),
        "panic" => ("A background task terminated unexpectedly.", exported),
        "validation" | "invalid_control" => (
            "The request contains invalid or outdated values.",
            "Check the selected values and try the operation again.",
        ),
        "transport" => (
            "The interface could not complete a native request.",
            "Restart DLDR. Export diagnostics if the problem repeats.",
        ),
        _ => ("The operation could not be completed.", exported),
    }
}

/// Stable text representation suitable for terminals, copy/paste and support bundles.
pub fn format_event(event: &AuditEvent) -> String {
    let mut line = format!("{} [{:?}] {}", event.at, event.level, event.message);
    if let Some(ms) = event.duration_ms {
        line.push_str(&format!(" ({ms} ms)"));
    }
    line.push_str(&format!(
        "\n  operation={} stage={}",
        event.operation, event.stage
    ));
    if let Some(id) = &event.operation_id {
        line.push_str(&format!(" operationId={id}"));
    }
    if let Some(id) = &event.job_id {
        line.push_str(&format!(" jobId={id}"));
    }
    if let Some(cause) = &event.error_message {
        line.push_str(&format!("\n  Cause: {cause}"));
    }
    if let Some(hint) = &event.hint {
        line.push_str(&format!("\n  Next: {hint}"));
    }
    line.push_str(&format!("\n  details={}\n", event.details));
    line
}

use crate::progress::bulk_download_output;
use dldr_jobs::JobFailure;
use serde_json::{json, Value};

pub(crate) fn account_sync_failure(error: dldr_library::LibraryError) -> JobFailure {
    library_job_failure(error)
}

pub(crate) fn work_download_failure(error: dldr_library::LibraryError) -> JobFailure {
    library_job_failure(error)
}

pub(crate) fn library_job_failure(error: dldr_library::LibraryError) -> JobFailure {
    let code = error.failure_code().to_owned();
    let message = error.support_message();
    let details = error.support_details();

    let failure = job_failure_with_details(code, message, details);
    if let dldr_library::LibraryError::BulkInterrupted {
        report,
        interrupted_work,
        ..
    } = error
    {
        let mut output = bulk_download_output(&report, 0);
        output.insert("interruptedWorkId".to_owned(), json!(interrupted_work));
        failure.with_detail("bulkDownload", json!(output))
    } else {
        failure
    }
}

pub(crate) fn job_failure_with_details(
    code: impl Into<String>,
    message: impl Into<String>,
    details: Value,
) -> JobFailure {
    let mut failure = JobFailure::with_code(code, message);

    match details {
        Value::Object(details) => {
            for (key, value) in details {
                failure = failure.with_detail(key, value);
            }
        }
        other => {
            failure = failure.with_detail("diagnostic", other);
        }
    }

    failure
}

pub(crate) fn command_error(error: impl ToString + 'static) -> String {
    let value = &error as &dyn std::any::Any;
    let (code, details) = if let Some(e) = value.downcast_ref::<dldr_library::LibraryError>() {
        (e.failure_code(), e.support_details())
    } else if let Some(e) = value.downcast_ref::<std::io::Error>() {
        (
            "io",
            json!({"failureKind":"io", "osCode":e.raw_os_error(), "ioKind":format!("{:?}",e.kind())}),
        )
    } else if let Some(e) = value.downcast_ref::<dldr_player::Error>() {
        let os_code = if let dldr_player::Error::Io(io) = e {
            io.raw_os_error()
        } else {
            None
        };
        (
            "player",
            json!({"failureKind":"player", "playerErrorKind":e.diagnostic_kind(), "osCode":os_code}),
        )
    } else if let Some(error) = value.downcast_ref::<dldr_storage::StorageError>() {
        ("storage", error.support_details())
    } else if value.is::<dldr_credentials::CredentialsError>() {
        ("credentials", json!({}))
    } else {
        ("failed", json!({}))
    };
    dldr_audit::record_current_failure(code, details);
    error.to_string()
}

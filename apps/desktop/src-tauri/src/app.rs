use crate::two_factor::TwoFactorPrompts;
use crate::{
    commands::{self},
    diagnostics, dlsite_play, window_appearance,
};
use crate::{progress::*, state::*};
use dldr_audit::{AuditEvent, AuditLogger};
use dldr_credentials::{CredentialStore, LocalCredentialStore};
use dldr_jobs::{JobEventKind, JobManager};
use dldr_library::Library;
use dldr_storage::Storage;
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::broadcast::error::RecvError;
use tracing_subscriber::prelude::*;

fn forward_job_events(
    app: AppHandle,
    jobs: JobManager,
    audit: AuditLogger,
    download_reservations: DownloadReservations,
    storage: Storage,
) {
    let mut receiver = jobs.subscribe();

    tauri::async_runtime::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if event.event_kind == JobEventKind::Finished {
                        if let Some(reservation_id) = job_download_reservation_id(&event) {
                            let _ = storage.pause_queued_batch(reservation_id).await;
                            let released = download_reservations.release_owner(reservation_id);

                            if released > 0 {
                                tracing::debug!(
                                    target: "dldr::downloads",
                                    job_id = %event.job_id,
                                    reservation_id,
                                    released,
                                    "released download reservations"
                                );
                            }
                        }
                    }

                    if app.emit("dldr-job-event", event).is_err() {
                        audit.record_now(AuditEvent::failed("job.events", ""));
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    audit.record_now(
                        AuditEvent::failed("job.events", "")
                            .with_details(json!({"skipped":skipped})),
                    );
                }
                Err(RecvError::Closed) => {
                    audit.record_now(AuditEvent::failed("job.events", ""));
                    break;
                }
            }
        }
    });
}

fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let audit = app.state::<AuditLogger>().inner().clone();
    let log_dir = audit.log_dir().to_path_buf();
    let app_data_dir = dldr_library::profile_dir()?;
    let mut directories = audit.operation("app.startup");
    let result = std::fs::create_dir_all(&app_data_dir);
    directories.finish(if result.is_ok(){dldr_audit::AuditOutcome::Succeeded}else{dldr_audit::AuditOutcome::Failed},json!({"phase":"directories","osCode":result.as_ref().err().and_then(std::io::Error::raw_os_error)}));
    result?;
    let instance = dldr_library::LibraryInstanceGuard::acquire(&app_data_dir)?;
    let database_path: PathBuf = app_data_dir.join("dldr.sqlite");
    let mut database = audit.operation("app.startup");
    let storage_result = tauri::async_runtime::block_on(async {
        let storage = Storage::open(&database_path).await?;
        storage.run_migrations().await?;
        dldr_storage::Result::Ok(storage)
    });
    database.finish(if storage_result.is_ok(){dldr_audit::AuditOutcome::Succeeded}else{dldr_audit::AuditOutcome::Failed},json!({"phase":"database","errorDetails":storage_result.as_ref().err().map(|error|error.support_details())}));
    let storage = storage_result?;
    let download_rate_limiter = dldr_download::DownloadRateLimiter::default();
    let saved_rate = tauri::async_runtime::block_on(storage.download_rate_limit())?;
    download_rate_limiter
        .set_limit(saved_rate)
        .map_err(std::io::Error::other)?;
    let credential_vault_path = app_data_dir.join("credentials").join("vault.json");
    let mut vault = audit.operation("app.startup");
    let result = LocalCredentialStore::open(&credential_vault_path);
    vault.finish(if result.is_ok(){dldr_audit::AuditOutcome::Succeeded}else{dldr_audit::AuditOutcome::Failed},json!({"phase":"credentials","failureKind":if result.is_err(){Some("credentials")}else{None}}));
    let credentials: Arc<dyn CredentialStore> = Arc::new(result?);
    let library = Library::new(storage.clone(), credentials);
    tauri::async_runtime::block_on(storage.recover_download_queue())?;
    library
        .transfers()
        .set_limit(tauri::async_runtime::block_on(
            storage.download_concurrency(),
        )?)?;
    let jobs = JobManager::default().with_diagnostics(audit.clone());
    let download_reservations = DownloadReservations::default();

    tracing::info!(
        target: "dldr::app",
        log_dir = %log_dir.display(),
        data_dir = %app_data_dir.display(),
        credential_vault = %credential_vault_path.display(),
        "app setup completed"
    );
    tauri::async_runtime::block_on(record_audit(
        &audit,
        AuditEvent::succeeded("app.startup", "Started application").with_details(json!({
            "logDir": log_dir.to_string_lossy().to_string(),
            "dataDir": app_data_dir.to_string_lossy().to_string(),
        })),
    ));

    forward_job_events(
        app.handle().clone(),
        jobs.clone(),
        audit.clone(),
        download_reservations.clone(),
        storage.clone(),
    );
    let player_app = app.handle().clone();
    let player_audit = audit.clone();
    let last_player_failure = std::sync::Mutex::new(None);
    app.manage(dldr_player::Player::start(
        app_data_dir.join("player-history.json"),
        move |snapshot| {
            let failure = snapshot.error.as_ref().map(|message| {
                (
                    snapshot.work_id.clone(),
                    snapshot.track_id.clone(),
                    message.clone(),
                )
            });
            if let Ok(mut previous) = last_player_failure.lock() {
                if failure != *previous {
                    if let Some((work_id, _, message)) = &failure {
                        let kind = [
                            dldr_player::Error::EngineUnavailable,
                            dldr_player::Error::EngineInitialization,
                            dldr_player::Error::Archive,
                            dldr_player::Error::Playback,
                            dldr_player::Error::InvalidPath,
                        ]
                        .iter()
                        .find(|error| error.to_string() == *message)
                        .map_or("playback", |error| error.diagnostic_kind());
                        player_audit
                            .record_now(AuditEvent::failed("player.playback", "").with_details(
                            json!({"workId":work_id,"failureKind":"player","playerErrorKind":kind}),
                        ));
                    }
                    *previous = failure;
                }
            }
            let _ = player_app.emit("dldr-player-state", snapshot);
        },
    ));
    app.manage(dlsite_play::PlayBrowsers::default());
    app.manage(AppState {
        _instance: instance,
        mutation_lock: tokio::sync::RwLock::new(()),
        queue_lock: tokio::sync::Mutex::new(()),
        download_rate_limiter,
        download_rate_settings_lock: tokio::sync::Mutex::new(()),
        storage,
        library,
        jobs,
        audit,
        download_reservations,
        two_factor_prompts: TwoFactorPrompts::default(),
    });

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut context = tauri::generate_context!();
    let profile = dldr_library::profile_dir().expect("DLDR configuration directory unavailable");
    // Config-file dataDirectory only supports paths relative to LocalData.
    // Build the windows explicitly so browser storage can use the config root.
    let windows = context
        .config()
        .app
        .windows
        .iter()
        .filter(|window| window.create)
        .cloned()
        .collect::<Vec<_>>();
    for window in &mut context.config_mut().app.windows {
        window.create = false;
    }
    // GTK's Wayland surface also uses the program name for desktop matching.
    #[cfg(target_os = "linux")]
    gtk::glib::set_program_name(Some(context.config().identifier.as_str()));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            // GTK initializes the process locale. libmpv rejects non-C numeric
            // locales, even when their decimal separator also happens to be a dot.
            // Reset this once on the main thread before starting player workers.
            #[cfg(unix)]
            unsafe {
                libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
            }
            let audit=diagnostics::initialize();
            let _=tracing::subscriber::set_global_default(tracing_subscriber::registry().with(dldr_audit::DiagnosticLayer(audit.clone())));
            app.manage(audit.clone());
            app.manage(diagnostics::FrontendReports::default());
            for window in &windows {
                tauri::WebviewWindowBuilder::from_config(app, window)?
                    .data_directory(profile.join("webview"))
                    .build()?;
            }
            if let Err(error) = window_appearance::setup(app) {
                tracing::warn!(%error, "Could not customize the native titlebar");
            }
            if setup_app(app).is_err() {
                audit.record_now(AuditEvent::failed("app.startup", ""));
                app.dialog().message(format!("DLDR initialization failed. Open Settings → Logs to export diagnostics. Support run: {}",audit.run_id())).title("DLDR startup failure").show(|_|{});
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                dlsite_play::close_browsers(window.app_handle());
            }
        })
        .invoke_handler(|invoke| {
            if invoke.message.webview_ref().label() != "main" {
                invoke.resolver.reject("Application commands are restricted to the DLDR main window");
                return true;
            }
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
            crate::window_appearance::set_native_header_theme,
            crate::dlsite_play::open_dlsite_play,
            commands::player::open_player,
            commands::player::control_player,
            commands::player::player_snapshot,
            commands::player::player_subtitle,
            commands::library_tools::get_transfer_settings,
            commands::library_tools::set_transfer_concurrency,
            commands::library_tools::list_download_queue,
            commands::library_tools::control_download_queue,
            commands::library_tools::retry_failed_downloads,
            commands::library_tools::clear_download_queue,
            commands::library_tools::check_download_space,
            commands::library_tools::verify_library,
            commands::library_tools::export_library_backup,
            commands::library_tools::restore_library_backup,
            commands::settings::get_settings,
            commands::library_tools::get_setup_folders,
            commands::library_tools::configure_download_folders,
            commands::settings::list_name_languages,
            commands::settings::save_settings,
            commands::accounts::list_accounts,
            commands::accounts::save_account,
            commands::accounts::set_account_enabled,
            commands::accounts::remove_account,
            commands::library::list_products,
            commands::playlists::list_playlists,
            commands::playlists::edit_playlist,
            commands::library::list_product_filter_facets,
            commands::library::get_product_detail,
            commands::library::save_product_cover,
            commands::library::get_product_page_content,
            commands::accounts::start_account_sync,
            commands::downloads::start_work_download,
            commands::downloads::start_bulk_work_download,
            commands::downloads::preview_bulk_work_download,
            commands::accounts::submit_two_factor_code,
            commands::accounts::cancel_two_factor,
            commands::downloads::open_work_download,
            commands::downloads::delete_work_download,
            commands::downloads::mark_work_downloaded,
            commands::downloads::scan_local_work_downloads,
            commands::jobs::list_jobs,
            commands::jobs::get_job,
            commands::jobs::cancel_job,
            commands::jobs::get_job_logs,
            commands::jobs::clear_finished_jobs,
            commands::jobs::list_audit_events,
            commands::jobs::get_audit_log_dir,
            commands::desktop::open_audit_log_dir,
            commands::desktop::open_download_folder,
            commands::settings::get_download_rate_limit,
            commands::settings::set_download_rate_limit,
            commands::desktop::open_external_url,
            diagnostics::diagnostic_summary,
            diagnostics::diagnostic_runs,
            diagnostics::diagnostic_operation,
            diagnostics::export_diagnostics,
            diagnostics::report_frontend_failure,
        ];
            handler(invoke)
        })
        .build(context)
        .expect("error while building tauri application")
        .run(|app,event| {
            if matches!(event,tauri::RunEvent::Exit) {
                if let Some(player)=app.try_state::<dldr_player::Player>() {player.shutdown();}
                if let Some(audit)=app.try_state::<AuditLogger>() {audit.flush(Duration::from_secs(2));}
            }
        });
}

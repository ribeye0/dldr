//! A scoped official-site browser. Remote pages never receive application IPC permissions.
use crate::{
    commands::{downloads::start_work_download_inner, library_tools},
    opener,
};
use crate::{dto::*, errors::*, state::*, validation::*};
use dldr_storage::{UiLocale, WorkDownloadStatus};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{webview::NewWindowResponse, WebviewUrl, WebviewWindowBuilder};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const ORIGIN: &str = "https://play.dlsite.com";
const BRIDGE_PATH: &str = "/__dldr_bridge";
const SCRIPT: &str = include_str!("dlsite-play.js");

#[derive(Default)]
pub(super) struct PlayBrowsers {
    launch: tokio::sync::Mutex<()>,
    windows: Mutex<BTreeMap<String, String>>,
}

fn official_origin(url: &tauri::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("play.dlsite.com")
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
}

fn allowed_navigation(url: &tauri::Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && (matches!(
            url.host_str(),
            Some("play.dlsite.com" | "www.dlsite.com" | "login.dlsite.com" | "play.dl.dlsite.com")
        ) || (matches!(url.host_str(), Some("www.google.com" | "www.recaptcha.net"))
            && url.path().starts_with("/recaptcha/"))
            || (url.host_str() == Some("challenges.cloudflare.com")
                && url.path().starts_with("/cdn-cgi/")))
}

fn work_id(value: &str) -> Result<String, String> {
    let id = value.to_ascii_uppercase();
    let (prefix, digits) = (id.get(..2).unwrap_or(""), id.get(2..).unwrap_or(""));
    if matches!(prefix, "RJ" | "BJ" | "VJ")
        && !digits.is_empty()
        && digits.len() <= 10
        && digits.bytes().all(|c| c.is_ascii_digit())
    {
        Ok(id)
    } else {
        Err("Invalid DLsite work ID".into())
    }
}

fn route_work_id(url: &tauri::Url) -> Option<String> {
    if !official_origin(url) {
        return None;
    }
    let mut parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    if parts.first() == Some(&"admin") {
        parts.remove(0);
    }
    if parts.first() == Some(&"dllibrary") {
        parts.remove(0);
    }
    if parts.first() != Some(&"work") {
        return None;
    }
    work_id(parts.get(1)?).ok()
}

fn target_url(id: Option<&str>) -> Result<tauri::Url, String> {
    let path = match id {
        Some(id) => format!("/work/{}", work_id(id)?),
        None => "/library".into(),
    };
    tauri::Url::parse(&format!("{ORIGIN}{path}")).map_err(|_| "Invalid Play URL".into())
}

fn profile_directory(base: &Path, account_id: &str) -> Result<PathBuf, String> {
    if account_id.is_empty() || account_id.len() > 120 {
        return Err("Invalid account ID".into());
    }
    // An injective encoding keeps arbitrary stored IDs out of filesystem syntax.
    let key: String = account_id
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(base.join("play-browser").join(key))
}

#[derive(Debug, PartialEq)]
struct BridgeRequest {
    work_id: String,
    download: bool,
    sequence: u64,
}

fn bridge_request(url: &tauri::Url, token: &str) -> Option<BridgeRequest> {
    if !official_origin(url) || url.path() != BRIDGE_PATH || url.fragment().is_some() {
        return None;
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    if pairs.len() != 4 {
        return None;
    }
    let unique: BTreeMap<_, _> = pairs.into_iter().collect();
    if unique.len() != 4 || unique.get("token")?.as_ref() != token {
        return None;
    }
    let download = match unique.get("action")?.as_ref() {
        "status" => false,
        "download" => true,
        _ => return None,
    };
    Some(BridgeRequest {
        work_id: work_id(unique.get("work")?).ok()?,
        download,
        sequence: unique.get("sequence")?.parse().ok()?,
    })
}

const MESSAGE_KEYS: &[&str] = &[
    "Download to DLDR",
    "Download queued",
    "Downloading",
    "Downloaded",
    "Resume download",
    "Retry download",
    "Checking download status…",
    "DLDR account: {0}",
    "Sign into the same account on the official website.",
    "Sync purchases in DLDR before downloading this work.",
    "This work belongs to a different download account.",
    "Configure download folders in DLDR Settings first.",
    "Could not add the download. Check DLDR Logs.",
    "Could not read download status. Try again.",
    "This DLDR account is unavailable. Reopen Play from DLDR.",
    "Open DLDR for verification or download progress.",
];
fn messages(locale: UiLocale) -> Value {
    let source = match locale {
        UiLocale::TraditionalChinese => include_str!("../../src/lib/i18n/zh-TW.json"),
        UiLocale::SimplifiedChinese => include_str!("../../src/lib/i18n/zh-CN.json"),
        UiLocale::Japanese => include_str!("../../src/lib/i18n/ja.json"),
        UiLocale::English => include_str!("../../src/lib/i18n/en.json"),
    };
    let all: Value = serde_json::from_str(source).expect("bundled translations");
    Value::Object(
        MESSAGE_KEYS
            .iter()
            .map(|key| {
                (
                    key.to_string(),
                    all.get(*key).cloned().unwrap_or(json!(key)),
                )
            })
            .collect(),
    )
}

#[tauri::command]
pub(super) async fn open_dlsite_play(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    work_id: Option<String>,
) -> Result<(), String> {
    state
        .audit
        .operation("open_dlsite_play")
        .command(open_browser(app, &state, account_id, work_id))
        .await
}

async fn open_browser(
    app: AppHandle,
    state: &AppState,
    account_id: String,
    work_id: Option<String>,
) -> Result<(), String> {
    let browsers = app.state::<PlayBrowsers>();
    let _launch = browsers.launch.lock().await;
    let _admission = state.mutation_lock.read().await;
    let account = state
        .library
        .accounts()
        .await
        .map_err(command_error)?
        .into_iter()
        .find(|a| a.id == account_id && a.enabled)
        .ok_or("Choose an enabled DLDR account first")?;
    let target = target_url(work_id.as_deref())?;
    if let Some(id) = work_id.as_deref() {
        let detail = state
            .library
            .product_detail(&self::work_id(id)?)
            .await
            .map_err(command_error)?;
        if !detail.owners.iter().any(|o| o.account_id == account.id) {
            return Err("This work is not owned by the selected DLDR account".into());
        }
    }
    let existing = browsers
        .windows
        .lock()
        .expect("Play windows")
        .get(&account.id)
        .cloned();
    if let Some(window) = existing.and_then(|label| app.get_webview_window(&label)) {
        if work_id.is_some() {
            window
                .navigate(target)
                .map_err(|_| "Could not navigate DLsite Play")?;
        }
        window.show().map_err(|_| "Could not show DLsite Play")?;
        window
            .set_focus()
            .map_err(|_| "Could not focus DLsite Play")?;
        return Ok(());
    }
    // data_directory provides independent persistent contexts on Linux/Windows.
    // Do not silently share cookies on platforms where this setting is unsupported.
    if !cfg!(any(target_os = "linux", target_os = "windows")) {
        return Err("DLsite Play browser profiles currently support Linux and Windows".into());
    }
    let directory = profile_directory(
        &dldr_library::profile_dir().map_err(|_| "Application directory unavailable")?,
        &account.id,
    )?;
    std::fs::create_dir_all(&directory).map_err(|_| "Could not create Play browser profile")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not protect Play browser profile")?;
    }
    let label = format!("dlsite-play-{}", Uuid::new_v4());
    let token = Uuid::new_v4().to_string();
    let locale = state
        .storage
        .app_settings()
        .await
        .map_err(command_error)?
        .ui_locale;
    let script = format!(
        "({SCRIPT})({});",
        json!({"token":token,"accountLabel":account.label,"messages":messages(locale)})
    );
    let busy = Arc::new(AtomicBool::new(false));
    let nav_app = app.clone();
    let nav_label = label.clone();
    let nav_account = account.id.clone();
    let popup_app = app.clone();
    let popup_label = label.clone();
    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(target))
        .title(format!("DLsite Play · {} — DLDR", account.label))
        .inner_size(1120.0, 800.0)
        .min_inner_size(480.0, 480.0)
        .data_directory(directory)
        .initialization_script(&script)
        .on_download(|_, _| false)
        .on_navigation(move |url| {
            if official_origin(url) && url.path() == BRIDGE_PATH {
                if let Some(request) = bridge_request(url, &token) {
                    if busy
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                        .is_ok()
                    {
                        let app = nav_app.clone();
                        let label = nav_label.clone();
                        let account = nav_account.clone();
                        let busy = busy.clone();
                        tauri::async_runtime::spawn(async move {
                            handle_bridge(&app, &label, &account, request).await;
                            busy.store(false, Ordering::Release);
                        });
                    }
                }
                return false;
            }
            allowed_navigation(url)
        })
        .on_new_window(move |url, _| {
            let app = popup_app.clone();
            let label = popup_label.clone();
            tauri::async_runtime::spawn(async move {
                if allowed_navigation(&url) {
                    if let Some(window) = app.get_webview_window(&label) {
                        let _ = window.navigate(url);
                    }
                } else {
                    navigate_or_open_external(&app, &url);
                }
            });
            NewWindowResponse::Deny
        })
        .build()
        .map_err(|_| "Could not open DLsite Play browser")?;
    browsers
        .windows
        .lock()
        .expect("Play windows")
        .insert(account.id, label);
    window
        .set_focus()
        .map_err(|_| "Could not focus DLsite Play")?;
    Ok(())
}

fn navigate_or_open_external(app: &AppHandle, url: &tauri::Url) -> bool {
    if allowed_navigation(url) {
        return true;
    }
    // Only popup/new-window requests reach the opener, never automatic subframe navigation.
    // Never pass credentials, file URLs, scripts or custom schemes to an opener.
    if matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
    {
        let app = app.clone();
        let url = url.to_string();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let _ = opener::open_url(&app, &state.audit, &url).await;
        });
    }
    false
}

async fn handle_bridge(app: &AppHandle, label: &str, account_id: &str, request: BridgeRequest) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    // Re-check the actual top-level source after the navigation callback returns.
    if window.url().ok().and_then(|u| route_work_id(&u)).as_deref() != Some(&request.work_id) {
        return;
    }
    let result = bridge_operation(app, account_id, &request).await;
    let (status, message) =
        result.unwrap_or(("error", "Could not read download status. Try again."));
    let response = json!({"workId":request.work_id,"sequence":request.sequence,"status":status,"message":message});
    // JSON is code-serialized; no page text/HTML is evaluated.
    let _ = window.eval(format!(
        "window.dispatchEvent(new CustomEvent('dldr-play-status',{{detail:{response}}}));"
    ));
}

fn local_status(
    status: WorkDownloadStatus,
    entry: Option<&dldr_storage::DownloadQueueEntry>,
    account_id: &str,
) -> (&'static str, &'static str) {
    if status == WorkDownloadStatus::Downloaded {
        return ("downloaded", "Downloaded");
    }
    if let Some(entry) = entry {
        if matches!(
            entry.status.as_str(),
            "queued" | "running" | "pausing" | "cancelling"
        ) {
            return (
                if entry.status == "queued" {
                    "queued"
                } else {
                    "downloading"
                },
                "Open DLDR for verification or download progress.",
            );
        }
        if entry
            .account_id
            .as_deref()
            .is_some_and(|id| id != account_id)
        {
            return (
                "unavailable",
                "This work belongs to a different download account.",
            );
        }
    }
    match entry.map(|e| e.status.as_str()) {
        Some("paused" | "cancelled") => ("ready", "Resume download"),
        Some("failed") => ("ready", "Retry download"),
        _ => ("ready", "Download to DLDR"),
    }
}

async fn bridge_operation(
    app: &AppHandle,
    account_id: &str,
    request: &BridgeRequest,
) -> Result<(&'static str, &'static str), String> {
    let state = app.state::<AppState>();
    let _admission = state.mutation_lock.read().await;
    let accounts = state.library.accounts().await.map_err(command_error)?;
    if !accounts.iter().any(|a| a.id == account_id && a.enabled) {
        return Ok((
            "unavailable",
            "This DLDR account is unavailable. Reopen Play from DLDR.",
        ));
    }
    let detail = match state.library.product_detail(&request.work_id).await {
        Ok(detail) if detail.owners.iter().any(|o| o.account_id == account_id) => detail,
        _ => {
            return Ok((
                "unavailable",
                "Sync purchases in DLDR before downloading this work.",
            ))
        }
    };
    let _queue = state.queue_lock.lock().await;
    let entry = state
        .storage
        .download_queue()
        .await
        .map_err(command_error)?
        .into_iter()
        .find(|e| e.work_id == request.work_id);
    let status = local_status(detail.download.status, entry.as_ref(), account_id);
    if status.0 != "ready" || !request.download {
        return Ok(status);
    }
    let settings = state.storage.app_settings().await.map_err(command_error)?;
    if entry.is_none() && required_library_root(&settings).is_err() {
        return Ok((
            "error",
            "Configure download folders in DLDR Settings first.",
        ));
    }
    let started = if let Some(mut entry) =
        entry.filter(|e| matches!(e.status.as_str(), "paused" | "failed" | "cancelled"))
    {
        entry.account_id = Some(account_id.to_owned());
        library_tools::resume_entry(app.clone(), &state, entry).await
    } else {
        start_work_download_inner(
            app.clone(),
            &state,
            StartWorkDownloadRequest {
                work_id: request.work_id.clone(),
                account_id: Some(account_id.to_owned()),
                password: None,
                unpack_policy: None,
                replace_existing: Some(false),
            },
            None,
        )
        .await
    };
    match started {
        Ok(_) => Ok(("queued", "Download queued")),
        Err(_) => Ok(("error", "Could not add the download. Check DLDR Logs.")),
    }
}

pub(super) fn close_browsers(app: &AppHandle) {
    if let Some(state) = app.try_state::<PlayBrowsers>() {
        let labels: Vec<_> = state
            .windows
            .lock()
            .expect("Play windows")
            .values()
            .cloned()
            .collect();
        for label in labels {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.close();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::library_tools::queue_entry;
    #[test]
    fn strict_routes_and_official_navigation() {
        for path in [
            "/work/RJ01760323",
            "/work/rj1/tree",
            "/admin/dllibrary/work/BJ123/view/track",
        ] {
            assert!(
                route_work_id(&tauri::Url::parse(&format!("{ORIGIN}{path}")).unwrap()).is_some()
            );
        }
        for path in [
            "/library?work=RJ1",
            "/viewer/sample/RJ1",
            "/work/RJ1evil",
            "/work/RJ1%2Fetc",
        ] {
            assert!(
                route_work_id(&tauri::Url::parse(&format!("{ORIGIN}{path}")).unwrap()).is_none()
            );
        }
        for url in [
            "https://play.dlsite.com.evil.test/work/RJ1",
            "http://play.dlsite.com/work/RJ1",
            "https://evil@play.dlsite.com/work/RJ1",
            "https://play.dlsite.com:444/work/RJ1",
            "file:///etc/passwd",
        ] {
            let url = tauri::Url::parse(url).unwrap();
            assert!(!allowed_navigation(&url));
            assert!(route_work_id(&url).is_none());
        }
        for url in [
            "https://login.dlsite.com/oauth2/auth",
            "https://play.dl.dlsite.com/media",
            "https://www.google.com/recaptcha/api2/anchor",
            "https://challenges.cloudflare.com/cdn-cgi/challenge-platform/widget",
        ] {
            assert!(allowed_navigation(&tauri::Url::parse(url).unwrap()));
        }
        assert!(!allowed_navigation(
            &tauri::Url::parse("https://www.google.com/search").unwrap()
        ));
        assert_eq!(
            target_url(Some("rj01760323")).unwrap().as_str(),
            "https://play.dlsite.com/work/RJ01760323"
        );
        assert!(target_url(Some("../RJ1")).is_err());
    }
    #[test]
    fn bridge_validates_token_action_id_duplicates_and_scheme() {
        let base =
            format!("{ORIGIN}{BRIDGE_PATH}?token=secret&action=download&work=RJ1&sequence=42");
        let request = bridge_request(&tauri::Url::parse(&base).unwrap(), "secret").unwrap();
        assert_eq!(
            request,
            BridgeRequest {
                work_id: "RJ1".into(),
                download: true,
                sequence: 42
            }
        );
        for url in [
            base.replace("secret", "wrong"),
            format!("{base}&action=status"),
            base.replace("download", "delete"),
            base.replace("RJ1", "RJ1/../etc"),
            base.replace("https:", "http:"),
            format!("{base}#frame"),
        ] {
            assert!(bridge_request(&tauri::Url::parse(&url).unwrap(), "secret").is_none());
        }
    }
    #[test]
    fn local_status_never_replaces_completed_active_or_foreign_account_downloads() {
        let mut entry = queue_entry(
            "RJ1",
            "batch",
            Some("a".into()),
            Path::new("/library"),
            Path::new("/temp"),
            dldr_download::UnpackPolicy::UnpackWhenRecognized,
            false,
        );
        for (status, expected) in [
            ("queued", "queued"),
            ("running", "downloading"),
            ("pausing", "downloading"),
            ("cancelling", "downloading"),
            ("paused", "ready"),
            ("cancelled", "ready"),
            ("failed", "ready"),
        ] {
            entry.status = status.into();
            assert_eq!(
                local_status(WorkDownloadStatus::NotDownloaded, Some(&entry), "a").0,
                expected
            );
            assert_eq!(
                local_status(WorkDownloadStatus::Downloaded, Some(&entry), "a").0,
                "downloaded"
            );
        }
        entry.status = "paused".into();
        assert_eq!(
            local_status(WorkDownloadStatus::NotDownloaded, Some(&entry), "b").0,
            "unavailable"
        );
        entry.account_id = None;
        assert_eq!(
            local_status(WorkDownloadStatus::NotDownloaded, Some(&entry), "b"),
            ("ready", "Resume download")
        );
        assert_eq!(
            local_status(WorkDownloadStatus::NotDownloaded, None, "b"),
            ("ready", "Download to DLDR")
        );
    }
    #[test]
    fn profiles_are_distinct_private_paths_and_messages_are_translated() {
        let root = Path::new("/profile");
        let a = profile_directory(root, "account/a").unwrap();
        assert_ne!(a, profile_directory(root, "account-a").unwrap());
        assert!(a.starts_with(root.join("play-browser")));
        assert!(!a.to_str().unwrap().contains("account/a"));
        assert!(profile_directory(root, "").is_err());
        for locale in [
            UiLocale::TraditionalChinese,
            UiLocale::SimplifiedChinese,
            UiLocale::Japanese,
            UiLocale::English,
        ] {
            let values = messages(locale);
            assert_eq!(values.as_object().unwrap().len(), MESSAGE_KEYS.len());
            for key in MESSAGE_KEYS {
                assert!(values[*key].as_str().is_some_and(|v| !v.is_empty()));
            }
        }
    }
}

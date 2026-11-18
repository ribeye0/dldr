use dldr_library::LibraryInstanceGuard;
use dldr_storage::{
    AccountSyncCommit, AccountUpsert, AccountWork, AppSettingsPatch, CachedWork, Storage,
    WorkDownloadStatus, WorkDownloadUpdate,
};
use serde_json::{json, Value};
use std::{
    path::Path,
    process::{Command, Output},
};

fn invoke(data: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dldr"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap()
}
fn parsed(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "invalid stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[cfg(unix)]
#[tokio::test]
async fn default_profile_uses_config_home_and_never_imports_or_removes_legacy_data() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let data = temp.path().join("data");
    let profile = config.join("dldr");
    let legacy = data.join("io.github.ribeye0.dldr");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("keep.txt"), "legacy").unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dldr"));
        command
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_DATA_HOME", &data)
            .args(["config", "--json"]);
        command
    };
    let missing = command().output().unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(!profile.exists());
    assert_eq!(std::fs::read(legacy.join("keep.txt")).unwrap(), b"legacy");

    std::fs::create_dir_all(&profile).unwrap();
    let storage = seed(&profile).await;
    let result = command().output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        parsed(&result)["data_dir"],
        profile.to_string_lossy().as_ref()
    );
    assert_eq!(std::fs::read(legacy.join("keep.txt")).unwrap(), b"legacy");
    drop(storage);
}
async fn seed(dir: &Path) -> Storage {
    let storage = Storage::open(dir.join("dldr.sqlite")).await.unwrap();
    storage.run_migrations().await.unwrap();
    let library = dir.join("作品庫 with spaces");
    let temp = dir.join("暫存");
    std::fs::create_dir_all(library.join("RJ000001")).unwrap();
    std::fs::create_dir_all(&temp).unwrap();
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            library_root: Some(library.to_str().unwrap().into()),
            download_root: Some(temp.to_str().unwrap().into()),
            ..Default::default()
        })
        .await
        .unwrap();
    storage
        .save_account(&AccountUpsert {
            id: "account-a".into(),
            label: "測試帳號".into(),
            login_name: Some("private@example.invalid".into()),
            credential_ref: Some("private-vault-key".into()),
            enabled: true,
        })
        .await
        .unwrap();
    let works=(1..=2).map(|i|CachedWork {
        work_id:format!("RJ{i:06}"),title_json:json!({"ja_JP":format!("作品 {i} NTR")}).to_string(),
        maker_id:Some("circle".into()),maker_json:Some(json!({"ja_JP":"森のサークル"}).to_string()),work_type:Some("SOU".into()),age_category:Some("r18".into()),
        thumbnail_url:None,registered_at:None,published_at:None,updated_at:None,
        raw_json:json!({"content_size":1024,"genre_ids":[503],"tags":[{"class":"voice_by","name":"声優一"},{"class":"voice_by","name":"声優二"}]}).to_string(),last_detail_sync_at:"2026-11-16T00:00:00Z".into()
    }).collect();
    storage
        .commit_account_sync(&AccountSyncCommit {
            sync_run_id: "fixture-sync".into(),
            account_id: "account-a".into(),
            started_at: "2026-11-16T00:00:00Z".into(),
            completed_at: "2026-11-16T00:00:01Z".into(),
            works,
            account_works: (1..=2)
                .map(|i| AccountWork {
                    work_id: format!("RJ{i:06}"),
                    purchased_at: None,
                })
                .collect(),
        })
        .await
        .unwrap();
    storage
        .save_work_download(&WorkDownloadUpdate {
            work_id: "RJ000001".into(),
            status: WorkDownloadStatus::Downloaded,
            local_path: Some(library.join("RJ000001").to_str().unwrap().into()),
            staging_path: None,
            unpack_policy: "manual".into(),
            bytes_received: 1024,
            bytes_total: Some(1024),
            error_code: None,
            error_message: None,
            started_at: None,
            completed_at: None,
            updated_at: "2026-11-16T00:00:01Z".into(),
        })
        .await
        .unwrap();
    storage
}

#[test]
fn help_and_invalid_arguments_never_initialize_a_profile() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("not-created");
    for args in [vec!["--help"], vec!["--version"], vec![]] {
        assert!(invoke(&missing, &args).status.success());
    }
    for args in [
        vec!["download"],
        vec!["dl", "../RJ1"],
        vec!["dl", "RJ000002", "-j", "9"],
        vec!["search", "--limit", "0"],
        vec!["gui"],
        vec!["RJ1", "search", "NTR"],
        vec!["-j", "2", "search", "NTR"],
    ] {
        assert_eq!(invoke(&missing, &args).status.code(), Some(2));
    }
    assert!(!missing.exists());
}

#[tokio::test]
async fn cached_queries_are_read_only_and_work_while_another_process_owns_the_profile() {
    let temp = tempfile::tempdir().unwrap();
    let storage = seed(temp.path()).await;
    let _gui = LibraryInstanceGuard::acquire(temp.path()).unwrap();
    let before = std::fs::read(temp.path().join("dldr.sqlite")).unwrap();
    let search = invoke(
        temp.path(),
        &[
            "search",
            "$circle:森のサークル$ $va:声優二$ $tag:低語$",
            "--json",
            "-n",
            "1",
            "-p",
            "2",
        ],
    );
    assert!(search.status.success());
    let page = parsed(&search);
    assert_eq!(page["total"], 2);
    assert_eq!(page["works"].as_array().unwrap().len(), 1);
    let downloaded = invoke(temp.path(), &["ls", "--downloaded", "--json"]);
    assert_eq!(parsed(&downloaded)["total"], 1);
    let show = parsed(&invoke(temp.path(), &["show", "rj000002", "--json"]));
    assert_eq!(show["voice_actors"], json!(["声優一", "声優二"]));
    let plan = invoke(
        temp.path(),
        &["RJ000001", "RJ000002", "RJ000002", "--dry-run", "--json"],
    );
    assert!(plan.status.success());
    let plan = parsed(&plan);
    assert_eq!(plan["selected"], 1);
    assert_eq!(plan["skipped_downloaded"], 1);
    assert_eq!(plan["known_bytes"], 1024);
    let accounts = invoke(temp.path(), &["accounts", "--json"]);
    let text = String::from_utf8(accounts.stdout).unwrap();
    assert!(!text.contains("private@example"));
    assert!(!text.contains("private-vault-key"));
    let open = invoke(temp.path(), &["open", "RJ000001", "--print"]);
    assert_eq!(
        Path::new(String::from_utf8(open.stdout).unwrap().trim()),
        temp.path().join("作品庫 with spaces/RJ000001").as_path()
    );
    assert!(storage.download_queue().await.unwrap().is_empty());
    assert!(!temp.path().join("credentials").exists());
    assert_eq!(
        std::fs::read(temp.path().join("dldr.sqlite")).unwrap(),
        before
    );
    let blocked = invoke(temp.path(), &["RJ000002", "--json"]);
    assert_eq!(blocked.status.code(), Some(1));
    assert!(String::from_utf8(blocked.stderr)
        .unwrap()
        .contains("Another GUI or dldr"));
    assert!(storage.download_queue().await.unwrap().is_empty());
}

#[tokio::test]
async fn explicit_download_errors_are_durable_and_resume_keeps_the_saved_roots() {
    let temp = tempfile::tempdir().unwrap();
    let storage = seed(temp.path()).await;
    // No vault/password/session exists. Authentication must fail before any authenticated request.
    let result = invoke(temp.path(), &["dl", "RJ000002", "--json", "-j", "1"]);
    assert_eq!(result.status.code(), Some(1));
    let report = parsed(&result);
    assert_eq!(report["failed"].as_array().unwrap().len(), 1);
    let queue = storage.download_queue().await.unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].status, "failed");
    assert!(queue[0].library_root.ends_with("作品庫 with spaces"));
    let retry = invoke(temp.path(), &["resume", "RJ000002", "--json"]);
    assert_eq!(retry.status.code(), Some(1));
    assert_eq!(
        storage.download_queue().await.unwrap()[0].library_root,
        queue[0].library_root
    );
    let status = parsed(&invoke(temp.path(), &["status", "--json"]));
    assert_eq!(status["queue"][0]["status"], "failed");
    assert!(!String::from_utf8(retry.stdout)
        .unwrap()
        .contains("private@example"));
}

#[tokio::test]
async fn search_downloads_require_explicit_confirmation_and_invalid_selections_do_not_queue() {
    let temp = tempfile::tempdir().unwrap();
    let storage = seed(temp.path()).await;
    let confirmation = invoke(temp.path(), &["dl", "--search", "NTR", "--json"]);
    assert_eq!(confirmation.status.code(), Some(2));
    assert!(storage.download_queue().await.unwrap().is_empty());
    let unknown = invoke(temp.path(), &["dl", "RJ999999", "--dry-run", "--json"]);
    assert!(!unknown.status.success());
    let malformed = invoke(temp.path(), &["search", "$duration:10$", "--json"]);
    assert!(!malformed.status.success());
    assert!(storage.download_queue().await.unwrap().is_empty());
}

#[tokio::test]
async fn read_only_connection_rejects_writes_and_never_creates_an_empty_database() {
    let temp = tempfile::tempdir().unwrap();
    let _seed = seed(temp.path()).await;
    let read = Storage::open_read_only(temp.path().join("dldr.sqlite"))
        .await
        .unwrap();
    assert!(read.save_download_concurrency(3).await.is_err());
    let missing = temp.path().join("missing.sqlite");
    assert!(Storage::open_read_only(&missing).await.is_err());
    assert!(!missing.exists());
}

#[tokio::test]
async fn saved_interface_locale_changes_genre_values_without_changing_cli_json_keys() {
    let temp = tempfile::tempdir().unwrap();
    let storage = seed(temp.path()).await;
    for (locale, label) in [
        (dldr_storage::UiLocale::TraditionalChinese, "低語"),
        (dldr_storage::UiLocale::SimplifiedChinese, "低语"),
        (dldr_storage::UiLocale::Japanese, "ささやき"),
        (dldr_storage::UiLocale::English, "Whispering"),
    ] {
        let old = storage.app_settings().await.unwrap();
        storage
            .save_app_settings_patch(&AppSettingsPatch {
                library_root: old.library_root,
                download_root: old.download_root,
                ui_locale: Some(locale),
                ..Default::default()
            })
            .await
            .unwrap();
        let detail = parsed(&invoke(temp.path(), &["show", "RJ000002", "--json"]));
        assert_eq!(detail["tags"], json!([label]));
        assert_eq!(detail["voice_actors"], json!(["声優一", "声優二"]));
        let page = parsed(&invoke(
            temp.path(),
            &["search", "$tag:Whispering$", "--json"],
        ));
        assert_eq!(page["total"], 2);
        assert_eq!(page["works"][0]["tags"], json!([label]));
    }
}

#[test]
fn login_refuses_redirected_secrets_before_initializing_a_profile() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("new-profile");
    let result = invoke(&missing, &["login", "--no-sync", "--json"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("interactive terminal"));
    assert!(!missing.exists());
}

#[tokio::test]
async fn logout_keeps_cached_ownership_and_downloads_and_respects_the_writer_lock() {
    let temp = tempfile::tempdir().unwrap();
    let storage = seed(temp.path()).await;
    let gui = LibraryInstanceGuard::acquire(temp.path()).unwrap();
    assert_eq!(invoke(temp.path(), &["logout"]).status.code(), Some(1));
    assert!(storage.accounts().await.unwrap()[0].enabled);
    drop(gui);
    let result = invoke(temp.path(), &["logout", "--json"]);
    assert!(result.status.success());
    assert_eq!(parsed(&result)["logged_out"], true);
    let account = &storage.accounts().await.unwrap()[0];
    assert!(!account.enabled);
    assert!(account.credential_ref.is_none());
    let work = parsed(&invoke(temp.path(), &["show", "RJ000001", "--json"]));
    assert!(work.to_string().contains("RJ000001"));
    assert!(temp.path().join("作品庫 with spaces/RJ000001").is_dir());
    assert_eq!(
        storage
            .product_detail("RJ000001", &Default::default())
            .await
            .unwrap()
            .owners
            .len(),
        1
    );
}

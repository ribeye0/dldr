use crate::commands::settings::*;
use crate::{dto::*, progress::*, state::*, validation::*};
use dldr_storage::ProductMakerFacet;
use serde_json::json;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
fn settings_request_preserves_omitted_priority_and_rejects_invalid_supplied_codes() {
    let path_only: SaveSettingsRequest =
        serde_json::from_value(json!({ "libraryRoot": "/library" })).unwrap();
    assert!(path_only
        .into_app_settings_patch()
        .unwrap()
        .language_priority
        .is_none());
    let supplied: SaveSettingsRequest =
        serde_json::from_value(json!({ "languagePriority": ["pt_BR", "ja_JP"] })).unwrap();
    assert_eq!(
        supplied
            .into_app_settings_patch()
            .unwrap()
            .language_priority
            .unwrap()
            .codes(),
        &["pt_BR", "ja_JP"]
    );
    for value in [json!(["ja_JP", "ja_JP"]), json!([" "])] {
        let invalid: SaveSettingsRequest =
            serde_json::from_value(json!({ "languagePriority": value })).unwrap();
        assert!(invalid.into_app_settings_patch().is_err());
    }
}

#[test]
fn download_rate_limit_dto_normalizes_unlimited_and_validates_the_native_boundary() {
    for value in [json!(null), json!(0), json!(1024), json!(1073741824u64)] {
        let request: DownloadRateLimitDto =
            serde_json::from_value(json!({"bytesPerSecond": value})).unwrap();
        assert!(request.validated_limit().is_ok());
        if request.bytes_per_second == Some(0) {
            assert_eq!(request.validated_limit().unwrap(), None);
        }
    }
    for value in [1, 1023, 1073741825u64] {
        let request: DownloadRateLimitDto =
            serde_json::from_value(json!({"bytesPerSecond": value})).unwrap();
        assert!(request.validated_limit().is_err());
    }
    for value in [json!(-1), json!(1.5), json!("1024")] {
        assert!(
            serde_json::from_value::<DownloadRateLimitDto>(json!({"bytesPerSecond": value}))
                .is_err()
        );
    }
}

#[test]
fn bulk_download_concurrency_defaults_to_two_and_rejects_unsupported_values() {
    let omitted: BulkWorkDownloadCommandRequest = serde_json::from_value(json!({})).unwrap();
    assert_eq!(omitted.concurrency().unwrap(), 2);
    for value in [1, 2, 3, 4] {
        let request: BulkWorkDownloadCommandRequest =
            serde_json::from_value(json!({"maxConcurrentWorks": value})).unwrap();
        assert_eq!(request.concurrency().unwrap(), value);
    }
    for value in [0, 5, 1000] {
        let request: BulkWorkDownloadCommandRequest =
            serde_json::from_value(json!({"maxConcurrentWorks": value})).unwrap();
        assert!(request.concurrency().is_err());
    }
    for value in [json!(-1), json!(2.5), json!("2")] {
        assert!(serde_json::from_value::<BulkWorkDownloadCommandRequest>(
            json!({"maxConcurrentWorks": value})
        )
        .is_err());
    }
}

#[test]
fn selected_bulk_ids_preserve_empty_scope_and_normalize_duplicates() {
    let all: BulkWorkDownloadCommandRequest = serde_json::from_value(json!({})).unwrap();
    assert_eq!(all.selected_work_ids().unwrap(), None);
    let empty: BulkWorkDownloadCommandRequest =
        serde_json::from_value(json!({"workIds": []})).unwrap();
    assert_eq!(empty.selected_work_ids().unwrap(), Some(vec![]));
    let selected: BulkWorkDownloadCommandRequest =
        serde_json::from_value(json!({"workIds": [" RJ000001 ", "RJ000001", "RJ000002"]})).unwrap();
    assert_eq!(
        selected.selected_work_ids().unwrap(),
        Some(vec!["RJ000001".into(), "RJ000002".into()])
    );
    let invalid: BulkWorkDownloadCommandRequest =
        serde_json::from_value(json!({"workIds": ["bad\u{0}id"]})).unwrap();
    assert!(invalid.selected_work_ids().is_err());
}

#[test]
fn maker_key_contract_is_shared_by_list_bulk_and_facet_dtos() {
    let request = json!({ "makerKeys": ["id:RG1"] });
    let list: ListProductsRequest = serde_json::from_value(request.clone()).unwrap();
    let bulk: BulkWorkDownloadCommandRequest = serde_json::from_value(request).unwrap();
    assert_eq!(list.into_query().unwrap().maker_keys, vec!["id:RG1"]);
    assert_eq!(bulk.to_query().unwrap().maker_keys, vec!["id:RG1"]);
    let facet = ProductMakerFacetDto::from(ProductMakerFacet {
        key: "id:RG1".into(),
        name: "制作元".into(),
        count: 2,
    });
    assert_eq!(
        serde_json::to_value(facet).unwrap(),
        json!({ "key": "id:RG1", "name": "制作元", "count": 2 })
    );
}

#[test]
fn downloaded_gallery_scope_defaults_off_and_is_preserved_in_list_queries() {
    for (request, expected) in [
        (json!({}), false),
        (json!({"downloadedOnly": false}), false),
        (json!({"downloadedOnly": true}), true),
    ] {
        let request: ListProductsRequest = serde_json::from_value(request).unwrap();
        assert_eq!(request.into_query().unwrap().downloaded_only, expected);
    }
}

#[test]
fn save_account_command_defaults_new_accounts_to_enabled() {
    let request = SaveAccountCommandRequest {
        id: None,
        label: "New account".to_owned(),
        login_name: Some("user@example.test".to_owned()),
        password: Some("secret".to_owned()),
    }
    .into_library_request()
    .expect("valid new account");

    assert!(request.id.is_none());
    assert!(request.enabled);
    assert!(request.remember_password);
}

#[test]
fn work_download_progress_throttle_limits_steady_download_updates() {
    let start = Instant::now();
    let mut throttle = WorkDownloadProgressThrottle::default();

    assert_eq!(
        throttle.emit_decision_at(dldr_download::DownloadPhase::Downloading, start),
        Some(true)
    );
    assert_eq!(
        throttle.emit_decision_at(
            dldr_download::DownloadPhase::Downloading,
            start + WORK_DOWNLOAD_PROGRESS_EVENT_INTERVAL - Duration::from_millis(1),
        ),
        None
    );
    assert_eq!(
        throttle.emit_decision_at(
            dldr_download::DownloadPhase::Downloading,
            start + WORK_DOWNLOAD_PROGRESS_EVENT_INTERVAL,
        ),
        Some(false)
    );
}

#[test]
fn work_download_progress_throttle_keeps_phase_changes_immediate() {
    let start = Instant::now();
    let mut throttle = WorkDownloadProgressThrottle::default();

    assert_eq!(
        throttle.emit_decision_at(dldr_download::DownloadPhase::ProbingMetadata, start),
        Some(true)
    );
    assert_eq!(
        throttle.emit_decision_at(
            dldr_download::DownloadPhase::Downloading,
            start + Duration::from_millis(1),
        ),
        Some(true)
    );
    assert_eq!(
        throttle.emit_decision_at(
            dldr_download::DownloadPhase::Finalizing,
            start + Duration::from_millis(2),
        ),
        Some(true)
    );
}

#[test]
fn path_root_check_allows_descendant_paths() {
    let root = PathBuf::from("/Users/example/Downloads/dldr-desktop/library");
    let path = root.join("RJ01488944");

    assert!(path_is_under_any_root(&path, &[root]));
}

#[test]
fn path_root_check_rejects_sibling_prefix_paths() {
    let root = PathBuf::from("/Users/example/Downloads/dldr-desktop/library");
    let path = PathBuf::from("/Users/example/Downloads/dldr-desktop/library-other/RJ01488944");

    assert!(!path_is_under_any_root(&path, &[root]));
}

#[test]
fn download_reservations_claim_skip_and_release_by_owner() {
    let reservations = DownloadReservations::default();
    let first_owner = reservations.next_owner_id();
    let second_owner = reservations.next_owner_id();
    let first = reservations.claim_available(
        &["RJ000001".to_owned(), "RJ000002".to_owned()],
        &first_owner,
    );
    let second = reservations.claim_available(
        &["RJ000002".to_owned(), "RJ000003".to_owned()],
        &second_owner,
    );

    assert_eq!(
        first.available,
        vec!["RJ000001".to_owned(), "RJ000002".to_owned()]
    );
    assert!(first.reserved.is_empty());
    assert_eq!(second.available, vec!["RJ000003".to_owned()]);
    assert_eq!(second.reserved, vec!["RJ000002".to_owned()]);
    assert!(reservations.is_reserved("RJ000001"));
    assert!(reservations.is_reserved("RJ000002"));
    assert!(reservations.is_reserved("RJ000003"));

    assert_eq!(reservations.release_owner(&first_owner), 2);
    assert!(!reservations.is_reserved("RJ000001"));
    assert!(!reservations.is_reserved("RJ000002"));
    assert!(reservations.is_reserved("RJ000003"));
}

#[test]
fn download_reservations_split_available_deduplicates_inputs() {
    let reservations = DownloadReservations::default();
    let owner = reservations.next_owner_id();
    reservations.claim_available(&["RJ000001".to_owned()], &owner);

    let split = reservations.split_available(&[
        "RJ000001".to_owned(),
        "RJ000001".to_owned(),
        "RJ000002".to_owned(),
        "RJ000002".to_owned(),
    ]);

    assert_eq!(split.reserved, vec!["RJ000001".to_owned()]);
    assert_eq!(split.available, vec!["RJ000002".to_owned()]);
}

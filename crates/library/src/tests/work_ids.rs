use super::*;

#[test]
fn detects_work_ids_in_flexible_folder_names() {
    assert_eq!(
        detect_work_ids_in_text("[RJ01005844] Soothing Voice")
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        vec!["RJ01005844"]
    );
    assert_eq!(
        detect_work_ids_in_text("title rj123456 extra")
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        vec!["RJ123456"]
    );
    assert_eq!(
        detect_work_ids_in_text("commercial VJ01001165 and book BJ123456")
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        vec!["VJ01001165", "BJ123456"]
    );
}

#[test]
fn work_id_detection_requires_boundaries_and_known_lengths() {
    for value in [
        "XRJ123456",
        "RJ123456A",
        "RJ12345",
        "RJ123456789",
        "AB123456",
    ] {
        assert_eq!(detect_work_ids_in_text(value), Vec::<WorkId>::new());
    }
}

#[test]
fn work_id_detection_deduplicates_in_first_seen_order() {
    assert_eq!(
        detect_work_ids_in_text("RJ000001 / rj000001 / VJ000001")
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        vec!["RJ000001", "VJ000001"]
    );
}

#[test]
fn detects_unique_work_id_from_path_file_name() {
    let path = Path::new("/library/[RJ01005844] Soothing Voice");

    assert_eq!(
        detect_unique_work_id_in_path(path).map(|id| id.to_string()),
        Some("RJ01005844".to_owned())
    );
    assert_eq!(
        detect_work_ids_in_path("/library/no-id"),
        Vec::<WorkId>::new()
    );
    assert_eq!(
        detect_unique_work_id_in_path("/library/RJ000001 VJ000001"),
        None
    );
}

#[test]
fn download_path_root_check_rejects_root_and_sibling_prefixes() {
    let root = PathBuf::from("/tmp/dlsite/library");

    assert!(path_is_download_child_of_any_root(
        &root.join("RJ000001"),
        std::slice::from_ref(&root)
    ));
    assert!(!path_is_download_child_of_any_root(
        &root,
        std::slice::from_ref(&root)
    ));
    assert!(!path_is_download_child_of_any_root(
        &PathBuf::from("/tmp/dlsite/library-other/RJ000001"),
        &[root]
    ));
}

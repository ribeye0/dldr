//! Work IDs found in user input and local folder names.
use dldr_api::WorkId;
use std::{collections::BTreeSet, path::Path};

pub const DL_SITE_WORK_ID_PREFIXES: &[&str] = &["RJ", "VJ", "BJ"];
const WORK_ID_MIN_DIGITS: usize = 6;
const WORK_ID_MAX_DIGITS: usize = 8;

pub fn detect_work_ids_in_text(text: &str) -> Vec<WorkId> {
    let text = text.to_ascii_uppercase();
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut seen = BTreeSet::new();
    let mut detected = Vec::new();

    while index + 2 + WORK_ID_MIN_DIGITS <= bytes.len() {
        let Some(prefix) = work_id_prefix_at(bytes, index) else {
            index += 1;
            continue;
        };

        if !is_work_id_boundary_before(bytes, index) {
            index += 1;
            continue;
        }

        let digit_start = index + prefix.len();
        let mut digit_end = digit_start;

        while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
            digit_end += 1;
        }

        let digit_count = digit_end.saturating_sub(digit_start);
        let is_valid = (WORK_ID_MIN_DIGITS..=WORK_ID_MAX_DIGITS).contains(&digit_count)
            && is_work_id_boundary_after(bytes, digit_end);

        if is_valid {
            let digits =
                std::str::from_utf8(&bytes[digit_start..digit_end]).expect("digits are ascii");
            let work_id = format!("{prefix}{digits}");

            if seen.insert(work_id.clone()) {
                detected.push(WorkId::from(work_id));
            }

            index = digit_end;
        } else {
            index += 1;
        }
    }

    detected
}

pub fn detect_work_ids_in_path(path: impl AsRef<Path>) -> Vec<WorkId> {
    path.as_ref()
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .map(detect_work_ids_in_text)
        .unwrap_or_default()
}

pub fn detect_unique_work_id_in_path(path: impl AsRef<Path>) -> Option<WorkId> {
    let mut detected = detect_work_ids_in_path(path);

    if detected.len() == 1 {
        detected.pop()
    } else {
        None
    }
}

fn work_id_prefix_at(bytes: &[u8], index: usize) -> Option<&'static str> {
    DL_SITE_WORK_ID_PREFIXES.iter().copied().find(|prefix| {
        bytes
            .get(index..index + prefix.len())
            .is_some_and(|candidate| candidate == prefix.as_bytes())
    })
}

fn is_work_id_boundary_before(bytes: &[u8], index: usize) -> bool {
    index == 0 || !bytes[index - 1].is_ascii_alphanumeric()
}

fn is_work_id_boundary_after(bytes: &[u8], index: usize) -> bool {
    index == bytes.len() || !bytes[index].is_ascii_alphanumeric()
}

//! Cached JSON projections for product cards and detail views.
use crate::models::*;
use crate::{
    genres, names::preferred_creator_name, LOCAL_PRODUCT_OWNER_ID, LOCAL_PRODUCT_OWNER_LABEL,
    MISSING_WORK_DETAIL_STATUS,
};
use serde::Deserialize;
use std::collections::BTreeMap;

pub(crate) fn local_product_owner() -> ProductOwner {
    ProductOwner {
        account_id: LOCAL_PRODUCT_OWNER_ID.to_owned(),
        label: LOCAL_PRODUCT_OWNER_LABEL.to_owned(),
        purchased_at: None,
    }
}

pub(crate) fn cached_work_needs_local_metadata_hydration(
    work_id: &str,
    title: &str,
    maker_name: Option<&str>,
    raw_json: &str,
) -> bool {
    let parsed = serde_json::from_str::<serde_json::Value>(raw_json).ok();
    let detail_status = parsed
        .as_ref()
        .and_then(|value| value.get("detail_status"))
        .and_then(serde_json::Value::as_str);
    let source = parsed
        .as_ref()
        .and_then(|value| value.get("source"))
        .and_then(serde_json::Value::as_str);

    matches!(
        detail_status,
        Some("local_only") | Some(MISSING_WORK_DETAIL_STATUS)
    ) || source == Some("local_scan")
        || (maker_name.is_none() && title.trim().eq_ignore_ascii_case(work_id))
}

pub(crate) fn product_credit_groups_from_raw_json(raw_json: &str) -> Vec<ProductCreditGroup> {
    let Ok(work) = serde_json::from_str::<RawWorkCredits>(raw_json) else {
        return Vec::new();
    };

    let mut groups = credit_group_templates();

    let mut tags = work
        .tags
        .into_iter()
        .chain(work.public_credit_tags)
        .filter_map(|tag| Some((tag.class, preferred_creator_name(&tag.name)?)))
        .collect::<Vec<_>>();
    tags.sort_by(|(left_class, left_name), (right_class, right_name)| {
        credit_sort_key(left_class, left_name).cmp(&credit_sort_key(right_class, right_name))
    });

    for (class, name) in tags {
        let Some((kind, _label, _rank)) = credit_kind_label_and_rank(&class) else {
            continue;
        };
        let name = name.trim();

        if name.is_empty() {
            continue;
        }

        if let Some((_kind, _label, names)) = groups
            .iter_mut()
            .find(|(group_kind, _, _)| *group_kind == kind)
        {
            if !names.iter().any(|existing| existing.as_str() == name) {
                names.push(name.to_owned());
            }
        }
    }

    groups
        .into_iter()
        .filter_map(|(kind, label, names)| {
            if names.is_empty() {
                None
            } else {
                Some(ProductCreditGroup {
                    kind: kind.to_owned(),
                    label: label.to_owned(),
                    names,
                })
            }
        })
        .collect()
}

pub(crate) fn product_content_size_from_raw_json(raw_json: &str) -> Option<u64> {
    let value = serde_json::from_str::<serde_json::Value>(raw_json).ok()?;
    value
        .get("content_size")
        .or_else(|| value.get("contents_file_size"))
        .and_then(json_value_as_u64)
}

pub(crate) fn product_text_values_from_json(raw_json: &str) -> Vec<ProductTextValue> {
    let Ok(values) = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(raw_json) else {
        return Vec::new();
    };

    values
        .into_iter()
        .filter_map(|(language, value)| {
            let value = value.as_str()?.trim();

            if value.is_empty() {
                None
            } else {
                Some(ProductTextValue {
                    language,
                    value: value.to_owned(),
                })
            }
        })
        .collect()
}

pub(crate) fn product_tags_from_raw_json(raw_json: &str) -> Vec<ProductTag> {
    let Ok(work) = serde_json::from_str::<RawWorkTags>(raw_json) else {
        return Vec::new();
    };

    let genre_ids = serde_json::from_str(raw_json)
        .map(|value| genres::ids_from_json(&value))
        .unwrap_or_default();
    let mut tags = work
        .tags
        .into_iter()
        .chain(work.public_credit_tags)
        .filter_map(|tag| {
            let class = tag.class.trim();
            let name = preferred_creator_name(&tag.name)?;
            let name = name.as_str();

            if class.is_empty() || name.is_empty() || (class == "genre" && !genre_ids.is_empty()) {
                None
            } else {
                Some(ProductTag {
                    class: class.to_owned(),
                    name: if class == "genre" {
                        genres::localized_name(name)?
                    } else {
                        name.to_owned()
                    },
                    genre_id: (class == "genre")
                        .then(|| genres::id_for_name(name))
                        .flatten(),
                })
            }
        })
        .collect::<Vec<_>>();

    tags.extend(genre_ids.into_iter().map(|id| ProductTag {
        class: "genre".into(),
        name: genres::name_for_id(id),
        genre_id: Some(id),
    }));

    tags.sort_by(|left, right| {
        left.class
            .cmp(&right.class)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then(left.genre_id.cmp(&right.genre_id))
    });
    tags.dedup();
    tags
}

fn json_value_as_u64(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Number(number) => number.as_u64(),
        serde_json::Value::String(value) => value.parse::<u64>().ok(),
        _ => None,
    }
}

fn credit_group_templates() -> Vec<(&'static str, &'static str, Vec<String>)> {
    vec![
        ("voice", "CV", Vec::new()),
        ("illust", "Illust", Vec::new()),
        ("scenario", "Scenario", Vec::new()),
        ("creator", "Creator", Vec::new()),
        ("series", "Series", Vec::new()),
        ("music", "Music", Vec::new()),
        ("other", "Other", Vec::new()),
    ]
}

fn credit_sort_key(class: &str, name: &str) -> (u8, String) {
    let rank = credit_kind_label_and_rank(class)
        .map(|(_kind, _label, rank)| rank)
        .unwrap_or(u8::MAX);

    (rank, name.trim().to_lowercase())
}

fn credit_kind_label_and_rank(class: &str) -> Option<(&'static str, &'static str, u8)> {
    match class {
        "voice_by" => Some(("voice", "CV", 0)),
        "illust_by" => Some(("illust", "Illust", 1)),
        "scenario_by" => Some(("scenario", "Scenario", 2)),
        "created_by" => Some(("creator", "Creator", 3)),
        "music_by" => Some(("music", "Music", 4)),
        "other_by" => Some(("other", "Other", 5)),
        "series" => Some(("series", "Series", 7)),
        _ if class.ends_with("_by") => Some(("other", "Other", 6)),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
struct RawWorkCredits {
    #[serde(default)]
    tags: Vec<RawWorkCreditTag>,
    #[serde(default)]
    public_credit_tags: Vec<RawWorkCreditTag>,
}

#[derive(Debug, Deserialize)]
struct RawWorkTags {
    #[serde(default)]
    tags: Vec<RawWorkCreditTag>,
    #[serde(default)]
    public_credit_tags: Vec<RawWorkCreditTag>,
}

#[derive(Debug, Deserialize)]
struct RawWorkCreditTag {
    #[serde(rename = "class")]
    class: String,
    #[serde(rename = "name")]
    name: serde_json::Value,
}

use super::*;

pub(super) fn build_storage_sync(
    account_id: &str,
    sync_run_id: &str,
    started_at: &str,
    completed_at: &str,
    purchases: Vec<Purchase>,
    works: Vec<Work>,
) -> Result<AccountSyncBuild> {
    let works_by_id = works
        .into_iter()
        .map(|work| (work.id.as_ref().to_owned(), work))
        .collect::<BTreeMap<_, _>>();
    let mut storage_works = Vec::with_capacity(purchases.len());
    let mut account_works = Vec::with_capacity(purchases.len());
    let mut missing_detail_count = 0;

    for purchase in purchases {
        let work_id = purchase.id.as_ref().to_owned();

        if let Some(work) = works_by_id.get(&work_id) {
            storage_works.push(cached_work_from_api(work.clone(), completed_at)?);
        } else {
            missing_detail_count += 1;
            storage_works.push(cached_work_from_purchase_placeholder(
                &purchase,
                completed_at,
            )?);
        }

        account_works.push(AccountWork {
            work_id,
            purchased_at: Some(datetime_to_string(purchase.purchased_at)),
        });
    }

    Ok(AccountSyncBuild {
        commit: AccountSyncCommit {
            sync_run_id: sync_run_id.to_owned(),
            account_id: account_id.to_owned(),
            started_at: started_at.to_owned(),
            completed_at: completed_at.to_owned(),
            works: storage_works,
            account_works,
        },
        missing_detail_count,
    })
}

pub(super) struct AccountSyncBuild {
    pub(super) commit: AccountSyncCommit,
    pub(super) missing_detail_count: usize,
}

pub(super) fn cached_work_from_api(work: Work, synced_at: &str) -> Result<CachedWork> {
    let age_category = serde_json::to_value(&work.age_category)?
        .as_str()
        .map(str::to_owned);

    Ok(CachedWork {
        work_id: work.id.as_ref().to_owned(),
        title_json: serde_json::to_string(&work.name)?,
        maker_id: Some(work.maker.id.clone()),
        maker_json: Some(serde_json::to_string(&work.maker.name)?),
        work_type: Some(work.work_kind.code.clone()),
        age_category,
        thumbnail_url: Some(work.thumbnail.full.to_string()),
        registered_at: work.registered_at.map(datetime_to_string),
        published_at: work.published_at.map(datetime_to_string),
        updated_at: work.updated_at.map(datetime_to_string),
        raw_json: serde_json::to_string(&work)?,
        last_detail_sync_at: synced_at.to_owned(),
    })
}

pub(super) fn cached_work_from_public_api(work: PublicWork, synced_at: &str) -> Result<CachedWork> {
    let work_id = work.id.as_ref().to_owned();
    let title = work
        .name
        .as_deref()
        .and_then(non_empty_trimmed)
        .unwrap_or(work_id.as_str())
        .to_owned();
    let title_json = localized_unspecified_json(&title)?;
    let maker_name = work
        .maker_name
        .as_deref()
        .and_then(non_empty_trimmed)
        .map(str::to_owned);
    let maker_json = maker_name
        .as_deref()
        .map(localized_unspecified_json)
        .transpose()?;
    let content_size = public_work_content_size(&work);

    Ok(CachedWork {
        work_id,
        title_json,
        maker_id: work
            .maker_id
            .as_deref()
            .and_then(non_empty_trimmed)
            .map(str::to_owned),
        maker_json,
        work_type: work
            .work_kind
            .as_deref()
            .and_then(non_empty_trimmed)
            .map(str::to_owned),
        age_category: public_work_age_category(&work),
        thumbnail_url: public_work_thumbnail_url(&work),
        registered_at: work
            .registered_at
            .as_deref()
            .and_then(normalize_public_datetime),
        published_at: work
            .published_at
            .as_deref()
            .or(work.registered_at.as_deref())
            .and_then(normalize_public_datetime),
        updated_at: work
            .updated_at
            .as_deref()
            .and_then(normalize_public_datetime),
        raw_json: public_work_raw_json(&work, content_size)?,
        last_detail_sync_at: synced_at.to_owned(),
    })
}

pub(super) fn cached_work_from_purchase_placeholder(
    purchase: &Purchase,
    synced_at: &str,
) -> Result<CachedWork> {
    let work_id = purchase.id.as_ref().to_owned();
    let raw_json = serde_json::to_string(&serde_json::json!({
        "workno": work_id,
        "source": "content/sales",
        "detail_status": "missing_from_content_works",
        "sales_date": datetime_to_string(purchase.purchased_at),
    }))?;

    Ok(CachedWork {
        work_id: work_id.clone(),
        title_json: serde_json::to_string(&serde_json::json!({ "und": work_id }))?,
        maker_id: None,
        maker_json: None,
        work_type: None,
        age_category: None,
        thumbnail_url: None,
        registered_at: None,
        published_at: None,
        updated_at: None,
        raw_json,
        last_detail_sync_at: synced_at.to_owned(),
    })
}

pub(super) fn cached_work_from_local_folder(
    work_id: &str,
    folder_name: &str,
    scanned_at: &str,
) -> Result<CachedWork> {
    let title = folder_name.trim();
    let title = if title.is_empty() { work_id } else { title };
    let raw_json = serde_json::to_string(&serde_json::json!({
        "workno": work_id,
        "source": "local_scan",
        "detail_status": "local_only",
        "folder_name": folder_name,
    }))?;

    Ok(CachedWork {
        work_id: work_id.to_owned(),
        title_json: serde_json::to_string(&serde_json::json!({ "und": title }))?,
        maker_id: None,
        maker_json: None,
        work_type: None,
        age_category: None,
        thumbnail_url: None,
        registered_at: None,
        published_at: None,
        updated_at: None,
        raw_json,
        last_detail_sync_at: scanned_at.to_owned(),
    })
}

pub(super) fn localized_unspecified_json(value: &str) -> Result<String> {
    let text = BTreeMap::from([("und", value)]);

    Ok(serde_json::to_string(&text)?)
}

pub(super) fn non_empty_trimmed(value: &str) -> Option<&str> {
    let value = value.trim();

    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub(super) fn public_work_age_category(work: &PublicWork) -> Option<String> {
    public_age_category_value(work.age_category.as_ref())
        .or_else(|| public_age_category_label(work.age_category_label.as_deref()))
}

pub(super) fn public_age_category_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Number(number) => match number.as_u64()? {
            1 => Some("all".to_owned()),
            2 => Some("r15".to_owned()),
            3 => Some("r18".to_owned()),
            _ => None,
        },
        Value::String(value) => public_age_category_label(Some(value)),
        _ => None,
    }
}

pub(super) fn public_age_category_label(value: Option<&str>) -> Option<String> {
    let normalized = value?
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "");

    if normalized.is_empty() {
        None
    } else if normalized.contains("adult") || normalized.contains("r18") || normalized == "18" {
        Some("r18".to_owned())
    } else if normalized.contains("r15") || normalized == "15" {
        Some("r15".to_owned())
    } else if normalized.contains("general") || normalized.contains("all") {
        Some("all".to_owned())
    } else {
        None
    }
}

pub(super) fn public_work_content_size(work: &PublicWork) -> Option<u64> {
    work.content_size.as_ref().and_then(json_value_as_u64)
}

pub(super) fn public_work_thumbnail_url(work: &PublicWork) -> Option<String> {
    work.image_main_url()
        .or(work.image_thumb.as_deref())
        .and_then(normalize_public_url)
}

pub(super) fn normalize_public_url(value: &str) -> Option<String> {
    let value = value.trim();

    if value.is_empty() {
        None
    } else if value.starts_with("//") {
        Some(format!("https:{value}"))
    } else if value.starts_with("http://") || value.starts_with("https://") {
        Some(value.to_owned())
    } else if value.starts_with('/') {
        Some(format!("https://www.dlsite.com{value}"))
    } else {
        Some(format!("https://img.dlsite.jp/{value}"))
    }
}

pub(super) fn normalize_public_datetime(value: &str) -> Option<String> {
    let value = value.trim();

    if value.is_empty() {
        return None;
    }

    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Some(datetime_to_string(value.with_timezone(&Utc)));
    }

    if let Ok(value) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Some(datetime_to_string(
            DateTime::<Utc>::from_naive_utc_and_offset(value, Utc),
        ));
    }

    if let Ok(value) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let value = value.and_hms_opt(0, 0, 0)?;
        return Some(datetime_to_string(
            DateTime::<Utc>::from_naive_utc_and_offset(value, Utc),
        ));
    }

    Some(value.to_owned())
}

pub(super) fn public_work_raw_json(work: &PublicWork, content_size: Option<u64>) -> Result<String> {
    let mut value = serde_json::to_value(work)?;
    let tags = public_work_tags(&value);

    if let Value::Object(map) = &mut value {
        map.insert("source".to_owned(), json!("public_product"));

        if let Some(content_size) = content_size {
            map.insert("content_size".to_owned(), json!(content_size));
        }

        if !tags.is_empty() {
            map.insert("tags".to_owned(), Value::Array(tags));
        }
    }

    Ok(serde_json::to_string(&value)?)
}

pub(super) fn public_work_tags(value: &Value) -> Vec<Value> {
    let mut tags = BTreeSet::<(String, String)>::new();

    if let Some(existing_tags) = value.get("tags").and_then(Value::as_array) {
        for tag in existing_tags {
            if let (Some(class), Some(name)) = (
                tag.get("class").and_then(Value::as_str),
                tag.get("name").and_then(Value::as_str),
            ) {
                push_public_tag(&mut tags, class, name);
            }
        }
    }

    if let Some(genres) = value.get("genres").and_then(Value::as_array) {
        for genre in genres {
            let name = genre
                .get("name")
                .or_else(|| genre.get("name_base"))
                .and_then(Value::as_str);

            if let Some(name) = name {
                push_public_tag(&mut tags, "genre", name);
            }
        }
    }

    if let Some(creators) = value.get("creaters").and_then(Value::as_object) {
        for (class, values) in creators {
            push_public_credit_values(&mut tags, class, values);
        }
    }

    for class in [
        "voice_by",
        "illust_by",
        "scenario_by",
        "created_by",
        "music_by",
        "other_by",
    ] {
        if let Some(values) = value.get(class) {
            push_public_credit_values(&mut tags, class, values);
        }
    }

    if let Some(name) = value.get("series_name").and_then(Value::as_str) {
        push_public_tag(&mut tags, "series", name);
    }
    tags.into_iter()
        .map(|(class, name)| json!({ "class": class, "name": name }))
        .collect()
}

pub(super) fn push_public_credit_values(
    tags: &mut BTreeSet<(String, String)>,
    fallback_class: &str,
    value: &Value,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                push_public_credit_values(tags, fallback_class, value);
            }
        }
        Value::Object(map) => {
            if let Some(name) = map.get("name").and_then(Value::as_str) {
                let class = map
                    .get("classification")
                    .and_then(Value::as_str)
                    .unwrap_or(fallback_class);
                push_public_tag(tags, class, name);
            } else {
                for value in map.values() {
                    push_public_credit_values(tags, fallback_class, value);
                }
            }
        }
        Value::String(name) => push_public_tag(tags, fallback_class, name),
        _ => {}
    }
}

pub(super) fn push_public_tag(tags: &mut BTreeSet<(String, String)>, class: &str, name: &str) {
    let class = class.trim();
    let name = name.trim();

    if !class.is_empty() && !name.is_empty() {
        tags.insert((class.to_owned(), name.to_owned()));
    }
}

pub(super) fn json_value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(value) => value.parse::<u64>().ok(),
        _ => None,
    }
}

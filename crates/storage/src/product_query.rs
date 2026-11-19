//! SQL predicates and ordering shared by list, counts, facets and search.
use crate::models::*;
use crate::names::push_valid_name_map;
use crate::{genres, search, Result, MISSING_WORK_DETAIL_STATUS};
use sqlx::{QueryBuilder, Sqlite};

pub(crate) fn push_product_visibility_filter(
    builder: &mut QueryBuilder<Sqlite>,
    query: &ProductListQuery,
) {
    let account_ids = product_account_ids(query);

    builder.push(" AND (");
    push_product_owned_condition(builder, &account_ids);

    if account_ids.is_empty() {
        builder.push(" OR ");
        push_product_has_download_condition(builder);
    }

    builder.push(")");
}

pub(crate) fn push_product_filters(
    builder: &mut QueryBuilder<Sqlite>,
    query: &ProductListQuery,
) -> Result<()> {
    push_product_filters_internal(builder, query, true)
}

pub(crate) fn push_product_filters_internal(
    builder: &mut QueryBuilder<Sqlite>,
    query: &ProductListQuery,
    include_maker_filter: bool,
) -> Result<()> {
    if let Some(id) = &query.playlist_id {
        builder.push(" AND EXISTS(SELECT 1 FROM playlist_works pw WHERE pw.work_id=w.work_id AND pw.playlist_id=");
        builder.push_bind(id);
        builder.push(")");
    }
    if query.downloaded_only {
        builder.push(
            " AND EXISTS (
                SELECT 1 FROM work_downloads downloaded_wd
                WHERE downloaded_wd.work_id = w.work_id
                    AND downloaded_wd.status = 'downloaded'
                    AND NULLIF(trim(downloaded_wd.local_path), '') IS NOT NULL
            )",
        );
    }

    builder.push(
        " AND COALESCE(
            CASE
                WHEN json_valid(w.raw_json) THEN json_extract(w.raw_json, '$.detail_status')
            END,
            ''
        ) <> ",
    );
    builder.push_bind(MISSING_WORK_DETAIL_STATUS);

    let age_categories = product_age_categories(query);
    if !age_categories.is_empty() {
        builder.push(" AND w.age_category IN (");
        for (index, age_category) in age_categories.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            builder.push_bind(age_category.as_storage_value());
        }
        builder.push(")");
    }

    let type_groups = product_type_groups(query);
    if !type_groups.is_empty() {
        builder.push(" AND ");
        builder.push(product_type_group_case_sql());
        builder.push(" IN (");
        for (index, type_group) in type_groups.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            builder.push_bind(type_group.as_storage_value());
        }
        builder.push(")");
    }

    let source_groups = product_source_groups(query);
    if !source_groups.is_empty() && source_groups.len() < 2 {
        builder.push(" AND (");
        for (index, source_group) in source_groups.iter().enumerate() {
            if index > 0 {
                builder.push(" OR ");
            }

            match source_group {
                ProductSourceGroup::Owned => push_product_owned_condition(builder, &[]),
                ProductSourceGroup::LocalOnly => push_product_local_only_condition(builder),
            }
        }
        builder.push(")");
    }

    let maker_keys = product_maker_keys(query);
    if include_maker_filter && !maker_keys.is_empty() {
        builder.push(" AND w.maker_key IN (");
        for (index, maker_key) in maker_keys.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            builder.push_bind((*maker_key).to_owned());
        }
        builder.push(")");
    }

    for name in &query.genre_names {
        builder.push(" AND (");
        genres::push_exact_filter(builder, name);
        builder.push(")");
    }
    for name in &query.excluded_genre_names {
        builder.push(" AND NOT (");
        genres::push_exact_filter(builder, name);
        builder.push(")");
    }
    if !query.voice_names.is_empty() {
        builder.push(" AND (");
        for (i, name) in query.voice_names.iter().enumerate() {
            if i > 0 {
                builder.push(" OR ");
            }
            push_voice_filter(builder, name);
        }
        builder.push(")");
    }
    for name in &query.excluded_voice_names {
        builder.push(" AND NOT (");
        push_voice_filter(builder, name);
        builder.push(")");
    }

    search::push_filters(builder, query.search.as_deref().unwrap_or_default())
}

pub(crate) fn push_text_filter(builder: &mut QueryBuilder<Sqlite>, search: &str) {
    let pattern = format!("%{}%", escape_like(search));
    builder.push(
        " AND (
                w.work_id LIKE ",
    );
    builder.push_bind(pattern.clone());
    builder.push(" ESCAPE '\\'");
    for column in ["w.title_json", "w.maker_json"] {
        builder.push(" OR EXISTS (SELECT 1 FROM json_each(");
        push_valid_name_map(builder, column);
        builder.push(
            ") AS name WHERE name.type = 'text' AND trim(name.value) <> '' AND name.value LIKE ",
        );
        builder.push_bind(pattern.clone());
        builder.push(" ESCAPE '\\')");
    }
    builder.push(
        " OR EXISTS (
                    SELECT 1
                    FROM (SELECT value FROM json_each(
                        CASE
                            WHEN json_valid(w.raw_json) THEN w.raw_json
                            ELSE '{\"tags\":[]}'
                        END,
                        '$.tags'
                    ) UNION ALL SELECT value FROM json_each(
                        CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,
                        '$.public_credit_tags'
                    )) AS tag
                    WHERE (json_type(tag.value, '$.name') = 'text' AND json_extract(tag.value, '$.name') LIKE ",
    );
    builder.push_bind(pattern.clone());
    builder.push(" ESCAPE '\\') OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_type(tag.value, '$.name') = 'object' THEN json_extract(tag.value, '$.name') ELSE '{}' END) AS credit_name WHERE credit_name.type = 'text' AND credit_name.value LIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" ESCAPE '\\'))");

    genres::push_search(builder, search);

    if search_matches_local_only_source(search) {
        builder.push(" OR ");
        push_product_local_only_condition(builder);
    }

    builder.push(")");
}

fn push_product_owned_condition(builder: &mut QueryBuilder<Sqlite>, account_ids: &[&str]) {
    builder.push(
        "EXISTS (
            SELECT 1
            FROM account_works source_aw
            JOIN accounts source_a ON source_a.id = source_aw.account_id
            WHERE source_aw.work_id = w.work_id
                AND source_aw.is_current = 1
                AND source_a.enabled = 1",
    );

    if !account_ids.is_empty() {
        builder.push(" AND source_aw.account_id IN (");
        for (index, account_id) in account_ids.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            builder.push_bind((*account_id).to_owned());
        }
        builder.push(")");
    }

    builder.push(")");
}

pub(crate) fn push_voice_filter(builder: &mut QueryBuilder<Sqlite>, name: &str) {
    builder.push("EXISTS (SELECT 1 FROM (SELECT value FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,'$.tags') UNION ALL SELECT value FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,'$.public_credit_tags')) AS credit WHERE json_extract(credit.value,'$.class')='voice_by' AND ((json_type(credit.value,'$.name')='text' AND trim(json_extract(credit.value,'$.name')) = ");
    builder.push_bind(name.to_owned());
    builder.push(" COLLATE NOCASE) OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_type(credit.value,'$.name')='object' THEN json_extract(credit.value,'$.name') ELSE '{}' END) AS alias WHERE alias.type='text' AND trim(alias.value) = ");
    builder.push_bind(name.to_owned());
    builder.push(" COLLATE NOCASE)))");
}

fn push_product_has_download_condition(builder: &mut QueryBuilder<Sqlite>) {
    builder.push(
        "EXISTS (
            SELECT 1
            FROM work_downloads source_wd
            WHERE source_wd.work_id = w.work_id
                AND (source_wd.status = 'downloaded' OR (source_wd.status = 'failed' AND source_wd.error_code = 'local_missing'))
        )",
    );
}

fn push_product_local_only_condition(builder: &mut QueryBuilder<Sqlite>) {
    builder.push("(NOT ");
    push_product_owned_condition(builder, &[]);
    builder.push(" AND ");
    push_product_has_download_condition(builder);
    builder.push(")");
}

fn product_account_ids(query: &ProductListQuery) -> Vec<&str> {
    let mut ids = Vec::new();

    if let Some(account_id) = query.account_id.as_deref() {
        push_unique_str(&mut ids, account_id);
    }

    for account_id in &query.account_ids {
        push_unique_str(&mut ids, account_id);
    }

    ids
}

fn product_age_categories(query: &ProductListQuery) -> Vec<ProductAgeCategory> {
    let mut values = Vec::new();

    if let Some(age_category) = query.age_category {
        push_unique_copy(&mut values, age_category);
    }

    for age_category in &query.age_categories {
        push_unique_copy(&mut values, *age_category);
    }

    values
}

fn product_type_groups(query: &ProductListQuery) -> Vec<ProductTypeGroup> {
    let mut values = Vec::new();

    if let Some(type_group) = query.type_group {
        push_unique_copy(&mut values, type_group);
    }

    for type_group in &query.type_groups {
        push_unique_copy(&mut values, *type_group);
    }

    values
}

fn product_source_groups(query: &ProductListQuery) -> Vec<ProductSourceGroup> {
    let mut values = Vec::new();

    for source_group in &query.source_groups {
        push_unique_copy(&mut values, *source_group);
    }

    values
}

fn product_maker_keys(query: &ProductListQuery) -> Vec<&str> {
    let mut names = Vec::new();

    for maker_name in &query.maker_keys {
        let maker_name = maker_name.trim();
        if !maker_name.is_empty() {
            push_unique_str(&mut names, maker_name);
        }
    }

    names
}

fn search_matches_local_only_source(search: &str) -> bool {
    let normalized = search.trim().to_lowercase().replace(['_', '-'], " ");

    normalized.contains("local")
        || normalized.contains("not owned")
        || normalized.contains("notowned")
        || normalized.contains("unowned")
}

fn push_unique_str<'a>(values: &mut Vec<&'a str>, value: &'a str) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn push_unique_copy<T>(values: &mut Vec<T>, value: T)
where
    T: Copy + PartialEq,
{
    if !values.contains(&value) {
        values.push(value);
    }
}

fn product_type_group_case_sql() -> &'static str {
    "CASE
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%vcm%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%voicecomic%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%vcomic%'
            THEN 'image'
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%sou%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%amt%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%mus%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%audio%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%voice%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%asmr%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%music%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%sound%'
            THEN 'audio'
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%mov%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%movie%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%video%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%anime%'
            THEN 'video'
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%gam%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%acn%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%adv%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%etc%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%pzl%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%qiz%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%sln%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%tbl%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%typ%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%game%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%rpg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%adv%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%action%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%acn%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%puzzle%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%puz%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%quiz%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%simulation%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%slg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%shooter%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%stg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%tabletop%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%typing%'
            THEN 'game'
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%cg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%adl%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%doh%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%dnv%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%icg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%imt%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%mng%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%scm%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%nre%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%ksv%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%icg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%image%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%illust%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%comic%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%com%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%manga%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%mng%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%gekiga%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%pdf%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%novel%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%digitalnovel%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%book%'
            THEN 'image'
        ELSE 'other'
    END"
}

pub(crate) fn push_product_sort(builder: &mut QueryBuilder<Sqlite>, sort: ProductSort) {
    match sort {
        ProductSort::TitleAsc => {
            builder.push("sort_title ASC, w.work_id ASC");
        }
        ProductSort::LatestPurchaseDesc => {
            builder.push("latest_purchased_at DESC, sort_title ASC, w.work_id ASC");
        }
        ProductSort::PublishedAtDesc => {
            builder.push("sort_published_at DESC, sort_title ASC, w.work_id ASC");
        }
    }
}

pub(crate) fn push_outer_product_sort(builder: &mut QueryBuilder<Sqlite>, sort: ProductSort) {
    match sort {
        ProductSort::TitleAsc => {
            builder.push("vw.sort_title ASC, w.work_id ASC");
        }
        ProductSort::LatestPurchaseDesc => {
            builder.push("vw.latest_purchased_at DESC, vw.sort_title ASC, w.work_id ASC");
        }
        ProductSort::PublishedAtDesc => {
            builder.push("vw.sort_published_at DESC, vw.sort_title ASC, w.work_id ASC");
        }
    }
}

fn escape_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }

    escaped
}

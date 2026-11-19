use crate::models::{
    ProductDetail, ProductFilterFacets, ProductListItem, ProductListPage, ProductListQuery,
    ProductMakerFacet, ProductNameFacet, ProductOwner,
};
use crate::names::push_valid_name_map;
use crate::names::{preferred_name, product_query};
use crate::product_metadata::{
    cached_work_needs_local_metadata_hydration, local_product_owner,
    product_content_size_from_raw_json, product_credit_groups_from_raw_json,
    product_tags_from_raw_json, product_text_values_from_json,
};
use crate::product_query::{
    push_outer_product_sort, push_product_filters, push_product_filters_internal,
    push_product_sort, push_product_visibility_filter,
};
use crate::rows::{ensure_changed, work_download_state_from_product_row};
use crate::{LanguagePriority, Result, Storage, StorageError, MISSING_WORK_DETAIL_STATUS};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::{collections::BTreeMap, collections::BTreeSet};

impl Storage {
    pub async fn list_products(&self, query: &ProductListQuery) -> Result<ProductListPage> {
        let total_count = self.count_products(query).await?;
        let products = self.fetch_product_page(query).await?;

        Ok(ProductListPage {
            total_count,
            products,
        })
    }

    pub async fn available_name_languages(&self) -> Result<Vec<String>> {
        let mut builder = QueryBuilder::<Sqlite>::new("");
        for (index, column) in ["w.title_json", "w.maker_json"].iter().enumerate() {
            if index > 0 {
                builder.push(" UNION ");
            }
            builder.push("SELECT name.key AS code FROM works w, json_each(");
            push_valid_name_map(&mut builder, column);
            builder.push(") AS name WHERE name.type = 'text' AND trim(name.value) <> '' AND trim(name.key) <> ''");
        }
        builder.push(" ORDER BY code COLLATE BINARY");
        Ok(builder.build_query_scalar().fetch_all(&self.pool).await?)
    }

    pub async fn product_filter_facets(
        &self,
        query: &ProductListQuery,
    ) -> Result<ProductFilterFacets> {
        let mut builder = product_query(&query.language_priority);
        builder.push(
            ", matching_makers AS (
                SELECT w.work_id, w.maker_key, w.maker_json
                FROM named_works w WHERE w.maker_key IS NOT NULL",
        );
        push_product_visibility_filter(&mut builder, query);
        push_product_filters_internal(&mut builder, query, false)?;
        builder.push(
            "), maker_counts AS (
                SELECT maker_key, COUNT(DISTINCT work_id) AS work_count
                FROM matching_makers GROUP BY maker_key
            ), maker_labels AS (
                SELECT w.maker_key, trim(name.value) AS maker_label,
                    ROW_NUMBER() OVER (
                        PARTITION BY w.maker_key
                        ORDER BY priority.rank IS NULL, priority.rank,
                            name.key COLLATE BINARY, trim(name.value) COLLATE BINARY
                    ) AS label_rank
                FROM matching_makers w JOIN json_each(",
        );
        push_valid_name_map(&mut builder, "w.maker_json");
        builder.push(
            ") AS name LEFT JOIN creator_name_priority priority ON priority.code = name.key
                WHERE name.type = 'text' AND trim(name.value) <> ''
            )
            SELECT counts.maker_key, labels.maker_label, counts.work_count
            FROM maker_counts counts
            JOIN maker_labels labels ON labels.maker_key = counts.maker_key AND labels.label_rank = 1
            ORDER BY counts.work_count DESC, lower(labels.maker_label) ASC,
                labels.maker_label COLLATE BINARY, counts.maker_key COLLATE BINARY",
        );

        let rows = builder.build().fetch_all(&self.pool).await?;
        let makers = rows
            .into_iter()
            .map(|row| {
                let count: i64 = row.try_get("work_count")?;
                Ok(ProductMakerFacet {
                    key: row.try_get("maker_key")?,
                    name: row.try_get("maker_label")?,
                    count: count.max(0) as u64,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let (genres, voices) = self.metadata_facets(query).await?;
        Ok(ProductFilterFacets {
            genres,
            voices,
            makers,
        })
    }

    async fn metadata_facets(
        &self,
        query: &ProductListQuery,
    ) -> Result<(Vec<ProductNameFacet>, Vec<ProductNameFacet>)> {
        let mut query = query.clone();
        query.genre_names.clear();
        query.excluded_genre_names.clear();
        query.voice_names.clear();
        query.excluded_voice_names.clear();
        let mut builder = product_query(&query.language_priority);
        builder.push("SELECT w.raw_json FROM named_works w WHERE 1=1");
        push_product_visibility_filter(&mut builder, &query);
        push_product_filters_internal(&mut builder, &query, true)?;
        let rows: Vec<String> = builder.build_query_scalar().fetch_all(&self.pool).await?;
        let mut genres = BTreeMap::<(String, Option<i64>), u64>::new();
        let mut voices = BTreeMap::<(String, Option<i64>), u64>::new();
        for raw in rows {
            let names = product_tags_from_raw_json(&raw)
                .into_iter()
                .filter(|t| t.class == "genre")
                .map(|t| (t.name, t.genre_id))
                .collect::<BTreeSet<_>>();
            for name in names {
                *genres.entry(name).or_default() += 1;
            }
            let names = product_credit_groups_from_raw_json(&raw)
                .into_iter()
                .filter(|g| g.kind == "voice")
                .flat_map(|g| g.names)
                .collect::<BTreeSet<_>>();
            for name in names {
                *voices.entry((name, None)).or_default() += 1;
            }
        }
        let facets = |map: BTreeMap<(String, Option<i64>), u64>| {
            map.into_iter()
                .map(|((name, genre_id), count)| ProductNameFacet {
                    name,
                    count,
                    genre_id,
                })
                .collect::<Vec<_>>()
        };
        Ok((facets(genres), facets(voices)))
    }

    pub async fn product_detail(
        &self,
        work_id: &str,
        priority: &LanguagePriority,
    ) -> Result<ProductDetail> {
        let mut builder = product_query(priority);
        builder.push(
            "SELECT
                w.work_id,
                w.title,
                w.title_json,
                w.maker_id,
                w.maker_name,
                w.maker_json,
                w.work_type,
                w.age_category,
                w.thumbnail_url,
                w.registered_at,
                w.published_at,
                w.updated_at,
                w.raw_json,
                w.last_detail_sync_at,
                (
                    SELECT MIN(owned_aw.purchased_at)
                    FROM account_works owned_aw
                    JOIN accounts owned_a ON owned_a.id = owned_aw.account_id
                    WHERE owned_aw.work_id = w.work_id
                        AND owned_aw.is_current = 1
                        AND owned_a.enabled = 1
                ) AS earliest_purchased_at,
                (
                    SELECT MAX(owned_aw.purchased_at)
                    FROM account_works owned_aw
                    JOIN accounts owned_a ON owned_a.id = owned_aw.account_id
                    WHERE owned_aw.work_id = w.work_id
                        AND owned_aw.is_current = 1
                        AND owned_a.enabled = 1
                ) AS latest_purchased_at,
                wd.status AS download_status,
                wd.local_path AS download_local_path,
                wd.staging_path AS download_staging_path,
                wd.unpack_policy AS download_unpack_policy,
                wd.bytes_received AS download_bytes_received,
                wd.bytes_total AS download_bytes_total,
                wd.error_code AS download_error_code,
                wd.error_message AS download_error_message,
                wd.started_at AS download_started_at,
                wd.completed_at AS download_completed_at,
                wd.updated_at AS download_updated_at
             FROM named_works w
             LEFT JOIN work_downloads wd ON wd.work_id = w.work_id
             WHERE w.work_id = ",
        );
        builder.push_bind(work_id.to_owned());
        builder.push(
            "
                AND COALESCE(
                    CASE
                        WHEN json_valid(w.raw_json) THEN json_extract(w.raw_json, '$.detail_status')
                    END,
                    ''
                ) <> ",
        );
        builder.push_bind(MISSING_WORK_DETAIL_STATUS);
        let row = builder
            .build()
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StorageError::NotFound {
                entity: "product detail",
                id: work_id.to_owned(),
            })?;

        let raw_json: String = row.try_get("raw_json")?;
        let download = work_download_state_from_product_row(&row)?;
        let mut owners = self.product_owners(work_id).await?;

        if owners.is_empty() {
            owners.push(local_product_owner());
        }

        Ok(ProductDetail {
            work_id: row.try_get("work_id")?,
            title: row.try_get("title")?,
            title_variants: product_text_values_from_json(&row.try_get::<String, _>("title_json")?),
            maker_id: row.try_get("maker_id")?,
            maker_name: row.try_get("maker_name")?,
            maker_names: row
                .try_get::<Option<String>, _>("maker_json")?
                .as_deref()
                .map(product_text_values_from_json)
                .unwrap_or_default(),
            work_type: row.try_get("work_type")?,
            age_category: row.try_get("age_category")?,
            thumbnail_url: row.try_get("thumbnail_url")?,
            content_size_bytes: product_content_size_from_raw_json(&raw_json),
            registered_at: row.try_get("registered_at")?,
            published_at: row.try_get("published_at")?,
            updated_at: row.try_get("updated_at")?,
            last_detail_sync_at: row.try_get("last_detail_sync_at")?,
            earliest_purchased_at: row.try_get("earliest_purchased_at")?,
            latest_purchased_at: row.try_get("latest_purchased_at")?,
            credit_groups: product_credit_groups_from_raw_json(&raw_json),
            tags: product_tags_from_raw_json(&raw_json),
            download,
            owners,
        })
    }

    async fn product_owners(&self, work_id: &str) -> Result<Vec<ProductOwner>> {
        let rows = sqlx::query(
            "SELECT a.id, a.label, aw.purchased_at
             FROM account_works aw
             JOIN accounts a ON a.id = aw.account_id
             WHERE aw.work_id = ?1
                AND aw.is_current = 1
                AND a.enabled = 1
             ORDER BY lower(a.label) ASC, a.id ASC",
        )
        .bind(work_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(ProductOwner {
                    account_id: row.try_get("id")?,
                    label: row.try_get("label")?,
                    purchased_at: row.try_get("purchased_at")?,
                })
            })
            .collect()
    }

    async fn count_products(&self, query: &ProductListQuery) -> Result<u64> {
        let mut builder = product_query(&query.language_priority);
        builder.push(
            "SELECT COUNT(*) AS count FROM (
                 SELECT w.work_id
                 FROM named_works w
                 WHERE 1 = 1",
        );

        push_product_visibility_filter(&mut builder, query);
        push_product_filters(&mut builder, query)?;
        builder.push(" GROUP BY w.work_id)");

        let row = builder.build().fetch_one(&self.pool).await?;
        let count: i64 = row.try_get("count")?;

        Ok(count as u64)
    }

    async fn fetch_product_page(&self, query: &ProductListQuery) -> Result<Vec<ProductListItem>> {
        let mut builder = product_query(&query.language_priority);
        builder.push(
            ", visible_works AS (
                 SELECT
                    w.work_id,
                    lower(w.title) AS sort_title,
                    COALESCE(w.published_at, '') AS sort_published_at,
                    (
                        SELECT MIN(owned_aw.purchased_at)
                        FROM account_works owned_aw
                        JOIN accounts owned_a ON owned_a.id = owned_aw.account_id
                        WHERE owned_aw.work_id = w.work_id
                            AND owned_aw.is_current = 1
                            AND owned_a.enabled = 1
                    ) AS earliest_purchased_at,
                    (
                        SELECT MAX(owned_aw.purchased_at)
                        FROM account_works owned_aw
                        JOIN accounts owned_a ON owned_a.id = owned_aw.account_id
                        WHERE owned_aw.work_id = w.work_id
                            AND owned_aw.is_current = 1
                            AND owned_a.enabled = 1
                    ) AS latest_purchased_at
                 FROM named_works w
                 WHERE 1 = 1",
        );

        push_product_visibility_filter(&mut builder, query);
        push_product_filters(&mut builder, query)?;
        builder.push(" GROUP BY w.work_id ORDER BY ");
        push_product_sort(&mut builder, query.sort);
        builder.push(" LIMIT ");
        builder.push_bind(i64::from(query.limit));
        builder.push(" OFFSET ");
        builder.push_bind(i64::from(query.offset));
        builder.push(
            ")
             SELECT
                w.work_id,
                w.title,
                w.maker_name,
                w.work_type,
                w.age_category,
                w.thumbnail_url,
                w.published_at,
                w.updated_at,
                w.raw_json,
                vw.earliest_purchased_at,
                vw.latest_purchased_at,
                wd.status AS download_status,
                wd.local_path AS download_local_path,
                wd.staging_path AS download_staging_path,
                wd.unpack_policy AS download_unpack_policy,
                wd.bytes_received AS download_bytes_received,
                wd.bytes_total AS download_bytes_total,
                wd.error_code AS download_error_code,
                wd.error_message AS download_error_message,
                wd.started_at AS download_started_at,
                wd.completed_at AS download_completed_at,
                wd.updated_at AS download_updated_at,
                a.id AS account_id,
                a.label AS account_label,
                aw.purchased_at
             FROM visible_works vw
             JOIN named_works w ON w.work_id = vw.work_id
             LEFT JOIN work_downloads wd ON wd.work_id = w.work_id
             LEFT JOIN account_works aw ON aw.work_id = w.work_id AND aw.is_current = 1
             LEFT JOIN accounts a ON a.id = aw.account_id AND a.enabled = 1
             WHERE a.id IS NOT NULL OR NOT EXISTS (
                SELECT 1
                FROM account_works visible_aw
                JOIN accounts visible_a ON visible_a.id = visible_aw.account_id
                WHERE visible_aw.work_id = w.work_id
                    AND visible_aw.is_current = 1
                    AND visible_a.enabled = 1
             )
             ORDER BY ",
        );
        push_outer_product_sort(&mut builder, query.sort);
        builder.push(", lower(a.label) ASC, aw.account_id ASC");

        let rows = builder.build().fetch_all(&self.pool).await?;
        let mut products = Vec::<ProductListItem>::new();

        for row in rows {
            let work_id: String = row.try_get("work_id")?;
            let raw_json: String = row.try_get("raw_json")?;
            let owner = row
                .try_get::<Option<String>, _>("account_id")?
                .map(|account_id| {
                    Ok::<_, StorageError>(ProductOwner {
                        account_id,
                        label: row.try_get("account_label")?,
                        purchased_at: row.try_get("purchased_at")?,
                    })
                })
                .transpose()?;

            if let Some(product) = products
                .last_mut()
                .filter(|product| product.work_id == work_id)
            {
                if let Some(owner) = owner {
                    product.owners.push(owner);
                }
                continue;
            }

            products.push(ProductListItem {
                work_id,
                title: row.try_get("title")?,
                maker_name: row.try_get("maker_name")?,
                work_type: row.try_get("work_type")?,
                age_category: row.try_get("age_category")?,
                thumbnail_url: row.try_get("thumbnail_url")?,
                content_size_bytes: product_content_size_from_raw_json(&raw_json),
                published_at: row.try_get("published_at")?,
                updated_at: row.try_get("updated_at")?,
                earliest_purchased_at: row.try_get("earliest_purchased_at")?,
                latest_purchased_at: row.try_get("latest_purchased_at")?,
                credit_groups: product_credit_groups_from_raw_json(&raw_json),
                tags: product_tags_from_raw_json(&raw_json),
                download: work_download_state_from_product_row(&row)?,
                owners: owner
                    .map(|owner| vec![owner])
                    .unwrap_or_else(|| vec![local_product_owner()]),
            });
        }

        Ok(products)
    }
}

impl Storage {
    pub async fn work_needs_local_metadata_hydration(&self, work_id: &str) -> Result<bool> {
        let row = sqlx::query(
            "SELECT title_json, maker_json, raw_json
             FROM works
             WHERE work_id = ?1",
        )
        .bind(work_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(true);
        };

        let title_json: String = row.try_get("title_json")?;
        let maker_json: Option<String> = row.try_get("maker_json")?;
        let title = preferred_name(Some(&title_json)).unwrap_or_else(|| work_id.to_owned());
        let maker_name = preferred_name(maker_json.as_deref());
        let raw_json: String = row.try_get("raw_json")?;

        Ok(cached_work_needs_local_metadata_hydration(
            work_id,
            &title,
            maker_name.as_deref(),
            &raw_json,
        ))
    }

    pub async fn product_credits_checked_at(&self, work_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT CASE WHEN json_valid(raw_json)
                THEN CASE WHEN json_extract(raw_json, '$.public_credits_version') = 2
                    THEN json_extract(raw_json, '$.public_credits_checked_at') END
                END FROM works WHERE work_id = ?1",
        )
        .bind(work_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    /// Supplement credits only; never replace purchase metadata or download state.
    pub async fn cache_public_product_credits(
        &self,
        work_id: &str,
        tags: &[serde_json::Value],
        checked_at: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE works SET raw_json = json_set(raw_json,
                '$.public_credit_tags', json(?2), '$.public_credits_checked_at', ?3, '$.public_credits_version', 2)
             WHERE work_id = ?1 AND json_valid(raw_json)",
        )
        .bind(work_id)
        .bind(serde_json::to_string(tags).expect("JSON tags serialize"))
        .bind(checked_at)
        .execute(&self.pool)
        .await?;
        ensure_changed(result, "product metadata", work_id)
    }
}

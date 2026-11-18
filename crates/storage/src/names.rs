use crate::{Result, StorageError, UiLocale};
use sqlx::{QueryBuilder, Sqlite};

pub const DEFAULT_LANGUAGE_PRIORITY: [&str; 4] = ["zh_TW", "zh_CN", "ja_JP", "en_US"];
// Maker filter identities must remain stable when display defaults change.
const MAKER_IDENTITY_PRIORITY: [&str; 5] = ["en_US", "ja_JP", "ko_KR", "zh_TW", "zh_CN"];
const CREATOR_LANGUAGE_PRIORITY: [&str; 5] = ["ja_JP", "zh_TW", "zh_CN", "en_US", "ko_KR"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguagePriority(Vec<String>);

impl Default for LanguagePriority {
    fn default() -> Self {
        Self::for_locale(UiLocale::default())
    }
}

impl LanguagePriority {
    pub fn for_locale(locale: UiLocale) -> Self {
        let codes: &[&str] = match locale {
            UiLocale::TraditionalChinese => &DEFAULT_LANGUAGE_PRIORITY,
            UiLocale::SimplifiedChinese => &["zh_CN", "zh_TW", "ja_JP", "en_US"],
            UiLocale::Japanese => &["ja_JP", "zh_TW", "zh_CN", "en_US"],
            UiLocale::English => &["en_US", "ja_JP", "zh_TW", "zh_CN"],
        };
        Self(codes.iter().map(|code| (*code).to_owned()).collect())
    }

    pub(super) fn is_legacy_default(&self) -> bool {
        self.codes()
            .iter()
            .map(String::as_str)
            .eq(MAKER_IDENTITY_PRIORITY)
    }

    pub fn new(codes: Vec<String>) -> Result<Self> {
        let mut seen = std::collections::BTreeSet::new();
        for code in &codes {
            if code.trim().is_empty() {
                return Err(StorageError::InvalidLanguagePriority(
                    "language code must not be blank",
                ));
            }
            if !seen.insert(code) {
                return Err(StorageError::InvalidLanguagePriority(
                    "language codes must be unique",
                ));
            }
        }
        Ok(Self(codes))
    }

    /// Settings expose four supported title languages; all other names remain
    /// deterministic fallbacks. Read legacy custom codes without rewriting data.
    pub(super) fn for_settings(self, locale: UiLocale) -> Self {
        let mut codes: Vec<String> = self
            .0
            .into_iter()
            .filter(|code| DEFAULT_LANGUAGE_PRIORITY.contains(&code.as_str()))
            .collect();
        if codes.is_empty() {
            return Self::for_locale(locale);
        }
        // Complete old custom subsets consistently, independently of UI changes.
        for code in DEFAULT_LANGUAGE_PRIORITY {
            if !codes.iter().any(|existing| existing == code) {
                codes.push(code.to_owned());
            }
        }
        Self(codes)
    }

    pub fn codes(&self) -> &[String] {
        &self.0
    }

    pub fn into_codes(self) -> Vec<String> {
        self.0
    }

    pub(super) fn to_json(&self) -> String {
        serde_json::to_string(&self.0).expect("strings serialize as JSON")
    }

    pub(super) fn from_stored(value: &str) -> Result<Self> {
        serde_json::from_str(value)
            .ok()
            .and_then(|codes| Self::new(codes).ok())
            .ok_or_else(|| StorageError::InvalidStoredValue {
                field: "app_settings.language_priority",
                value: value.to_owned(),
            })
    }
}

/// Computed name columns share the same selection rule across filters, sorting,
/// pagination and detail queries. The underlying works table stores only maps.
pub(super) fn product_query(priority: &LanguagePriority) -> QueryBuilder<Sqlite> {
    let mut builder = QueryBuilder::new(
        "WITH name_priority AS (
            SELECT value AS code, CAST(key AS INTEGER) AS rank FROM json_each(",
    );
    builder.push_bind(priority.to_json());
    builder.push(")), maker_identity_priority AS (SELECT value AS code, CAST(key AS INTEGER) AS rank FROM json_each(");
    builder.push_bind(
        serde_json::to_string(&MAKER_IDENTITY_PRIORITY).expect("language codes serialize"),
    );
    builder.push(")), creator_name_priority AS (SELECT value AS code, CAST(key AS INTEGER) AS rank FROM json_each(");
    builder.push_bind(
        serde_json::to_string(&CREATOR_LANGUAGE_PRIORITY).expect("language codes serialize"),
    );
    builder.push(")), named_works AS (SELECT works.*, COALESCE(");
    push_preferred_name(&mut builder, "works.title_json", "name_priority");
    builder.push(", works.work_id) AS title, ");
    push_preferred_name(&mut builder, "works.maker_json", "creator_name_priority");
    builder.push(" AS maker_name, CASE WHEN trim(COALESCE(works.maker_id, '')) <> '' THEN 'id:' || trim(works.maker_id) ELSE 'name:' || ");
    push_preferred_name(&mut builder, "works.maker_json", "maker_identity_priority");
    builder.push(" END AS maker_key FROM works) ");
    builder
}

fn push_preferred_name(builder: &mut QueryBuilder<Sqlite>, column: &str, priority_table: &str) {
    builder.push("(SELECT trim(name.value) FROM json_each(");
    push_valid_name_map(builder, column);
    builder
        .push(") AS name LEFT JOIN ")
        .push(priority_table)
        .push(" AS priority ON priority.code = name.key");
    builder.push(
        "
         WHERE name.type = 'text' AND trim(name.value) <> ''
         ORDER BY priority.rank IS NULL, priority.rank, name.key COLLATE BINARY
         LIMIT 1)",
    );
}

pub(super) fn push_valid_name_map(builder: &mut QueryBuilder<Sqlite>, column: &str) {
    // Only trusted column names from this module's callers are SQL text.
    builder
        .push("CASE WHEN json_valid(")
        .push(column)
        .push(") THEN CASE WHEN json_type(");
    builder.push(column).push(") = 'object' THEN ").push(column);
    builder.push(" ELSE '{}' END ELSE '{}' END");
}

pub(super) fn preferred_name(json: Option<&str>) -> Option<String> {
    preferred_name_with_priority(json, &LanguagePriority::default())
}

pub(super) fn preferred_name_with_priority(
    json: Option<&str>,
    priority: &LanguagePriority,
) -> Option<String> {
    let map: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_str(json?).ok()?;
    priority
        .codes()
        .iter()
        .filter_map(|code| map.get(code))
        .chain(map.values())
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .find(|name| !name.is_empty())
        .map(str::to_owned)
}

pub(super) fn preferred_creator_name(value: &serde_json::Value) -> Option<String> {
    let name = if let Some(name) = value.as_str() {
        Some(name)
    } else {
        let map = value.as_object()?;
        CREATOR_LANGUAGE_PRIORITY
            .into_iter()
            .filter_map(|code| map.get(code))
            .chain(map.values())
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .find(|name| !name.is_empty())
    }?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

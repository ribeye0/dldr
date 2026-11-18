//! Offline localized labels and search aliases for DLsite's stable genre IDs.
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};
use std::sync::LazyLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Genre {
    id: i64,
    zh_tw: String,
    zh_cn: String,
    en: String,
    ja: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
}

impl Genre {
    fn labels(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.zh_tw.as_str())
            .chain(self.ja.as_deref())
            .chain([self.zh_cn.as_str(), self.en.as_str()])
    }

    fn names(&self) -> impl Iterator<Item = &str> {
        self.labels().chain(self.aliases.iter().map(String::as_str))
    }
}

static GENRES: LazyLock<Vec<Genre>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../resources/genres.json"))
        .expect("bundled DLsite genre catalogue must be valid")
});

pub fn display_name(name: &str, locale: crate::UiLocale) -> String {
    if let Some(id) = id_for_name(name) {
        return display_name_for_id(id, locale);
    }
    if let Some(id) = unknown_id(name) {
        return display_name_for_id(id, locale);
    }
    name.to_owned()
}

pub fn display_name_for_id(id: i64, locale: crate::UiLocale) -> String {
    use crate::UiLocale;
    if let Some(genre) = GENRES.iter().find(|g| g.id == id) {
        return match locale {
            UiLocale::TraditionalChinese => &genre.zh_tw,
            UiLocale::SimplifiedChinese => &genre.zh_cn,
            UiLocale::Japanese => genre.ja.as_ref().unwrap_or(&genre.zh_tw),
            UiLocale::English => &genre.en,
        }
        .clone();
    }
    let prefix = match locale {
        UiLocale::TraditionalChinese => "分類",
        UiLocale::SimplifiedChinese => "分类",
        UiLocale::Japanese => "ジャンル",
        UiLocale::English => "Genre",
    };
    format!("{prefix} #{id}")
}

pub(super) fn id_for_name(name: &str) -> Option<i64> {
    let name = name.trim();
    // Stored tags use Traditional Chinese canonical names. Current labels win
    // over historical aliases, even when they belong to a different genre.
    GENRES
        .iter()
        .find(|g| g.zh_tw.eq_ignore_ascii_case(name))
        .or_else(|| {
            GENRES
                .iter()
                .find(|g| g.labels().any(|label| label.eq_ignore_ascii_case(name)))
        })
        .or_else(|| {
            GENRES
                .iter()
                .find(|g| g.names().any(|alias| alias.eq_ignore_ascii_case(name)))
        })
        .map(|g| g.id)
        .or_else(|| unknown_id(name))
}

pub(super) fn unknown_id(name: &str) -> Option<i64> {
    ["分類 #", "分类 #", "ジャンル #", "Genre #"]
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix).and_then(|v| v.parse().ok()))
}

pub(super) fn name_for_id(id: i64) -> String {
    GENRES
        .iter()
        .find(|genre| genre.id == id)
        .map(|genre| genre.zh_tw.clone())
        .unwrap_or_else(|| format!("分類 #{id}"))
}

pub(super) fn localized_name(name: &str) -> Option<String> {
    let name = name.trim();
    id_for_name(name)
        .map(name_for_id)
        // Preserve already-Chinese metadata without leaking untranslated kana/English.
        .or_else(|| {
            (name.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
                && !name.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)))
            .then(|| name.to_owned())
        })
}

pub(super) fn ids_from_json(value: &serde_json::Value) -> Vec<i64> {
    let mut ids = Vec::new();
    for id in value
        .get("genre_ids")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .chain(
            value
                .get("genres")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|genre| genre.get("id")),
        )
    {
        if let Some(id) = id
            .as_i64()
            .or_else(|| id.as_str()?.parse().ok())
            .filter(|id| *id > 0)
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

pub(super) fn push_search(builder: &mut QueryBuilder<Sqlite>, search: &str) {
    let query = search.to_lowercase();
    let matches = GENRES
        .iter()
        .filter(|genre| {
            genre
                .names()
                .any(|name| name.to_lowercase().contains(&query))
        })
        .collect::<Vec<_>>();
    let mut ids = matches.iter().map(|genre| genre.id).collect::<Vec<_>>();
    if let Some(id) = unknown_id(search) {
        ids.push(id);
    }
    if ids.is_empty() {
        return;
    }
    // Bound JSON arrays avoid per-ID parameter growth and never interpolate labels into SQL.
    let ids = serde_json::to_string(&ids).expect("genre IDs serialize");
    let names = matches
        .iter()
        .flat_map(|genre| genre.names())
        .collect::<Vec<_>>();
    let names = serde_json::to_string(&names).expect("genre names serialize");
    builder.push(" OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END, '$.genre_ids') AS genre_id WHERE CAST(genre_id.value AS INTEGER) IN (SELECT value FROM json_each(");
    builder.push_bind(ids.clone());
    builder.push("))) OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END, '$.genres') AS genre WHERE CAST(json_extract(genre.value, '$.id') AS INTEGER) IN (SELECT value FROM json_each(");
    builder.push_bind(ids);
    builder.push("))) OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END, '$.tags') AS genre_tag WHERE json_extract(genre_tag.value, '$.class') = 'genre' AND json_extract(genre_tag.value, '$.name') COLLATE NOCASE IN (SELECT value FROM json_each(");
    builder.push_bind(names);
    builder.push(")))");
}

pub(super) fn push_exact_filter(builder: &mut QueryBuilder<Sqlite>, name: &str) {
    let mut matches = GENRES
        .iter()
        .filter(|genre| genre.labels().any(|label| label.eq_ignore_ascii_case(name)))
        .collect::<Vec<_>>();
    if matches.is_empty() {
        matches = GENRES
            .iter()
            .filter(|genre| genre.names().any(|alias| alias.eq_ignore_ascii_case(name)))
            .collect::<Vec<_>>();
    }
    let mut ids = matches.iter().map(|g| g.id).collect::<Vec<_>>();
    if let Some(id) = unknown_id(name) {
        ids.push(id);
    }
    let mut names = vec![name];
    for genre in matches {
        names.extend(
            genre
                .names()
                .filter(|alias| id_for_name(alias).is_some_and(|id| ids.contains(&id))),
        );
    }
    let ids = serde_json::to_string(&ids).expect("genre IDs");
    let names = serde_json::to_string(&names).expect("genre names");
    builder.push("EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,'$.genre_ids') AS g WHERE CAST(g.value AS INTEGER) IN (SELECT value FROM json_each(");
    builder.push_bind(ids.clone());
    builder.push("))) OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,'$.genres') AS g WHERE CAST(json_extract(g.value,'$.id') AS INTEGER) IN (SELECT value FROM json_each(");
    builder.push_bind(ids);
    // Numeric metadata is authoritative. A historical name on an ID-backed row
    // must not turn a precise NTR condition into a different genre.
    builder.push("))) OR (NOT EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,'$.genre_ids') AS g WHERE CAST(g.value AS INTEGER) > 0) AND NOT EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,'$.genres') AS g WHERE CAST(json_extract(g.value,'$.id') AS INTEGER) > 0) AND EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,'$.tags') AS g WHERE json_extract(g.value,'$.class')='genre' AND json_extract(g.value,'$.name') COLLATE NOCASE IN (SELECT value FROM json_each(");
    builder.push_bind(names);
    builder.push("))))");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_labels_share_ids_across_regional_terminology_and_unknown_ids() {
        use crate::UiLocale::*;
        for alias in ["強勢女", "女性主导", "女性主導", "女性優位", "Femdom"] {
            assert_eq!(display_name(alias, TraditionalChinese), "強勢女");
            assert_eq!(display_name(alias, SimplifiedChinese), "女性主导");
            assert_eq!(display_name(alias, Japanese), "女性優位");
            assert_eq!(display_name(alias, English), "Femdom");
        }
        assert_eq!(display_name("分類 #999999", English), "Genre #999999");
        assert_eq!(unknown_id("ジャンル #999999"), Some(999999));
        assert_eq!(display_name("future name", Japanese), "future name");
        for genre in GENRES.iter() {
            assert!(!genre.zh_cn.is_empty());
            assert!(!genre.en.is_empty());
            assert!(genre.ja.as_ref().is_some_and(|name| !name.is_empty()));
        }
    }

    #[test]
    fn corrected_genre_labels_keep_old_search_aliases_and_meaningful_digits() {
        use crate::UiLocale::*;
        for alias in ["6 萝", "6萝", "6 つるぺた", "6 Petite", "萝"] {
            assert_eq!(display_name(alias, SimplifiedChinese), "萝");
            assert_eq!(display_name(alias, Japanese), "つるぺた");
            assert_eq!(display_name(alias, English), "Petite");
            assert_eq!(localized_name(alias).as_deref(), Some("酪梨"));
        }
        for alias in ["7 反向奸", "7反向奸", "7 逆レ", "7 Reverse R", "反向奸"] {
            assert_eq!(display_name(alias, SimplifiedChinese), "反向奸");
            assert_eq!(display_name(alias, Japanese), "逆レ");
            assert_eq!(display_name(alias, English), "Reverse R");
            assert_eq!(localized_name(alias).as_deref(), Some("強制你播種"));
        }
        assert_eq!(display_name("3D作品", SimplifiedChinese), "3D作品");
        assert_eq!(display_name("3D作品", English), "3D Works");
    }

    #[test]
    fn catalogue_resolves_ids_and_japanese_names_to_traditional_chinese() {
        let mut ids = std::collections::BTreeSet::new();
        for genre in GENRES.iter() {
            assert!(ids.insert(genre.id), "duplicate genre ID");
            assert!(!genre.zh_tw.trim().is_empty());
            if genre
                .zh_tw
                .chars()
                .any(|c| ('\u{3040}'..='\u{30ff}').contains(&c))
            {
                // These three TRPG names are still Japanese on the official
                // localized pages. Do not replace them with invented labels.
                assert!([516, 517, 518].contains(&genre.id));
                assert_eq!(genre.ja.as_deref(), Some(genre.zh_tw.as_str()));
            }
        }
        assert_eq!(name_for_id(503), "低語");
        assert_eq!(name_for_id(500), "舔耳");
        assert_eq!(localized_name("ささやき"), Some("低語".into()));
        assert_eq!(localized_name("耳舐め"), Some("舔耳".into()));
        assert_eq!(localized_name("untranslated genre"), None);
        assert_eq!(name_for_id(999999), "分類 #999999");
        assert_eq!(
            ids_from_json(&serde_json::json!({
                "genre_ids": [500, "503", null, 0, -3, {}],
                "genres": [{"id":"500"}, {"id":442}, {}]
            })),
            vec![500, 503, 442]
        );
    }
}

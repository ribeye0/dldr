//! Search syntax is enforced here so list, facets and bulk selection agree.
use super::*;

const MAX_QUERY_CHARS: usize = 4096;
const MAX_TERMS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Text(String),
    Circle(String),
    Voice(String),
    Genre(String),
    ExcludeGenre(String),
}

fn invalid(message: &'static str) -> StorageError {
    StorageError::InvalidSearch(message)
}

fn parse(input: &str) -> Result<Vec<Term>> {
    if input.chars().count() > MAX_QUERY_CHARS {
        return Err(invalid("搜尋內容最多 4096 字。"));
    }
    let mut rest = input.trim();
    if matches!(
        rest.to_ascii_lowercase().as_str(),
        "not owned" | "local only"
    ) || super::genres::unknown_id(rest).is_some()
    {
        return Ok(vec![Term::Text(rest.to_owned())]);
    }
    let mut terms = Vec::new();
    while !rest.is_empty() {
        if terms.len() >= MAX_TERMS {
            return Err(invalid("搜尋條件最多 64 個。"));
        }
        if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted
                .find('"')
                .ok_or_else(|| invalid("文字片語缺少結尾雙引號。"))?;
            let text = quoted[..end].trim();
            if text.is_empty() {
                return Err(invalid("文字片語不能為空。"));
            }
            terms.push(Term::Text(text.to_owned()));
            rest = quoted[end + 1..].trim_start();
            continue;
        }
        if let Some(operator) = rest.strip_prefix('$') {
            // Ordinary prices/dollar signs remain searchable as text. A colon
            // before whitespace or the next dollar starts an explicit operator.
            let head = operator
                .split(|c: char| c == '$' || c.is_whitespace())
                .next()
                .unwrap_or("");
            if head.contains(':') {
                let end = operator
                    .find('$')
                    .ok_or_else(|| invalid("精準條件缺少結尾 $，例如 $tag:NTR$。"))?;
                let (kind, value) = operator[..end].split_once(':').expect("operator colon");
                let value = value.trim();
                if value.is_empty() {
                    return Err(invalid("精準條件的名稱不能為空。"));
                }
                let value = value.to_owned();
                terms.push(match kind.to_ascii_lowercase().as_str() {
                    "circle" => Term::Circle(value),
                    "va" => Term::Voice(value),
                    "tag" => Term::Genre(value),
                    "-tag" => Term::ExcludeGenre(value),
                    _ => return Err(invalid("不支援此搜尋指令；可用 circle、va、tag、-tag。")),
                });
                rest = operator[end + 1..].trim_start();
                continue;
            }
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        terms.push(Term::Text(rest[..end].to_owned()));
        rest = rest[end..].trim_start();
    }
    Ok(terms)
}

pub(super) fn push_filters(builder: &mut QueryBuilder<Sqlite>, input: &str) -> Result<()> {
    for term in parse(input)? {
        match term {
            Term::Text(text) => push_text_filter(builder, &text),
            Term::Circle(name) => {
                builder.push(" AND EXISTS (SELECT 1 FROM json_each(");
                push_valid_name_map(builder, "w.maker_json");
                builder.push(
                    ") AS circle_name WHERE circle_name.type='text' AND trim(circle_name.value) = ",
                );
                builder.push_bind(name);
                builder.push(" COLLATE NOCASE)");
            }
            Term::Voice(name) => {
                builder.push(" AND (");
                push_voice_filter(builder, &name);
                builder.push(")");
            }
            Term::Genre(name) => {
                builder.push(" AND (");
                genres::push_exact_filter(builder, &name);
                builder.push(")");
            }
            Term::ExcludeGenre(name) => {
                builder.push(" AND NOT (");
                genres::push_exact_filter(builder, &name);
                builder.push(")");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_and_adjacent_conditions_without_losing_plain_words() {
        assert_eq!(
            parse(
                "NTR 青梅竹馬 $circle:バイコーンの森$$va:小花衣こっこ$ $tag:女性主导$ $-tag:舔耳$"
            )
            .unwrap(),
            vec![
                Term::Text("NTR".into()),
                Term::Text("青梅竹馬".into()),
                Term::Circle("バイコーンの森".into()),
                Term::Voice("小花衣こっこ".into()),
                Term::Genre("女性主导".into()),
                Term::ExcludeGenre("舔耳".into()),
            ]
        );
        assert_eq!(
            parse("\"A Long Evening\" $VA: Actor Name $ $5").unwrap(),
            vec![
                Term::Text("A Long Evening".into()),
                Term::Voice("Actor Name".into()),
                Term::Text("$5".into())
            ]
        );
    }

    #[test]
    fn rejects_malformed_unknown_and_unbounded_queries() {
        for input in [
            "$tag:$",
            "$tag:NTR",
            "$unknown:value$",
            "$tagw:NTR$",
            "$duration:60$",
            "\"unfinished",
            "\"\"",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
        assert!(parse(&"字".repeat(MAX_QUERY_CHARS + 1)).is_err());
        assert!(parse(&"a ".repeat(MAX_TERMS + 1)).is_err());
        assert!(parse("").unwrap().is_empty());
    }
}

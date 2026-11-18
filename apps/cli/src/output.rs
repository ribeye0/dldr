use crate::Result;
use dldr_storage::{ProductDetail, ProductListItem, WorkDownloadStatus};
use serde_json::{json, Value};
use std::io::Write;

pub fn clean(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
pub fn write(value: &str) -> Result<()> {
    std::io::stdout().lock().write_all(value.as_bytes())?;
    Ok(())
}
pub fn json(value: &Value) -> Result<()> {
    write(&format!(
        "{}\n",
        serde_json::to_string_pretty(value).expect("JSON value")
    ))
}
pub fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("—")
}
pub fn bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}
pub fn status(status: WorkDownloadStatus) -> &'static str {
    match status {
        WorkDownloadStatus::Downloaded => "downloaded",
        WorkDownloadStatus::Downloading => "downloading",
        WorkDownloadStatus::Failed => "failed",
        WorkDownloadStatus::Cancelled => "cancelled",
        WorkDownloadStatus::NotDownloaded => "not_downloaded",
    }
}
pub fn work(p: &ProductListItem, locale: dldr_storage::UiLocale) -> Value {
    json!({"work_id":p.work_id,"title":p.title,"circle":p.maker_name,"size_bytes":p.content_size_bytes,
        "voice_actors":p.credit_groups.iter().filter(|g|g.kind=="voice").flat_map(|g|g.names.iter()).collect::<Vec<_>>(),
        "tags":p.tags.iter().filter(|t|t.class=="genre").map(|t|genre_label(t, locale)).collect::<Vec<_>>(),
        "status":status(p.download.status),"local_path":p.download.local_path})
}
pub fn detail(p: &ProductDetail, locale: dldr_storage::UiLocale) -> Value {
    json!({"work_id":p.work_id,"title":p.title,"circle":p.maker_name,"size_bytes":p.content_size_bytes,
        "voice_actors":p.credit_groups.iter().filter(|g|g.kind=="voice").flat_map(|g|g.names.iter()).collect::<Vec<_>>(),
        "tags":p.tags.iter().filter(|t|t.class=="genre").map(|t|genre_label(t, locale)).collect::<Vec<_>>(),
        "status":status(p.download.status),"local_path":p.download.local_path})
}
fn genre_label(tag: &dldr_storage::ProductTag, locale: dldr_storage::UiLocale) -> String {
    tag.genre_id
        .map(|id| dldr_storage::genre_display_name_for_id(id, locale))
        .unwrap_or_else(|| dldr_storage::genre_display_name(&tag.name, locale))
}

pub fn works(works: &[Value]) -> Result<()> {
    if works.is_empty() {
        return write("No works found.\n");
    }
    write("ID           SIZE          STATUS          TITLE\n")?;
    for p in works {
        write(&format!(
            "{:<12} {:<13} {:<15} {}\n",
            string(p, "work_id"),
            p["size_bytes"]
                .as_u64()
                .map(bytes)
                .unwrap_or_else(|| "unknown".into()),
            string(p, "status"),
            clean(string(p, "title"))
        ))?;
    }
    Ok(())
}
pub fn show(p: &Value) -> Result<()> {
    write(&format!(
        "{}  {}\n",
        string(p, "work_id"),
        clean(string(p, "title"))
    ))?;
    for (label, key) in [
        ("Circle", "circle"),
        ("Voice", "voice_actors"),
        ("Tags", "tags"),
        ("Size", "size_bytes"),
        ("Status", "status"),
        ("Folder", "local_path"),
    ] {
        let value = match &p[key] {
            Value::Array(items) => items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            Value::Number(n) if key == "size_bytes" => n.as_u64().map(bytes).unwrap_or_default(),
            Value::String(s) => s.clone(),
            _ => "—".into(),
        };
        write(&format!("{label:<8} {}\n", clean(&value)))?;
    }
    Ok(())
}

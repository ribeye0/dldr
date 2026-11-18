use dldr_storage::AppSettings;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub(crate) fn normalize_path_setting(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_owned();

    if value.is_empty() {
        return Ok(None);
    }

    if value.contains('\0') {
        return Err("path contains a NUL byte".to_owned());
    }

    Ok(Some(value))
}

pub(crate) fn required_library_root(settings: &AppSettings) -> Result<PathBuf, String> {
    settings
        .library_root
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "Library folder is required".to_owned())
}

pub(crate) fn effective_download_root(
    app: &AppHandle,
    settings: &AppSettings,
) -> Result<PathBuf, String> {
    settings
        .download_root
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| app.path().download_dir().ok())
        .ok_or_else(|| "Download staging folder is required".to_owned())
}

pub(crate) fn canonicalize_existing_path(path: impl AsRef<Path>) -> Result<PathBuf, String> {
    let path = path.as_ref();

    path.canonicalize()
        .map_err(|error| format!("failed to resolve {}: {error}", path.display()))
}

pub(crate) fn canonical_open_roots(app: &AppHandle, settings: &AppSettings) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(root) = &settings.library_root {
        roots.push(PathBuf::from(root));
    }

    if let Some(root) = &settings.download_root {
        roots.push(PathBuf::from(root));
    } else if let Ok(download_dir) = app.path().download_dir() {
        roots.push(download_dir);
    }

    roots
        .into_iter()
        .filter_map(|root| canonicalize_existing_path(root).ok())
        .collect()
}

pub(crate) fn path_is_under_any_root(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

pub(crate) fn normalize_required_id(value: String) -> Result<String, String> {
    let value = value.trim().to_owned();

    if value.is_empty() {
        return Err("id is required".to_owned());
    }

    if value.contains('\0') {
        return Err("id contains a NUL byte".to_owned());
    }

    Ok(value)
}

pub(crate) fn normalize_required_path(value: String) -> Result<PathBuf, String> {
    let value = value.trim().to_owned();

    if value.is_empty() {
        return Err("path is required".to_owned());
    }

    if value.contains('\0') {
        return Err("path contains a NUL byte".to_owned());
    }

    Ok(PathBuf::from(value))
}

pub(crate) fn normalize_optional_id(value: Option<String>) -> Result<Option<String>, String> {
    normalize_optional_string(value).and_then(|value| match value {
        Some(value) => normalize_required_id(value).map(Some),
        None => Ok(None),
    })
}

pub(crate) fn normalize_optional_ids(values: Option<Vec<String>>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();

    for value in values.unwrap_or_default() {
        let Some(value) = normalize_optional_string(Some(value))? else {
            continue;
        };
        let value = normalize_required_id(value)?;
        if !normalized.contains(&value) {
            normalized.push(value);
        }
    }

    Ok(normalized)
}

pub(crate) fn normalize_label(value: String) -> Result<String, String> {
    let value = value.trim().to_owned();

    if value.is_empty() {
        return Err("label is required".to_owned());
    }

    if value.contains('\0') {
        return Err("label contains a NUL byte".to_owned());
    }

    Ok(value)
}

pub(crate) fn normalize_optional_string(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_owned();

    if value.is_empty() {
        return Ok(None);
    }

    if value.contains('\0') {
        return Err("value contains a NUL byte".to_owned());
    }

    Ok(Some(value))
}

pub(crate) fn normalize_optional_strings(
    values: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();

    for value in values.unwrap_or_default() {
        let Some(value) = normalize_optional_string(Some(value))? else {
            continue;
        };
        if !normalized.contains(&value) {
            normalized.push(value);
        }
    }

    Ok(normalized)
}

pub(crate) fn normalize_secret(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };

    if value.is_empty() {
        return Ok(None);
    }

    if value.contains('\0') {
        return Err("secret contains a NUL byte".to_owned());
    }

    Ok(Some(value))
}

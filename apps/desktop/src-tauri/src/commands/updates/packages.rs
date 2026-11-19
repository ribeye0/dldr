//! Package inspection is separate from installation. Never execute a selected
//! binary to discover its version. An explicitly supplied bad signature is fatal.
use super::{signed_version_matches, PackageKind, Release};
use base64::{engine::general_purpose::STANDARD, Engine};
use semver::Version;
use serde::Serialize;
use std::{fs::File, io::Read, path::Path, time::Duration};
use tauri_plugin_updater::Update;

const MAX_PACKAGE: u64 = 1024 * 1024 * 1024;
const MAX_METADATA: u64 = 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadyPackage {
    pub release: Release,
    pub file_name: String,
    pub size: u64,
    pub source: &'static str,
    pub direction: &'static str,
    pub verified: bool,
}

pub(super) struct PreparedPackage {
    pub transfer_id: String,
    pub kind: PackageKind,
    pub info: ReadyPackage,
    pub bytes: Vec<u8>,
    pub automatic: Option<Update>,
}

pub(super) fn info(
    current: &str,
    version: &str,
    file_name: String,
    size: u64,
    source: &'static str,
    verified: bool,
    notes: String,
) -> Result<ReadyPackage, String> {
    let target = Version::parse(version).map_err(|_| "update_version")?;
    let current_semver = Version::parse(current).map_err(|_| "update_version")?;
    let direction = match target.cmp(&current_semver) {
        std::cmp::Ordering::Greater => "upgrade",
        std::cmp::Ordering::Less => "downgrade",
        std::cmp::Ordering::Equal => "reinstall",
    };
    Ok(ReadyPackage {
        release: Release {
            current_version: current.into(),
            version: version.into(),
            notes,
        },
        file_name,
        size,
        source,
        direction,
        verified,
    })
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|_| "update_filesystem")?;
    if !file.metadata().map_err(|_| "update_filesystem")?.is_file() {
        return Err("update_package".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "update_filesystem")?;
    if bytes.len() as u64 > limit {
        return Err("update_package".into());
    }
    Ok(bytes)
}

pub(super) fn filename_version(name: &str, kind: PackageKind) -> Option<String> {
    let suffix = match kind {
        PackageKind::AppImage => "_amd64.AppImage",
        PackageKind::WindowsPortable => "_windows_x64.zip",
        PackageKind::WindowsInstaller => "_x64-setup.exe",
        PackageKind::Unsupported => return None,
    };
    let version = name.strip_prefix("DLDR_")?.strip_suffix(suffix)?;
    Version::parse(version).ok()?;
    Some(version.into())
}

fn validate_format(bytes: &[u8], path: &Path, kind: PackageKind) -> Result<(), String> {
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let valid = match kind {
        PackageKind::AppImage => {
            extension == "appimage"
                && bytes.starts_with(b"\x7fELF\x02\x01")
                && bytes.get(8..11) == Some(b"AI\x02")
                && bytes.get(18..20) == Some(&[62, 0])
        }
        PackageKind::WindowsPortable => extension == "zip" && bytes.starts_with(b"PK\x03\x04"),
        PackageKind::WindowsInstaller => {
            let offset = bytes
                .get(60..64)
                .map(|v| u32::from_le_bytes(v.try_into().unwrap()) as usize);
            extension == "exe"
                && bytes.starts_with(b"MZ")
                && offset.and_then(|o| bytes.get(o..o.saturating_add(4))) == Some(b"PE\0\0")
        }
        PackageKind::Unsupported => false,
    };
    if valid {
        Ok(())
    } else {
        Err("update_package".into())
    }
}

fn signature_from_feed(
    bytes: &[u8],
    version: &str,
    kind: PackageKind,
) -> Result<Option<String>, String> {
    let feed: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| "update_signature")?;
    if feed["version"].as_str().map(|s| s.trim_start_matches('v')) != Some(version) {
        return Ok(None);
    }
    let platform = match kind {
        PackageKind::AppImage => "linux-x86_64",
        PackageKind::WindowsPortable => "windows-portable-x86_64",
        PackageKind::WindowsInstaller => "windows-x86_64",
        PackageKind::Unsupported => return Ok(None),
    };
    feed["platforms"][platform]["signature"]
        .as_str()
        .map(|s| Some(s.into()))
        .ok_or("update_signature".into())
}

pub(super) fn verify(
    bytes: &[u8],
    signature: &str,
    pubkey: &str,
    version: &str,
) -> Result<(), String> {
    let decode = |value: &str| {
        STANDARD
            .decode(value)
            .ok()
            .and_then(|v| String::from_utf8(v).ok())
            .ok_or("update_signature".to_owned())
    };
    let key =
        minisign_verify::PublicKey::decode(&decode(pubkey)?).map_err(|_| "update_signature")?;
    let signature_text = decode(signature)?;
    let parsed =
        minisign_verify::Signature::decode(&signature_text).map_err(|_| "update_signature")?;
    key.verify(bytes, &parsed, true)
        .map_err(|_| "update_signature")?;
    if !signed_version_matches(signature, version) {
        return Err("update_signature".into());
    }
    Ok(())
}

fn local_verification(
    bytes: &[u8],
    signature: Option<&str>,
    explicit: bool,
    pubkey: &str,
    version: &str,
) -> Result<bool, String> {
    match signature {
        Some(signature) => match verify(bytes, signature, pubkey, version) {
            Ok(()) => Ok(true),
            Err(error) if explicit => Err(error),
            // A self-build may share a release version, but still needs user trust.
            Err(_) => Ok(false),
        },
        None => Ok(false),
    }
}

async fn official_signature(version: &str, kind: PackageKind) -> Result<Option<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "update_network")?;
    // Offline/unpublished builds can still be imported with explicit local trust.
    let Ok(mut response) = client
        .get(format!(
            "https://github.com/ribeye0/dldr/releases/download/v{version}/latest.json"
        ))
        .send()
        .await
    else {
        return Ok(None);
    };
    if !response.status().is_success() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "update_network")? {
        if bytes.len() + chunk.len() > MAX_METADATA as usize {
            return Err("update_signature".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    signature_from_feed(&bytes, version, kind)
}

pub(super) async fn prepare_local(
    id: String,
    path: String,
    hint: Option<String>,
    kind: PackageKind,
    current: String,
    pubkey: String,
) -> Result<PreparedPackage, String> {
    let (bytes, file_name, version, adjacent) = tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&path);
        let file_name = path
            .file_name()
            .ok_or("update_package")?
            .to_string_lossy()
            .into_owned();
        let bytes = read_limited(path, MAX_PACKAGE)?;
        validate_format(&bytes, path, kind)?;
        let named = filename_version(&file_name, kind);
        #[cfg(windows)]
        let embedded = if kind == PackageKind::WindowsPortable {
            Some(super::portable::package_version(&bytes).map_err(|_| "update_package")?)
        } else {
            None
        };
        #[cfg(not(windows))]
        let embedded: Option<String> = None;
        let hint = hint
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().trim_start_matches('v').to_owned());
        let version = embedded
            .clone()
            .or(named.clone())
            .or(hint.clone())
            .ok_or("update_version")?;
        Version::parse(&version).map_err(|_| "update_version")?;
        if [embedded, named, hint]
            .iter()
            .flatten()
            .any(|v| v != &version)
        {
            return Err("update_version".into());
        }
        let sig_path = path.with_file_name(format!("{file_name}.sig"));
        let feed_path = path.with_file_name("latest.json");
        let adjacent = if sig_path.exists() {
            Some(
                String::from_utf8(read_limited(&sig_path, MAX_METADATA)?)
                    .map_err(|_| "update_signature")?
                    .trim()
                    .into(),
            )
        } else if feed_path.exists() {
            signature_from_feed(&read_limited(&feed_path, MAX_METADATA)?, &version, kind)?
        } else {
            None
        };
        Ok::<_, String>((bytes, file_name, version, adjacent))
    })
    .await
    .map_err(|_| "update_filesystem")??;
    let explicit = adjacent.is_some();
    let signature = match adjacent {
        Some(sig) => Some(sig),
        None => official_signature(&version, kind).await?,
    };
    let (bytes, version, verified) = tauri::async_runtime::spawn_blocking(move || {
        // A self-build can share the official version without sharing its bytes.
        // Only absent sidecars allow this path; the UI/backend still require trust.
        let verified =
            local_verification(&bytes, signature.as_deref(), explicit, &pubkey, &version)?;
        Ok::<_, String>((bytes, version, verified))
    })
    .await
    .map_err(|_| "update_signature")??;
    let info = info(
        &current,
        &version,
        file_name,
        bytes.len() as u64,
        "local",
        verified,
        String::new(),
    )?;
    Ok(PreparedPackage {
        transfer_id: id,
        kind,
        info,
        bytes,
        automatic: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verifies_real_signatures_and_rejects_changed_payload_key_or_version() {
        // Disposable test key; no production signing material.
        let public = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDI2QTA1QUVFOEU1OEUzNTkKUldSWjQxaU83bHFnSnJHamZFUy96WEFQRmxPNVNCSFo0K01DeGtLWDZDak5FdXlsTFNuakk0UjkK";
        let signature = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVSWjQxaU83bHFnSmdyZzIwOERVR2ZNdHJES1FBUUxMeUp6YlVoZEJKcFNEZzkzWFNkUEFUMEh6N0k4U1lYKzZ3ZjlpdWEzMS8xTFJqUzVVMVBuYW1hVFB0YVdFdUNnbEF3PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk1MDk3ODQ4CWZpbGU6cGF5bG9hZAl2ZXJzaW9uOjAuMi4wCkhuWXJxMjRnS01SbzMwZ0V4T2pGUG1pMGcyMkxEWWh2WnhkSjJOYVNPbVRoUVRITUNZczU4Q3c4Wmswc1FWdS9SdmRiclk5TmFiSmFtY2xSS1RWM0F3PT0K";
        assert!(verify(b"DLDR local package test", signature, public, "0.2.0").is_ok());
        assert!(verify(b"changed bytes", signature, public, "0.2.0").is_err());
        assert!(verify(b"DLDR local package test", signature, public, "0.1.4").is_err());
        assert!(verify(b"DLDR local package test", signature, "wrong key", "0.2.0").is_err());
        assert!(local_verification(
            b"DLDR local package test",
            Some(signature),
            false,
            public,
            "0.2.0"
        )
        .unwrap());
        assert!(
            !local_verification(b"self-build", Some(signature), false, public, "0.2.0").unwrap()
        );
        assert!(local_verification(b"self-build", Some(signature), true, public, "0.2.0").is_err());
        assert!(!local_verification(b"self-build", None, false, public, "0.2.0").unwrap());
        let text = String::from_utf8(STANDARD.decode(signature).unwrap())
            .unwrap()
            .replace("version:0.2.0", "version:0.1.4");
        assert!(verify(
            b"DLDR local package test",
            &STANDARD.encode(text),
            public,
            "0.1.4"
        )
        .is_err());
    }

    #[test]
    fn local_version_and_direction_are_explicit() {
        assert_eq!(
            filename_version("DLDR_0.1.2_amd64.AppImage", PackageKind::AppImage).as_deref(),
            Some("0.1.2")
        );
        assert!(filename_version("DLDR_0.1.2_windows_x64.zip", PackageKind::AppImage).is_none());
        assert!(
            filename_version("DLDR_not-a-version_amd64.AppImage", PackageKind::AppImage).is_none()
        );
        assert_eq!(
            info(
                "0.1.4",
                "0.1.2",
                "test".into(),
                1,
                "local",
                false,
                "".into()
            )
            .unwrap()
            .direction,
            "downgrade"
        );
        assert_eq!(
            info(
                "0.1.4",
                "0.1.4",
                "test".into(),
                1,
                "local",
                false,
                "".into()
            )
            .unwrap()
            .direction,
            "reinstall"
        );
    }
    #[test]
    fn rejects_wrong_format_and_invalid_signature() {
        assert!(validate_format(
            b"not an AppImage",
            Path::new("test.AppImage"),
            PackageKind::AppImage
        )
        .is_err());
        let mut elf = vec![0; 64];
        elf[..6].copy_from_slice(b"\x7fELF\x02\x01");
        elf[8..11].copy_from_slice(b"AI\x02");
        elf[18] = 62;
        assert!(validate_format(&elf, Path::new("test.AppImage"), PackageKind::AppImage).is_ok());
        assert!(verify(&elf, "bad", "bad", "0.1.4").is_err());
        assert!(signature_from_feed(
            br#"{"version":"0.1.4","platforms":{}}"#,
            "0.1.4",
            PackageKind::AppImage
        )
        .is_err());
    }
}

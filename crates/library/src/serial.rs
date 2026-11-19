//! Sidecar serial information, written without overwriting user files.
use crate::{LibraryError, Result};
use dldr_api::SerialNumber;
use std::path::{Path, PathBuf};

pub(super) const SERIAL_INFORMATION_FILE_NAME: &str = "dldr-serial.txt";
pub(super) const SERIAL_INFORMATION_NUMBERED_PREFIX: &str = "dldr-serial-";
pub(super) const SERIAL_INFORMATION_MARKER: &str = "# dldr-desktop serial information";

pub(super) async fn write_serial_information_file(
    target_dir: &Path,
    work_id: &str,
    serial_numbers: &[SerialNumber],
    generated_at: &str,
) -> Result<Option<PathBuf>> {
    if serial_numbers.is_empty() {
        remove_owned_serial_information_files(target_dir).await?;
        return Ok(None);
    }

    let path = serial_information_path(target_dir).await?;
    let content = serial_information_content(work_id, serial_numbers, generated_at);

    tokio::fs::write(&path, content).await?;
    Ok(Some(path))
}

async fn serial_information_path(target_dir: &Path) -> Result<PathBuf> {
    let base = target_dir.join(SERIAL_INFORMATION_FILE_NAME);

    if serial_information_path_available(&base).await? {
        return Ok(base);
    }

    for index in 1..100 {
        let candidate = target_dir.join(format!("{SERIAL_INFORMATION_NUMBERED_PREFIX}{index}.txt"));

        if serial_information_path_available(&candidate).await? {
            return Ok(candidate);
        }
    }

    Err(LibraryError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not choose a serial information file name",
    )))
}

async fn serial_information_path_available(path: &Path) -> Result<bool> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => Ok(content.starts_with(SERIAL_INFORMATION_MARKER)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}

async fn remove_owned_serial_information_files(target_dir: &Path) -> Result<()> {
    remove_owned_serial_information_file(&target_dir.join(SERIAL_INFORMATION_FILE_NAME)).await?;

    for index in 1..100 {
        remove_owned_serial_information_file(
            &target_dir.join(format!("{SERIAL_INFORMATION_NUMBERED_PREFIX}{index}.txt")),
        )
        .await?;
    }

    Ok(())
}

async fn remove_owned_serial_information_file(path: &Path) -> Result<()> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) if content.starts_with(SERIAL_INFORMATION_MARKER) => {
            tokio::fs::remove_file(path).await?;
            Ok(())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn serial_information_content(
    work_id: &str,
    serial_numbers: &[SerialNumber],
    generated_at: &str,
) -> String {
    let mut content = String::new();

    content.push_str(SERIAL_INFORMATION_MARKER);
    content.push('\n');
    content.push_str(&format!("Work ID: {}\n", clean_serial_text(work_id)));
    content.push_str(&format!("Generated at: {generated_at}\n"));
    content.push_str("Source: DLsite download plan\n\n");
    content.push_str("Treat these serial values as sensitive.\n\n");
    content.push_str("Serial numbers:\n");

    for (index, serial) in serial_numbers.iter().enumerate() {
        content.push_str(&format!(
            "{}. {}\n",
            index + 1,
            clean_serial_text(&serial.label)
        ));
        content.push_str(&format!("   Value: {}\n", clean_serial_text(&serial.value)));
    }

    content
}

pub(super) fn clean_serial_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

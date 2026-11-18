use crate::{errors::*, state::*};
use dldr_storage::WorkDownloadStatus;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenPlayer {
    work_id: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenedPlayer {
    media: dldr_player::Media,
    state: dldr_player::Snapshot,
}

#[tauri::command]
pub(crate) async fn open_player(
    state: State<'_, AppState>,
    player: State<'_, dldr_player::Player>,
    request: OpenPlayer,
) -> Result<OpenedPlayer, String> {
    state
        .audit
        .operation("open_player")
        .command(async {
            let _admission = state.mutation_lock.read().await;
            let download = state
                .storage
                .work_download_state(&request.work_id)
                .await
                .map_err(command_error)?;
            if download.status != WorkDownloadStatus::Downloaded {
                return Err("This work is not downloaded.".into());
            }
            let root = download
                .local_path
                .filter(|p| !p.trim().is_empty())
                .ok_or("The downloaded work folder is unavailable.")?;
            let player = player.inner().clone();
            tauri::async_runtime::spawn_blocking(dldr_audit::blocking(move || {
                let media = player
                    .open(request.work_id, PathBuf::from(root))
                    .map_err(command_error)?;
                Ok(OpenedPlayer {
                    media,
                    state: player.snapshot(),
                })
            }))
            .await
            .map_err(command_error)?
        })
        .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlayerControl {
    work_id: String,
    control: dldr_player::Control,
}
#[tauri::command]
pub(crate) async fn control_player(
    state: State<'_, AppState>,
    player: State<'_, dldr_player::Player>,
    request: PlayerControl,
) -> Result<dldr_player::Snapshot, String> {
    state
        .audit
        .operation("control_player")
        .command(async {
            let player = player.inner().clone();
            tauri::async_runtime::spawn_blocking(dldr_audit::blocking(move || {
                player
                    .control(request.work_id, request.control)
                    .map_err(command_error)
            }))
            .await
            .map_err(command_error)?
        })
        .await
}
#[tauri::command]
pub(crate) fn player_snapshot(player: State<'_, dldr_player::Player>) -> dldr_player::Snapshot {
    player.snapshot()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlayerSubtitle {
    work_id: String,
    subtitle_id: String,
}
#[tauri::command]
pub(crate) async fn player_subtitle(
    state: State<'_, AppState>,
    player: State<'_, dldr_player::Player>,
    request: PlayerSubtitle,
) -> Result<Vec<dldr_player::Cue>, String> {
    state
        .audit
        .operation("player_subtitle")
        .command(async {
            let player = player.inner().clone();
            tauri::async_runtime::spawn_blocking(dldr_audit::blocking(move || {
                player
                    .subtitle(request.work_id, request.subtitle_id)
                    .map_err(command_error)
            }))
            .await
            .map_err(command_error)?
        })
        .await
}

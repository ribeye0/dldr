use crate::{errors::*, state::*};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum PlaylistEdit {
    Create {
        name: String,
    },
    Rename {
        playlist_id: String,
        name: String,
    },
    Add {
        playlist_id: String,
        work_id: String,
    },
    Remove {
        playlist_id: String,
        work_id: String,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlaylistEditResult {
    playlists: Vec<dldr_storage::Playlist>,
    playlist_id: String,
}

#[tauri::command]
pub(crate) async fn list_playlists(
    state: State<'_, AppState>,
) -> Result<Vec<dldr_storage::Playlist>, String> {
    state
        .audit
        .operation("list_playlists")
        .command(async { state.storage.playlists().await.map_err(command_error) })
        .await
}

#[tauri::command]
pub(crate) async fn edit_playlist(
    state: State<'_, AppState>,
    request: PlaylistEdit,
) -> Result<PlaylistEditResult, String> {
    state
        .audit
        .operation("edit_playlist")
        .command(async {
            let _admission = state.mutation_lock.read().await;
            let id = match request {
                PlaylistEdit::Create { name } => state
                    .storage
                    .create_playlist(&name)
                    .await
                    .map_err(command_error)?,
                PlaylistEdit::Rename { playlist_id, name } => {
                    state
                        .storage
                        .rename_playlist(&playlist_id, &name)
                        .await
                        .map_err(command_error)?;
                    playlist_id
                }
                PlaylistEdit::Add {
                    playlist_id,
                    work_id,
                } => {
                    state
                        .storage
                        .set_playlist_work(&playlist_id, &work_id, true)
                        .await
                        .map_err(command_error)?;
                    playlist_id
                }
                PlaylistEdit::Remove {
                    playlist_id,
                    work_id,
                } => {
                    state
                        .storage
                        .set_playlist_work(&playlist_id, &work_id, false)
                        .await
                        .map_err(command_error)?;
                    playlist_id
                }
            };
            Ok(PlaylistEditResult {
                playlists: state.storage.playlists().await.map_err(command_error)?,
                playlist_id: id,
            })
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::ListProductsRequest;
    use serde_json::json;

    #[test]
    fn playlist_ipc_uses_camel_case_ids_and_independent_query_scope() {
        let edit: PlaylistEdit = serde_json::from_value(json!({
            "action":"add","playlistId":"0","workId":"RJ000001"
        }))
        .expect("frontend add request");
        assert!(
            matches!(edit, PlaylistEdit::Add {playlist_id,work_id} if playlist_id=="0" && work_id=="RJ000001")
        );
        let request: ListProductsRequest =
            serde_json::from_value(json!({"playlistId":"named-list"})).expect("playlist query");
        let query = request.into_query().expect("valid playlist scope");
        assert_eq!(query.playlist_id.as_deref(), Some("named-list"));
        assert!(!query.downloaded_only);
        let request: ListProductsRequest =
            serde_json::from_value(json!({})).expect("default query");
        assert_eq!(
            request
                .into_query()
                .expect("valid default scope")
                .playlist_id,
            None
        );
    }
}

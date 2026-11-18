use crate::{dto::*, errors::*, state::*, validation::*};
use dldr_library::DlsitePublicMetadataSource;
use dldr_storage::UiLocale;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub(crate) async fn list_products(
    state: State<'_, AppState>,
    request: ListProductsRequest,
) -> Result<ProductListPageDto, String> {
    let diagnostic_operation = state.audit.operation("list_products");
    diagnostic_operation
        .command(async {
            state
                .library
                .list_products(&request.into_query()?)
                .await
                .map(ProductListPageDto::from)
                .map_err(command_error)
        })
        .await
}

#[tauri::command]
pub(crate) async fn list_product_filter_facets(
    state: State<'_, AppState>,
    request: ListProductsRequest,
) -> Result<ProductFilterFacetsDto, String> {
    let diagnostic_operation = state.audit.operation("list_product_filter_facets");
    diagnostic_operation
        .command(async {
            state
                .library
                .product_filter_facets(&request.into_query()?)
                .await
                .map(ProductFilterFacetsDto::from)
                .map_err(command_error)
        })
        .await
}

#[tauri::command]
pub(crate) async fn get_product_detail(
    state: State<'_, AppState>,
    request: GetProductDetailRequest,
) -> Result<ProductDetailDto, String> {
    let diagnostic_operation = state.audit.operation("get_product_detail");
    diagnostic_operation
        .command(async {
            let work_id = normalize_required_id(request.work_id)?;
            let client = dldr_api::DlsiteClient::new(dldr_api::DlsiteClientConfig::default())
                .map_err(command_error)?;
            let source = DlsitePublicMetadataSource::new(client);
            state
                .library
                .product_detail_with_metadata_source(&work_id, &source)
                .await
                .map(ProductDetailDto::from)
                .map_err(command_error)
        })
        .await
}

#[tauri::command]
pub(crate) async fn save_product_cover(
    app: AppHandle,
    state: State<'_, AppState>,
    work_id: String,
) -> Result<bool, String> {
    let operation = state.audit.operation("save_product_cover");
    operation
        .command(async {
            let work_id = normalize_required_id(work_id)?;
            let detail = state
                .library
                .product_detail(&work_id)
                .await
                .map_err(command_error)?;
            let url = detail.thumbnail_url.ok_or("This work has no cover image")?;
            let cover = dldr_library::cover::fetch_cover(&url)
                .await
                .map_err(command_error)?;
            let (send, recv) = tokio::sync::oneshot::channel();
            let locale = state
                .storage
                .app_settings()
                .await
                .map(|s| s.ui_locale)
                .unwrap_or_default();
            let (title, filter) = match locale {
                UiLocale::TraditionalChinese => ("儲存封面圖片", "封面圖片"),
                UiLocale::SimplifiedChinese => ("保存封面图片", "封面图片"),
                UiLocale::Japanese => ("ジャケット画像を保存", "ジャケット画像"),
                UiLocale::English => ("Save cover image", "Cover image"),
            };
            app.dialog()
                .file()
                .set_title(title)
                .set_file_name(format!("{}.{}", work_id, cover.extension))
                .add_filter(filter, &[cover.extension])
                .save_file(move |path| {
                    let _ = send.send(path);
                });
            let Some(path) = recv.await.map_err(|_| "Save dialog unavailable")? else {
                return Ok(false);
            };
            let path = path
                .into_path()
                .map_err(|_| "Only local file destinations are supported")?;
            cover.save(&path).await.map_err(command_error)?;
            Ok(true)
        })
        .await
}

#[tauri::command]
pub(crate) async fn get_product_page_content(
    state: State<'_, AppState>,
    work_id: String,
    locale: String,
) -> Result<String, String> {
    state
        .audit
        .operation("get_product_page_content")
        .command(async {
            let work_id = normalize_required_id(work_id)?;
            // Restrict this command to works already present in the local library.
            state
                .library
                .product_detail(&work_id)
                .await
                .map_err(command_error)?;
            dldr_library::product_page::fetch_content(&work_id, &locale)
                .await
                .map_err(command_error)
        })
        .await
}

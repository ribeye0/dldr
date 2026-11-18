//! Main-window GTK chrome only; never changes global desktop settings or remote pages.
use tauri::WebviewWindow;

#[cfg(target_os = "linux")]
use tauri::Manager;

#[cfg(target_os = "linux")]
use gtk::prelude::*;

pub fn setup(app: &tauri::App) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let window = app
            .get_webview_window("main")
            .ok_or("Main window is unavailable")?;
        let native = window.gtk_window().map_err(|e| e.to_string())?;
        let provider = gtk::CssProvider::new();
        provider
            .load_from_data(include_bytes!("window_appearance.css"))
            .map_err(|e| e.to_string())?;
        let screen = gtk::prelude::WidgetExt::screen(&native).ok_or("GTK screen is unavailable")?;
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        native.style_context().add_class("dldr-native-window");
        if let Some(titlebar) = native.titlebar() {
            compact_header(&titlebar);
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
    Ok(())
}

#[cfg(target_os = "linux")]
fn compact_header(widget: &gtk::Widget) {
    if let Some(header) = widget.downcast_ref::<gtk::HeaderBar>() {
        // Tao wraps this header in an EventBox on Wayland. Keep that wrapper and
        // all native drag/double-click/resize/button handlers exactly as installed.
        header.set_has_subtitle(false);
        header.style_context().add_class("dldr-headerbar");
    } else if let Some(container) = widget.downcast_ref::<gtk::Container>() {
        for child in container.children() {
            compact_header(&child);
        }
    }
}

#[tauri::command]
pub async fn set_native_header_theme(window: WebviewWindow, dark: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let target = window.clone();
        window
            .run_on_main_thread(move || {
                let result = target
                    .gtk_window()
                    .map_err(|e| e.to_string())
                    .map(|native| {
                        let context = native.style_context();
                        if dark {
                            context.add_class("dldr-dark");
                        } else {
                            context.remove_class("dldr-dark");
                        }
                    });
                let _ = sender.send(result);
            })
            .map_err(|e| e.to_string())?;
        receiver.await.map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, dark);
        Ok(())
    }
}

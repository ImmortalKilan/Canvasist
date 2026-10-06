//! Main window lifecycle.
//!
//! The main window is created on demand and fully destroyed when closed, so the
//! WebView (the largest memory consumer) only exists while the user looks at it.
//! The app itself keeps running in the tray.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const MAIN_LABEL: &str = "main";

/// Shows and focuses the main window, creating it if it does not exist.
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    // WebView2 can deadlock if a webview is created synchronously inside an
    // event handler, so build it from the async runtime instead.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = WebviewWindowBuilder::new(&app, MAIN_LABEL, WebviewUrl::default())
            .title("Canvasist")
            .inner_size(440.0, 680.0)
            .min_inner_size(360.0, 480.0)
            .center()
            .build();
        if let Err(e) = result {
            // Two rapid tray clicks can race to create the window; the loser fails harmlessly.
            log::debug!("main window not created: {e}");
        }
    });
}

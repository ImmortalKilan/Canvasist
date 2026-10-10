//! Direct Gradescope sign-in, for students whose courses are not linked to
//! Gradescope through Canvas (see `bridge.rs`).
//!
//! Works like the Canvas login window: a window shows Gradescope's own sign-in
//! page (school credentials or email; Canvasist never sees a password). After
//! each page load on Gradescope, its cookies are checked against Gradescope. On
//! success they are added to the encrypted session and the window is closed.
//!
//! The window shares the Canvas login window's browser data, so a school sign-in
//! that is still remembered can be reused, and it is granted no capabilities.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use url::Url;

use crate::account;
use crate::domain::is_gradescope_host;
use crate::error::{AppError, AppResult};
use crate::locale::{self, Locale};
use crate::settings::SettingsStore;
use crate::window;

pub const LOGIN_LABEL: &str = "gradescope-login";
/// Tells the main window to refresh, which loads the Gradescope assignments.
pub const EVENT_LOGIN_COMPLETED: &str = "gradescope-login-completed";

/// Opens (or focuses) the Gradescope sign-in window for `origin`.
pub fn start(app: &AppHandle, origin: Url) -> AppResult<()> {
    if let Some(existing) = app.get_webview_window(LOGIN_LABEL) {
        existing.set_focus()?;
        return Ok(());
    }
    let is_gradescope = origin.host_str().is_some_and(is_gradescope_host);
    if origin.scheme() != "https" || !is_gradescope {
        return Err(AppError::InvalidUrl);
    }
    let login_url = origin.join("/login").map_err(|_| AppError::InvalidUrl)?;
    let title = match locale::current(app.state::<SettingsStore>().get().language) {
        Locale::En => "Sign in to Gradescope",
        Locale::ZhCn => "登录 Gradescope",
    };

    let completed = Arc::new(AtomicBool::new(false));
    WebviewWindowBuilder::new(app, LOGIN_LABEL, WebviewUrl::External(login_url))
        .title(title)
        .inner_size(520.0, 760.0)
        .center()
        .data_directory(account::login_data_dir(app)?)
        .on_page_load(move |window, payload| {
            // Hosts only, at debug level, as in the Canvas login window.
            log::debug!(
                "gradescope sign-in window: page load {:?} on {}",
                payload.event(),
                payload.url().host_str().unwrap_or("?")
            );
            if payload.event() == PageLoadEvent::Finished {
                let url = payload.url().clone();
                tauri::async_runtime::spawn(try_complete(window, url, completed.clone()));
            }
        })
        .build()?;
    log::info!("gradescope: direct sign-in window opened");
    Ok(())
}

pub fn cancel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LOGIN_LABEL) {
        let _ = w.close();
    }
}

pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(LOGIN_LABEL).is_some()
}

/// Checks whether the page that just loaded belongs to a signed-in Gradescope user.
async fn try_complete(window: WebviewWindow, url: Url, completed: Arc<AtomicBool>) {
    if url.scheme() != "https" || !url.host_str().is_some_and(is_gradescope_host) {
        return;
    }
    let Ok(origin) = Url::parse(&url.origin().ascii_serialization()) else {
        return;
    };
    // Reading cookies on the main thread deadlocks WebView2; this runs on the async runtime.
    let cookies = match window.cookies_for_url(origin.clone()) {
        Ok(cookies) if !cookies.is_empty() => cookies,
        Ok(_) => return,
        Err(e) => {
            log::debug!("could not read Gradescope sign-in cookies: {e}");
            return;
        }
    };
    match super::signed_in(&origin, &cookies).await {
        Ok(true) => {}
        // Not signed in yet (still on a sign-in or school page).
        Ok(false) => return,
        Err(e) => {
            log::debug!("Gradescope sign-in probe failed: {e}");
            return;
        }
    }
    if completed.swap(true, Ordering::SeqCst) {
        return; // Another page load already completed the sign-in.
    }

    let app = window.app_handle().clone();
    if let Err(e) = account::attach_gradescope_session(&app, origin, &cookies) {
        log::warn!("could not save the Gradescope session: {e}");
        return;
    }
    log::info!("gradescope: direct sign-in completed");
    let _ = window.close();
    window::show_main(&app);
    if let Err(e) = app.emit_to(window::MAIN_LABEL, EVENT_LOGIN_COMPLETED, ()) {
        log::debug!("gradescope-login-completed not delivered: {e}");
    }
}

//! Obtains a Gradescope session through Canvas's LTI launch.
//!
//! Gradescope has no API and students sign in to it through Canvas. An
//! invisible window (sharing the login window's browser data) is given the
//! stored Canvas session cookies and opens a course's Gradescope tab, exactly
//! as a user would. Once Gradescope has set its session cookies, they are
//! copied into Canvasist's encrypted cookie jar and the window is closed. The
//! window is granted no capabilities.

use std::time::{Duration, Instant};

use tauri::webview::Cookie;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use url::Url;

use crate::account;
use crate::canvas::cookies::SharedCookies;
use crate::domain::is_gradescope_host;
use crate::error::{AppError, AppResult};

const BRIDGE_LABEL: &str = "gradescope-bridge";
const GRADESCOPE_ORIGINS: &[&str] = &[
    "https://www.gradescope.com",
    "https://www.gradescope.ca",
    "https://www.gradescope.eu",
];
/// Cookies that only exist once Gradescope has signed the user in.
const SESSION_COOKIES: &[&str] = &["signed_token", "remember_me"];
const TIMEOUT: Duration = Duration::from_secs(45);
const POLL_INTERVAL: Duration = Duration::from_millis(750);

/// Closes the bridge window however the launch ends.
struct WindowGuard(WebviewWindow);

impl Drop for WindowGuard {
    fn drop(&mut self) {
        let _ = self.0.destroy();
    }
}

/// Launches Gradescope from `tab_url` and copies its session cookies into
/// `jar`. Returns the Gradescope origin that was signed in.
pub async fn connect(
    app: &AppHandle,
    canvas_origin: &Url,
    tab_url: &Url,
    jar: &SharedCookies,
) -> AppResult<Url> {
    if let Some(stale) = app.get_webview_window(BRIDGE_LABEL) {
        let _ = stale.destroy();
    }
    let window = WebviewWindowBuilder::new(
        app,
        BRIDGE_LABEL,
        WebviewUrl::External("about:blank".parse().map_err(|_| AppError::InvalidUrl)?),
    )
    .title("Canvasist")
    .visible(false)
    .skip_taskbar(true)
    .data_directory(account::login_data_dir(app)?)
    .build()?;
    let window = WindowGuard(window);

    // Canvas session cookies have no expiry, so the webview forgets them on
    // restart; hand it the stored ones before navigating.
    let host = canvas_origin
        .host_str()
        .ok_or(AppError::InvalidUrl)?
        .to_owned();
    for entry in jar.entries_for(canvas_origin) {
        let cookie = Cookie::build((entry.name, entry.value))
            .domain(host.clone())
            .path(entry.path)
            .secure(entry.secure)
            .http_only(entry.http_only)
            .build();
        window.0.set_cookie(cookie)?;
    }
    window.0.navigate(tab_url.clone())?;
    log::info!("gradescope: launching through Canvas");

    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        tokio::time::sleep(POLL_INTERVAL).await;
        for origin in GRADESCOPE_ORIGINS {
            let origin = Url::parse(origin).map_err(|_| AppError::InvalidUrl)?;
            let cookies = window.0.cookies_for_url(origin.clone())?;
            if cookies.iter().any(|c| SESSION_COOKIES.contains(&c.name())) {
                for cookie in &cookies {
                    jar.insert_set_cookie(&cookie.to_string(), &origin);
                }
                log::info!("gradescope: session obtained");
                return Ok(origin);
            }
        }
    }

    // Still on a non-Canvas, non-Gradescope host means Canvas sent the window
    // to the school's sign-in page: the stored Canvas session no longer works.
    let host_now = window
        .0
        .url()
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned));
    let on_canvas = host_now.as_deref() == canvas_origin.host_str();
    let on_gradescope = host_now.as_deref().is_some_and(is_gradescope_host);
    // Cookie names only (never values): shows whether Gradescope signed the
    // user in under different cookie names than the ones expected.
    let mut names: Vec<String> = Vec::new();
    for origin in GRADESCOPE_ORIGINS {
        let Ok(origin) = Url::parse(origin) else {
            continue;
        };
        for cookie in window.0.cookies_for_url(origin).unwrap_or_default() {
            if !names.iter().any(|n| n == cookie.name()) {
                names.push(cookie.name().to_owned());
            }
        }
    }
    names.sort();
    log::info!(
        "gradescope: launch timed out (on canvas: {on_canvas}, on gradescope: {on_gradescope}, gradescope cookie names: {names:?})"
    );
    if !on_canvas && !on_gradescope {
        Err(AppError::SessionExpired)
    } else {
        Err(AppError::GradescopeAuth)
    }
}

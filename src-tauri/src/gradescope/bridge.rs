//! Obtains a Gradescope session through Canvas's LTI launch.
//!
//! Gradescope has no API and students sign in to it through Canvas. An
//! invisible window (sharing the login window's browser data) is given the
//! stored Canvas session cookies and opens a course's Gradescope tab, exactly
//! as a user would. Once Gradescope accepts the cookies it has set, they are
//! copied into Canvasist's encrypted cookie jar and the window is closed. The
//! window is granted no capabilities.
//!
//! The launch is opened as the whole page rather than inside Canvas's frame
//! (see `launch.js`): cookies set inside a frame are third-party cookies, which
//! some WebView2 setups block or keep apart from the rest of the browser data.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::webview::Cookie;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use url::Url;

use super::client::GradescopeClient;
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
const LAUNCH_SCRIPT: &str = include_str!("launch.js");
const TIMEOUT: Duration = Duration::from_secs(45);
const POLL_INTERVAL: Duration = Duration::from_millis(750);
/// Minimum time between two sign-in checks against Gradescope.
const CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// An unchanged set of cookies is checked again after this long, in case
/// Gradescope finished signing in on its side after the previous check.
const RECHECK_INTERVAL: Duration = Duration::from_secs(10);

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
    let origins: Vec<Url> = GRADESCOPE_ORIGINS
        .iter()
        .map(|o| Url::parse(o).map_err(|_| AppError::InvalidUrl))
        .collect::<AppResult<_>>()?;

    // Canvas answers an unusable session by sending the window to its sign-in
    // page. Other hosts (Gradescope's, or a launch service's such as
    // Turnitin's) appear during a normal launch, so only this counts.
    let sent_to_login = Arc::new(AtomicBool::new(false));
    let login_flag = sent_to_login.clone();
    let canvas = canvas_origin.origin();

    let window = WebviewWindowBuilder::new(
        app,
        BRIDGE_LABEL,
        WebviewUrl::External("about:blank".parse().map_err(|_| AppError::InvalidUrl)?),
    )
    .title("Canvasist")
    .visible(false)
    .skip_taskbar(true)
    .data_directory(account::login_data_dir(app)?)
    .initialization_script(launch_script(canvas_origin))
    .on_navigation(move |url| {
        if url.origin() == canvas && is_login_path(url.path()) {
            login_flag.store(true, Ordering::SeqCst);
        }
        true
    })
    .build()?;
    let window = WindowGuard(window);

    // Start without Gradescope cookies left over from an earlier session, so
    // only cookies set by this launch are considered.
    for origin in &origins {
        for cookie in window.0.cookies_for_url(origin.clone())? {
            window.0.delete_cookie(cookie)?;
        }
    }

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

    // Gradescope sets cookies several times during a launch; each new set is
    // checked against Gradescope itself, so no cookie names are assumed.
    let mut checked: HashMap<Url, (u64, Instant)> = HashMap::new();
    let mut last_check: Option<Instant> = None;
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        tokio::time::sleep(POLL_INTERVAL).await;
        if last_check.is_some_and(|t| t.elapsed() < CHECK_INTERVAL) {
            continue;
        }
        for origin in &origins {
            let cookies = window.0.cookies_for_url(origin.clone())?;
            if cookies.is_empty() {
                continue;
            }
            let fingerprint = fingerprint(&cookies);
            if checked
                .get(origin)
                .is_some_and(|(f, at)| *f == fingerprint && at.elapsed() < RECHECK_INTERVAL)
            {
                continue;
            }
            last_check = Some(Instant::now());
            checked.insert(origin.clone(), (fingerprint, Instant::now()));
            if signed_in(origin, &cookies).await? {
                for cookie in &cookies {
                    jar.insert_set_cookie(&cookie.to_string(), origin);
                }
                log::info!(
                    "gradescope: session obtained (full-page launch: {})",
                    window_host(&window.0)
                        .as_deref()
                        .is_some_and(is_gradescope_host)
                );
                return Ok(origin.clone());
            }
            break;
        }
    }

    let host_now = window_host(&window.0);
    let on_canvas = host_now.as_deref() == canvas_origin.host_str();
    let on_gradescope = host_now.as_deref().is_some_and(is_gradescope_host);
    let sent_to_login = sent_to_login.load(Ordering::SeqCst);
    // Cookie names only (never values): shows how far Gradescope got.
    let mut names: Vec<String> = Vec::new();
    for origin in &origins {
        for cookie in window.0.cookies_for_url(origin.clone()).unwrap_or_default() {
            if !names.iter().any(|n| n == cookie.name()) {
                names.push(cookie.name().to_owned());
            }
        }
    }
    names.sort();
    log::info!(
        "gradescope: launch timed out (on canvas: {on_canvas}, on gradescope: {on_gradescope}, sent to canvas login: {sent_to_login}, gradescope cookie names: {names:?})"
    );
    if sent_to_login {
        Err(AppError::SessionExpired)
    } else {
        Err(AppError::GradescopeAuth)
    }
}

/// The launch script with the Canvas origin filled in. `Url` serializes an
/// origin as plain ASCII, and JSON string escaping keeps it a valid JS literal.
fn launch_script(canvas_origin: &Url) -> String {
    let origin = canvas_origin.origin().ascii_serialization();
    let literal = serde_json::to_string(&origin).unwrap_or_else(|_| "\"\"".into());
    LAUNCH_SCRIPT.replace("\"__CANVAS_ORIGIN__\"", &literal)
}

/// Identifies a set of cookies by name and value, independent of order.
fn fingerprint(cookies: &[Cookie<'static>]) -> u64 {
    let mut pairs: Vec<(&str, &str)> = cookies.iter().map(|c| (c.name(), c.value())).collect();
    pairs.sort_unstable();
    let mut hasher = DefaultHasher::new();
    pairs.hash(&mut hasher);
    hasher.finish()
}

/// Asks Gradescope whether these cookies belong to a signed-in user.
async fn signed_in(origin: &Url, cookies: &[Cookie<'static>]) -> AppResult<bool> {
    let candidate = Arc::new(SharedCookies::default());
    for cookie in cookies {
        candidate.insert_set_cookie(&cookie.to_string(), origin);
    }
    match GradescopeClient::new(origin.clone(), candidate)?
        .verify_session()
        .await
    {
        Ok(()) => Ok(true),
        Err(AppError::GradescopeAuth) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Canvas's sign-in pages: `/login`, `/login/saml`, `/login/canvas`, ...
fn is_login_path(path: &str) -> bool {
    path == "/login" || path.starts_with("/login/")
}

fn window_host(window: &WebviewWindow) -> Option<String> {
    window
        .url()
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_script_is_bound_to_the_canvas_origin() {
        let script = launch_script(&Url::parse("https://canvas.example.edu/courses/1").unwrap());
        assert!(script.contains("location.origin !== \"https://canvas.example.edu\""));
        assert!(!script.contains("__CANVAS_ORIGIN__"));
    }

    #[test]
    fn canvas_login_paths() {
        assert!(is_login_path("/login"));
        assert!(is_login_path("/login/saml"));
        assert!(!is_login_path("/courses/1/external_tools/2"));
        assert!(!is_login_path("/loginx"));
    }

    #[test]
    fn fingerprint_ignores_order_but_not_values() {
        let a = Cookie::new("a", "1");
        let b = Cookie::new("b", "2");
        let b2 = Cookie::new("b", "3");
        let ab = fingerprint(&[a.clone(), b.clone()]);
        assert_eq!(ab, fingerprint(&[b, a.clone()]));
        assert_ne!(ab, fingerprint(&[a, b2]));
    }
}

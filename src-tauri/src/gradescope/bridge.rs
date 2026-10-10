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
//!
//! Gradescope only signs a student in from a course that the instructor has
//! linked to Gradescope through Canvas. Other courses still show a Gradescope
//! tab, which opens a "Course hasn't been created" page. Each course is tried
//! in turn; when none signs the user in, the user can sign in to Gradescope
//! directly instead (see `login.rs`).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
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
const LAUNCH_SCRIPT: &str = include_str!("launch.js");
/// Longest wait for one course's launch.
const TIMEOUT: Duration = Duration::from_secs(45);
const POLL_INTERVAL: Duration = Duration::from_millis(750);
/// Minimum time between two rounds of sign-in checks against Gradescope.
const CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// An unchanged set of cookies is checked again after this long, in case
/// Gradescope finished signing in on its side after the previous check.
const RECHECK_INTERVAL: Duration = Duration::from_secs(10);
/// A Gradescope page that has not navigated for this long, without the user
/// being signed in, means Gradescope declined to sign in from that course.
const SETTLE: Duration = Duration::from_secs(8);

/// How a launch through Canvas ended.
#[derive(Debug)]
pub enum Launch {
    /// Signed in to this Gradescope origin.
    Connected(Url),
    /// Gradescope opened from every course tried but did not sign the user in.
    /// Carries the Gradescope origin that was reached, for a direct sign-in.
    Declined(Url),
}

/// How one course's launch ended.
enum Attempt {
    SignedIn(Url, Vec<Cookie<'static>>),
    /// Reached Gradescope (this origin and page kind) without being signed in.
    Declined(Url, String),
    /// Never reached Gradescope.
    Stuck,
}

/// Closes the bridge window however the launch ends.
struct WindowGuard(WebviewWindow);

impl Drop for WindowGuard {
    fn drop(&mut self) {
        let _ = self.0.destroy();
    }
}

/// Launches Gradescope from each course tab in `tabs`, in order, until one
/// signs the user in, and copies that session's cookies into `jar`.
pub async fn connect(
    app: &AppHandle,
    canvas_origin: &Url,
    tabs: &[Url],
    jar: &SharedCookies,
) -> AppResult<Launch> {
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
    let navigations = Arc::new(AtomicUsize::new(0));
    let (login_flag, nav_count) = (sent_to_login.clone(), navigations.clone());
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
        nav_count.fetch_add(1, Ordering::SeqCst);
        if url.origin() == canvas && is_login_path(url.path()) {
            login_flag.store(true, Ordering::SeqCst);
        }
        true
    })
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
    log::info!(
        "gradescope: launching through Canvas ({} course(s) to try)",
        tabs.len()
    );

    let mut declined = None;
    for (i, tab) in tabs.iter().enumerate() {
        // The direct sign-in window shares these cookies; clearing them would
        // undo a sign-in in progress there.
        if super::login::is_open(app) {
            log::info!("gradescope: direct sign-in started, launch stopped");
            break;
        }
        // Start without Gradescope cookies left over from an earlier session
        // or course, so only cookies set by this launch are considered.
        for origin in &origins {
            for cookie in window.0.cookies_for_url(origin.clone())? {
                window.0.delete_cookie(cookie)?;
            }
        }
        window.0.navigate(tab.clone())?;

        match attempt(&window.0, &origins, &navigations, &sent_to_login).await? {
            Attempt::SignedIn(origin, cookies) => {
                for cookie in &cookies {
                    jar.insert_set_cookie(&cookie.to_string(), &origin);
                }
                log::info!(
                    "gradescope: session obtained from course {} of {} (full-page launch: {})",
                    i + 1,
                    tabs.len(),
                    window_url(&window.0)
                        .and_then(|u| u.host_str().map(is_gradescope_host))
                        .unwrap_or(false)
                );
                return Ok(Launch::Connected(origin));
            }
            Attempt::Declined(origin, page) => {
                log::info!(
                    "gradescope: course {} of {} opened Gradescope ({page} page) without signing in",
                    i + 1,
                    tabs.len()
                );
                declined = Some(origin);
            }
            Attempt::Stuck => {
                // Not specific to one course: the launch itself did not work.
                let on_canvas =
                    window_url(&window.0).is_some_and(|u| u.host_str() == canvas_origin.host_str());
                log::info!(
                    "gradescope: launch timed out (on canvas: {on_canvas}, gradescope cookie names: {:?})",
                    cookie_names(&window.0, &origins)
                );
                return Err(AppError::GradescopeAuth);
            }
        }
    }

    if declined.is_some() {
        log::info!(
            "gradescope: no course signed in through Canvas (gradescope cookie names: {:?})",
            cookie_names(&window.0, &origins)
        );
    }
    declined
        .map(Launch::Declined)
        .ok_or(AppError::GradescopeAuth)
}

/// Follows one course's launch until Gradescope signs in, settles without
/// signing in, or the launch times out.
async fn attempt(
    window: &WebviewWindow,
    origins: &[Url],
    navigations: &AtomicUsize,
    sent_to_login: &AtomicBool,
) -> AppResult<Attempt> {
    let mut checked: HashMap<Url, (u64, Instant)> = HashMap::new();
    let mut last_round: Option<Instant> = None;
    let mut seen = navigations.load(Ordering::SeqCst);
    let mut quiet_since = Instant::now();
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        tokio::time::sleep(POLL_INTERVAL).await;
        if sent_to_login.load(Ordering::SeqCst) {
            log::info!("gradescope: Canvas asked to sign in again");
            return Err(AppError::SessionExpired);
        }
        let count = navigations.load(Ordering::SeqCst);
        if count != seen {
            seen = count;
            quiet_since = Instant::now();
        }

        let landed = window_url(window).filter(|u| u.host_str().is_some_and(is_gradescope_host));
        if let Some(page) = landed.as_ref().filter(|_| quiet_since.elapsed() >= SETTLE) {
            // Settled on Gradescope: one last check of whatever it has set.
            return Ok(match check(window, origins, &mut checked, true).await? {
                Some((origin, cookies)) => Attempt::SignedIn(origin, cookies),
                None => Attempt::Declined(origin_of(page)?, page_kind(page.path())),
            });
        }

        if last_round.is_some_and(|t| t.elapsed() < CHECK_INTERVAL) {
            continue;
        }
        last_round = Some(Instant::now());
        if let Some((origin, cookies)) = check(window, origins, &mut checked, false).await? {
            return Ok(Attempt::SignedIn(origin, cookies));
        }
    }
    Ok(
        match window_url(window).filter(|u| u.host_str().is_some_and(is_gradescope_host)) {
            Some(page) => Attempt::Declined(origin_of(&page)?, page_kind(page.path())),
            None => Attempt::Stuck,
        },
    )
}

/// Asks Gradescope about each origin's cookies, skipping sets that were
/// checked recently unless `force`. Gradescope sets cookies several times
/// during a launch; checking against Gradescope itself means no cookie names
/// are assumed. Returns the signed-in origin and its cookies.
async fn check(
    window: &WebviewWindow,
    origins: &[Url],
    checked: &mut HashMap<Url, (u64, Instant)>,
    force: bool,
) -> AppResult<Option<(Url, Vec<Cookie<'static>>)>> {
    for origin in origins {
        let cookies = window.cookies_for_url(origin.clone())?;
        if cookies.is_empty() {
            continue;
        }
        let fingerprint = fingerprint(&cookies);
        let recent = checked
            .get(origin)
            .is_some_and(|(f, at)| *f == fingerprint && at.elapsed() < RECHECK_INTERVAL);
        if recent && !force {
            continue;
        }
        checked.insert(origin.clone(), (fingerprint, Instant::now()));
        if super::signed_in(origin, &cookies).await? {
            return Ok(Some((origin.clone(), cookies)));
        }
    }
    Ok(None)
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

/// Canvas's sign-in pages: `/login`, `/login/saml`, `/login/canvas`, ...
fn is_login_path(path: &str) -> bool {
    path == "/login" || path.starts_with("/login/")
}

/// A label for the kind of Gradescope page reached, safe to log: the first
/// path segment when it is a plain route name, never IDs or other values.
fn page_kind(path: &str) -> String {
    let segment = path.trim_start_matches('/').split('/').next().unwrap_or("");
    let is_route =
        segment.len() <= 24 && segment.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    match segment {
        "" => "home".into(),
        s if is_route => s.into(),
        _ => "other".into(),
    }
}

fn origin_of(url: &Url) -> AppResult<Url> {
    Url::parse(&url.origin().ascii_serialization()).map_err(|_| AppError::InvalidUrl)
}

fn window_url(window: &WebviewWindow) -> Option<Url> {
    window.url().ok()
}

/// Names (never values) of the Gradescope cookies in the window, for logs.
fn cookie_names(window: &WebviewWindow, origins: &[Url]) -> Vec<String> {
    let mut names: Vec<String> = origins
        .iter()
        .flat_map(|o| window.cookies_for_url(o.clone()).unwrap_or_default())
        .map(|c| c.name().to_owned())
        .collect();
    names.sort();
    names.dedup();
    names
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

    #[test]
    fn page_kinds_never_contain_ids() {
        assert_eq!(page_kind("/"), "home");
        assert_eq!(page_kind("/login"), "login");
        assert_eq!(page_kind("/courses/123456/assignments/7"), "courses");
        assert_eq!(page_kind("/auth/lti1p3/launch"), "auth");
        assert_eq!(page_kind("/123456"), "other");
        assert_eq!(page_kind("/a1b2c3d4"), "other");
        assert_eq!(page_kind("/Mixed"), "other");
        assert_eq!(page_kind(&format!("/{}", "x".repeat(25))), "other");
    }
}

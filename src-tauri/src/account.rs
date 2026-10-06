//! Canvas sign-in state: the login window, the encrypted session, and the
//! cached assignment snapshot.
//!
//! Login flow: a dedicated window shows the school's own Canvas login page
//! (SSO and MFA happen there; Canvasist never sees a password). After each page
//! load on a Canvas host, the window's cookies are copied into a private jar and
//! verified against the Canvas API. On success the jar is encrypted to disk and
//! the window is closed.
//!
//! The login window uses its own WebView2 data directory and is granted no
//! capabilities, so school web pages can never call Canvasist commands.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use tauri::webview::PageLoadEvent;
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use url::Url;

use crate::canvas::client::CanvasClient;
use crate::canvas::cookies::SharedCookies;
use crate::canvas::discovery::origin_string;
use crate::domain::Snapshot;
use crate::error::{AppError, AppResult};
use crate::gradescope;
use crate::secure_store::SecureStore;
use crate::settings::SettingsStore;
use crate::window;

pub const LOGIN_LABEL: &str = "canvas-login";
const SESSION_RECORD: &str = "canvas-session";
const SNAPSHOT_RECORD: &str = "snapshot";
/// Snapshot record name used before Gradescope support; removed on startup.
const LEGACY_SNAPSHOT_RECORD: &str = "canvas-snapshot";
const LOGIN_DATA_DIR: &str = "login-webview";
const WIPE_MARKER: &str = "login-webview.wipe";

/// Cookie names that mark a host as a Canvas site worth probing.
const CANVAS_SESSION_COOKIES: &[&str] = &[
    "canvas_session",
    "_normandy_session",
    "_legacy_normandy_session",
];

pub const EVENT_AUTH_CHANGED: &str = "auth-changed";
pub const EVENT_LOGIN_CANCELLED: &str = "canvas-login-cancelled";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AuthStatus {
    SignedOut,
    SignedIn {
        origin: String,
    },
    /// A session exists but Canvas rejected it; the user must log in again.
    Expired {
        origin: String,
    },
}

#[derive(Clone)]
struct Session {
    origin: Url,
    /// Holds both the Canvas and the Gradescope cookies; each is only ever
    /// sent to its own host.
    cookies: Arc<SharedCookies>,
    gradescope_origin: Option<Url>,
}

#[derive(Serialize, Deserialize)]
struct StoredSession {
    origin: String,
    cookies: String,
    #[serde(default)]
    gradescope_origin: Option<String>,
}

#[derive(Default)]
struct Inner {
    session: Option<Session>,
    expired: bool,
    snapshot: Option<Snapshot>,
}

pub struct Account {
    store: SecureStore,
    inner: Mutex<Inner>,
    refreshing: AtomicBool,
    login_completed: AtomicBool,
}

impl Account {
    /// Restores the saved session and snapshot, if any.
    pub fn load(store: SecureStore) -> Self {
        let session = store
            .load::<StoredSession>(SESSION_RECORD)
            .unwrap_or_else(|e| {
                log::warn!("could not read saved session: {e}");
                None
            })
            .and_then(|s| {
                let origin = Url::parse(&s.origin).ok()?;
                let cookies = SharedCookies::from_json(&s.cookies).ok()?;
                Some(Session {
                    origin,
                    cookies: Arc::new(cookies),
                    gradescope_origin: s.gradescope_origin.and_then(|g| Url::parse(&g).ok()),
                })
            });
        let _ = store.delete(LEGACY_SNAPSHOT_RECORD);
        let snapshot = if session.is_some() {
            store.load(SNAPSHOT_RECORD).unwrap_or(None)
        } else {
            None
        };
        Self {
            store,
            inner: Mutex::new(Inner {
                session,
                expired: false,
                snapshot,
            }),
            refreshing: AtomicBool::new(false),
            login_completed: AtomicBool::new(false),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn status(&self) -> AuthStatus {
        let inner = self.lock();
        match &inner.session {
            None => AuthStatus::SignedOut,
            Some(s) if inner.expired => AuthStatus::Expired {
                origin: origin_string(&s.origin),
            },
            Some(s) => AuthStatus::SignedIn {
                origin: origin_string(&s.origin),
            },
        }
    }

    pub fn snapshot(&self) -> Option<Snapshot> {
        self.lock().snapshot.clone()
    }

    fn save_session(&self, session: &Session) -> AppResult<()> {
        self.store.save(
            SESSION_RECORD,
            &StoredSession {
                origin: origin_string(&session.origin),
                cookies: session.cookies.to_json()?,
                gradescope_origin: session.gradescope_origin.as_ref().map(origin_string),
            },
        )
    }
}

fn emit_status(app: &AppHandle) {
    let status = app.state::<Account>().status();
    if let Err(e) = app.emit_to(window::MAIN_LABEL, EVENT_AUTH_CHANGED, status) {
        log::debug!("auth-changed not delivered: {e}");
    }
}

// ---------- login window ----------

pub fn login_data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_local_data_dir()?.join(LOGIN_DATA_DIR))
}

/// Opens (or focuses) the login window for the Canvas site at `origin`.
pub fn start_login(app: &AppHandle, origin: Url) -> AppResult<()> {
    if let Some(existing) = app.get_webview_window(LOGIN_LABEL) {
        existing.set_focus()?;
        return Ok(());
    }
    let account = app.state::<Account>();
    account.login_completed.store(false, Ordering::SeqCst);

    let login_url = origin.join("/login").map_err(|_| AppError::InvalidUrl)?;
    let title = match crate::locale::current(app.state::<SettingsStore>().get().language) {
        crate::locale::Locale::En => "Sign in to Canvas",
        crate::locale::Locale::ZhCn => "登录 Canvas",
    };

    let login_window = WebviewWindowBuilder::new(app, LOGIN_LABEL, WebviewUrl::External(login_url))
        .title(title)
        .inner_size(520.0, 760.0)
        .center()
        .data_directory(login_data_dir(app)?)
        // Diagnostics for sign-in problems: hosts only (never paths or query
        // strings, which can carry tokens), and only at debug level, so release
        // logs never reveal which school or identity provider a user signs in with.
        .on_navigation(|url| {
            log::debug!(
                "login window: navigating to {}",
                url.host_str().unwrap_or("?")
            );
            true
        })
        .on_page_load(|window, payload| {
            log::debug!(
                "login window: page load {:?} on {}",
                payload.event(),
                payload.url().host_str().unwrap_or("?")
            );
            if payload.event() == PageLoadEvent::Finished {
                let url = payload.url().clone();
                tauri::async_runtime::spawn(try_complete_login(window, url));
            }
        })
        .build()?;

    let handle = app.clone();
    login_window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed)
            && !handle
                .state::<Account>()
                .login_completed
                .load(Ordering::SeqCst)
        {
            let _ = handle.emit_to(window::MAIN_LABEL, EVENT_LOGIN_CANCELLED, ());
        }
    });
    Ok(())
}

pub fn cancel_login(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LOGIN_LABEL) {
        let _ = w.close();
    }
}

/// Checks whether the page that just loaded belongs to a signed-in Canvas site.
async fn try_complete_login(window: WebviewWindow, url: Url) {
    if url.scheme() != "https" {
        return;
    }
    let Ok(origin) = Url::parse(&url.origin().ascii_serialization()) else {
        return;
    };
    // Reading cookies on the main thread deadlocks WebView2; this runs on the async runtime.
    let cookies = match window.cookies_for_url(origin.clone()) {
        Ok(cookies) => cookies,
        Err(e) => {
            log::debug!("could not read login window cookies: {e}");
            return;
        }
    };
    if !cookies
        .iter()
        .any(|c| CANVAS_SESSION_COOKIES.contains(&c.name()))
    {
        return;
    }

    let jar = Arc::new(SharedCookies::default());
    for cookie in &cookies {
        jar.insert_set_cookie(&cookie.to_string(), &origin);
    }
    let verified = match CanvasClient::new(origin.clone(), jar.clone()) {
        Ok(client) => client.verify_session().await,
        Err(e) => Err(e),
    };
    match verified {
        Ok(()) => finish_login(
            window,
            Session {
                origin,
                cookies: jar,
                gradescope_origin: None,
            },
        ),
        // Not signed in yet (still on a login or SSO page).
        Err(AppError::SessionExpired) => {}
        Err(e) => log::debug!("login probe failed: {e}"),
    }
}

fn finish_login(login_window: WebviewWindow, session: Session) {
    let app = login_window.app_handle().clone();
    let account = app.state::<Account>();
    if account.login_completed.swap(true, Ordering::SeqCst) {
        return; // Another page load already completed the login.
    }
    if let Err(e) = account.save_session(&session) {
        log::error!("could not save Canvas session: {e}");
    }
    {
        let mut inner = account.lock();
        let switched_site = inner
            .session
            .as_ref()
            .is_some_and(|old| old.origin != session.origin);
        if switched_site {
            inner.snapshot = None;
            let _ = account.store.delete(SNAPSHOT_RECORD);
        }
        inner.session = Some(session);
        inner.expired = false;
    }
    log::info!("Canvas login completed");
    let _ = login_window.close();
    window::show_main(&app);
    emit_status(&app);
}

// ---------- refresh ----------

/// Clears the busy flag when a refresh ends, even on early return.
struct RefreshGuard<'a>(&'a AtomicBool);

impl Drop for RefreshGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub async fn refresh(app: &AppHandle) -> AppResult<Snapshot> {
    let account = app.state::<Account>();
    if account.refreshing.swap(true, Ordering::SeqCst) {
        return Err(AppError::Busy);
    }
    let _guard = RefreshGuard(&account.refreshing);

    let session = account
        .lock()
        .session
        .clone()
        .ok_or(AppError::NotSignedIn)?;
    let show_unsubmittable = app.state::<SettingsStore>().get().show_unsubmittable;
    let canvas = CanvasClient::new(session.origin.clone(), session.cookies.clone())?;

    let canvas_data = match canvas.fetch(show_unsubmittable).await {
        Ok(data) => data,
        Err(AppError::SessionExpired) => {
            log::info!("Canvas session expired");
            account.lock().expired = true;
            emit_status(app);
            return Err(AppError::SessionExpired);
        }
        Err(e) => return Err(e),
    };

    let gs = gradescope::refresh(
        app,
        &canvas,
        &session.origin,
        &canvas_data.course_ids,
        session.cookies.clone(),
        session.gradescope_origin.clone(),
    )
    .await;

    let mut courses = canvas_data.courses;
    courses.extend(gs.courses);
    let mut assignments = canvas_data.assignments;
    assignments.extend(gs.assignments);
    assignments.sort_by(|a, b| a.due_at.cmp(&b.due_at).then_with(|| a.id.cmp(&b.id)));
    let snapshot = Snapshot {
        fetched_at: time::OffsetDateTime::now_utc(),
        courses,
        assignments,
        gradescope: gs.state,
    };

    let session = Session {
        gradescope_origin: gs.origin,
        ..session
    };
    let was_expired = {
        let mut inner = account.lock();
        // Skip the update if the user signed out or switched sites meanwhile.
        let still_current = inner
            .session
            .as_ref()
            .is_some_and(|s| s.origin == session.origin);
        if !still_current {
            return Ok(snapshot);
        }
        inner.session = Some(session.clone());
        inner.snapshot = Some(snapshot.clone());
        std::mem::replace(&mut inner.expired, false)
    };
    if let Err(e) = account.store.save(SNAPSHOT_RECORD, &snapshot) {
        log::warn!("could not cache snapshot: {e}");
    }
    // Saves rotated Canvas cookies and any new Gradescope session.
    if let Err(e) = account.save_session(&session) {
        log::warn!("could not update saved session: {e}");
    }
    if was_expired {
        emit_status(app);
    }
    Ok(snapshot)
}

// ---------- sign out ----------

pub fn sign_out(app: &AppHandle) -> AppResult<()> {
    cancel_login(app);
    let account = app.state::<Account>();
    *account.lock() = Inner::default();
    account.store.delete(SESSION_RECORD)?;
    account.store.delete(SNAPSHOT_RECORD)?;
    wipe_login_data(app)?;
    log::info!("signed out of Canvas");
    emit_status(app);
    Ok(())
}

/// Deletes the login window's browser data (SSO cookies, Duo device memory).
/// WebView2 can keep files locked briefly after a window closes, so on failure
/// the wipe is retried at the next startup, before any webview exists.
fn wipe_login_data(app: &AppHandle) -> AppResult<()> {
    let dir = login_data_dir(app)?;
    let marker = app.path().app_local_data_dir()?.join(WIPE_MARKER);
    match fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            log::warn!("login data is in use ({e}); it will be wiped at next startup");
            fs::write(marker, b"")?;
            Ok(())
        }
    }
}

/// Completes a wipe that was deferred by [`wipe_login_data`].
pub fn finish_pending_wipe(app: &AppHandle) {
    let Ok(base) = app.path().app_local_data_dir() else {
        return;
    };
    let marker = base.join(WIPE_MARKER);
    if marker.exists() {
        match fs::remove_dir_all(base.join(LOGIN_DATA_DIR)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                log::warn!("deferred login data wipe failed: {e}");
            }
            _ => {
                let _ = fs::remove_file(marker);
            }
        }
    }
}

/// Loads the saved session from an app data directory (developer probes only).
#[cfg(test)]
pub(crate) fn load_saved_session(dir: PathBuf) -> Option<(Arc<SharedCookies>, Option<Url>)> {
    let stored: StoredSession = SecureStore::new(dir).load(SESSION_RECORD).ok()??;
    let cookies = SharedCookies::from_json(&stored.cookies).ok()?;
    let gradescope = stored.gradescope_origin.and_then(|g| Url::parse(&g).ok());
    Some((Arc::new(cookies), gradescope))
}

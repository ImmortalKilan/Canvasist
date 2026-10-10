//! IPC commands exposed to the main window.
//!
//! Every command here must also be listed in `build.rs` and granted in
//! `capabilities/main-window.json`; otherwise the frontend cannot call it.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::account::{self, Account, AuthStatus};
use crate::canvas::discovery::{self, School};
use crate::domain::{is_gradescope_host, Snapshot};
use crate::error::{AppError, AppResult};
use crate::locale::{self, Locale};
use crate::marks::{MarkStore, Marks};
use crate::settings::{normalize_reminder_offsets, LanguagePreference, SettingsStore};
use crate::{gradescope, http, tray};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    language: LanguagePreference,
    effective_locale: Locale,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
}

fn view(language: LanguagePreference) -> SettingsView {
    SettingsView {
        language,
        effective_locale: locale::current(language),
    }
}

// ---------- settings ----------

#[tauri::command]
pub fn get_settings(store: State<'_, SettingsStore>) -> SettingsView {
    view(store.get().language)
}

#[tauri::command]
pub fn set_language(
    app: AppHandle,
    store: State<'_, SettingsStore>,
    language: LanguagePreference,
) -> AppResult<SettingsView> {
    let saved = store.update(|s| s.language = language)?;
    tray::refresh_language(&app)?;
    Ok(view(saved.language))
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> AppResult<bool> {
    Ok(app.autolaunch().is_enabled()?)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> AppResult<bool> {
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable()?;
    } else {
        launcher.disable()?;
    }
    Ok(launcher.is_enabled()?)
}

#[tauri::command]
pub fn get_app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
    }
}

// ---------- connecting to Canvas ----------

#[tauri::command]
pub async fn search_schools(query: String) -> AppResult<Vec<School>> {
    discovery::search_schools(&http::anonymous_client()?, &query).await
}

/// Normalizes a manually entered address and confirms it is a Canvas site.
#[tauri::command]
pub async fn check_canvas_url(input: String) -> AppResult<String> {
    let origin = crate::canvas::url::normalize(&input)?;
    if !discovery::looks_like_canvas(&http::anonymous_client()?, &origin).await? {
        return Err(AppError::NotCanvas);
    }
    Ok(discovery::origin_string(&origin))
}

#[tauri::command]
pub async fn start_canvas_login(app: AppHandle, origin: String) -> AppResult<()> {
    // Re-validate: the frontend is not trusted to pass a safe URL.
    let origin = crate::canvas::url::normalize(&origin)?;
    account::start_login(&app, origin)
}

#[tauri::command]
pub fn cancel_canvas_login(app: AppHandle) {
    account::cancel_login(&app);
}

/// Opens Gradescope's own sign-in page, for courses that Gradescope does not
/// open from Canvas. The site is chosen by the backend, never by the frontend.
/// Async like `start_canvas_login`: creating a window from a synchronous
/// command deadlocks on Windows.
#[tauri::command]
pub async fn start_gradescope_login(app: AppHandle) -> AppResult<()> {
    let account = app.state::<Account>();
    if !matches!(account.status(), AuthStatus::SignedIn { .. }) {
        return Err(AppError::NotSignedIn);
    }
    gradescope::login::start(&app, account.gradescope_login_origin()?)
}

#[tauri::command]
pub fn get_auth_status(account: State<'_, Account>) -> AuthStatus {
    account.status()
}

#[tauri::command]
pub fn sign_out(app: AppHandle) -> AppResult<()> {
    account::sign_out(&app)
}

// ---------- assignments ----------

#[tauri::command]
pub fn get_snapshot(account: State<'_, Account>) -> Option<Snapshot> {
    account.snapshot()
}

#[tauri::command]
pub async fn refresh(app: AppHandle) -> AppResult<Snapshot> {
    account::refresh(&app).await
}

/// Opens an assignment page in the default browser. Only https links to the
/// signed-in Canvas site or to Gradescope are allowed.
#[tauri::command]
pub fn open_external(app: AppHandle, account: State<'_, Account>, url: String) -> AppResult<()> {
    let parsed = Url::parse(&url).map_err(|_| AppError::InvalidUrl)?;
    let host = parsed.host_str().unwrap_or_default();
    let canvas_host = match account.status() {
        AuthStatus::SignedIn { origin } | AuthStatus::Expired { origin } => Url::parse(&origin)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned)),
        AuthStatus::SignedOut => None,
    };
    let allowed = parsed.scheme() == "https"
        && (canvas_host.as_deref() == Some(host) || is_gradescope_host(host));
    if !allowed {
        return Err(AppError::InvalidUrl);
    }
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}

// ---------- the user's own marks (local only) ----------

#[tauri::command]
pub fn get_marks(marks: State<'_, MarkStore>) -> Marks {
    marks.get()
}

#[tauri::command]
pub fn mark_done(marks: State<'_, MarkStore>, id: String) -> AppResult<Marks> {
    marks.mark_done(&id)
}

#[tauri::command]
pub fn dismiss(marks: State<'_, MarkStore>, id: String) -> AppResult<Marks> {
    marks.dismiss(&id)
}

#[tauri::command]
pub fn restore(marks: State<'_, MarkStore>, id: String) -> AppResult<Marks> {
    marks.restore(&id)
}

// ---------- preferences (reminders, hidden courses, display rules) ----------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    reminder_offsets_minutes: Vec<u32>,
    hidden_courses: Vec<String>,
    show_unsubmittable: bool,
}

fn preferences(store: &SettingsStore) -> Preferences {
    let s = store.get();
    Preferences {
        reminder_offsets_minutes: s.reminder_offsets_minutes,
        hidden_courses: s.hidden_courses,
        show_unsubmittable: s.show_unsubmittable,
    }
}

#[tauri::command]
pub fn get_preferences(store: State<'_, SettingsStore>) -> Preferences {
    preferences(&store)
}

#[tauri::command]
pub fn set_reminder_offsets(
    store: State<'_, SettingsStore>,
    minutes: Vec<u32>,
) -> AppResult<Preferences> {
    let minutes = normalize_reminder_offsets(minutes).ok_or(AppError::InvalidInput)?;
    store.update(|s| s.reminder_offsets_minutes = minutes)?;
    Ok(preferences(&store))
}

#[tauri::command]
pub fn set_course_hidden(
    store: State<'_, SettingsStore>,
    course_id: String,
    hidden: bool,
) -> AppResult<Preferences> {
    let valid = !course_id.is_empty()
        && course_id.len() <= 64
        && course_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b':');
    if !valid {
        return Err(AppError::InvalidInput);
    }
    store.update(|s| {
        s.hidden_courses.retain(|id| id != &course_id);
        if hidden {
            s.hidden_courses.push(course_id);
        }
    })?;
    Ok(preferences(&store))
}

/// Changing this affects which Canvas items are fetched, so the frontend
/// refreshes afterwards.
#[tauri::command]
pub fn set_show_unsubmittable(
    store: State<'_, SettingsStore>,
    show: bool,
) -> AppResult<Preferences> {
    store.update(|s| s.show_unsubmittable = show)?;
    Ok(preferences(&store))
}

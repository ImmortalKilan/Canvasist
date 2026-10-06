//! IPC commands exposed to the main window.
//!
//! Every command here must also be listed in `build.rs` and granted in
//! `capabilities/main-window.json`; otherwise the frontend cannot call it.

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::account::{self, Account, AuthStatus};
use crate::canvas::discovery::{self, School};
use crate::domain::{is_gradescope_host, Snapshot};
use crate::error::{AppError, AppResult};
use crate::locale::{self, Locale};
use crate::settings::{LanguagePreference, SettingsStore};
use crate::{http, tray};

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

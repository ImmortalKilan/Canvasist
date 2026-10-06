//! IPC commands exposed to the main window.
//!
//! Every command here must also be listed in `build.rs` and granted in
//! `capabilities/main-window.json`; otherwise the frontend cannot call it.

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::error::AppResult;
use crate::locale::{self, Locale};
use crate::settings::{LanguagePreference, SettingsStore};
use crate::tray;

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

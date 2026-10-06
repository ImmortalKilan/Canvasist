//! User preferences persisted as JSON in the app config directory.
//!
//! Settings hold no credentials or personal data, so they are stored in plain
//! text. Writes are atomic (temp file + rename) so a crash never leaves a
//! half-written file behind.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LanguagePreference {
    /// Follow the operating system's display language.
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub language: LanguagePreference,
    /// Set once the first-run autostart default has been applied, so the app
    /// never re-enables autostart after the user turned it off.
    pub autostart_initialized: bool,
    /// Also show Canvas items that need no submission or are not graded.
    pub show_unsubmittable: bool,
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Loads settings from `dir`, falling back to defaults when the file is
    /// missing. A corrupt file is moved aside rather than silently overwritten.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(FILE_NAME);
        let settings = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("settings file is corrupt ({e}); using defaults");
                let _ = fs::rename(&path, path.with_extension("json.corrupt"));
                Settings::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                log::warn!("cannot read settings file ({e}); using defaults");
                Settings::default()
            }
        };
        Self {
            path,
            current: Mutex::new(settings),
        }
    }

    pub fn get(&self) -> Settings {
        self.lock().clone()
    }

    /// Applies `change` and persists the result. The in-memory value is only
    /// replaced after the write succeeds.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> AppResult<Settings> {
        let mut guard = self.lock();
        let mut next = guard.clone();
        change(&mut next);
        write_atomic(&self.path, &next)?;
        *guard = next.clone();
        Ok(next)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Settings> {
        // Settings are plain data, so a panic elsewhere cannot leave them inconsistent.
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn write_atomic(path: &Path, settings: &Settings) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn update_persists_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        store
            .update(|s| {
                s.language = LanguagePreference::ZhCn;
                s.autostart_initialized = true;
            })
            .unwrap();

        let reloaded = SettingsStore::load(dir.path());
        assert_eq!(reloaded.get().language, LanguagePreference::ZhCn);
        assert!(reloaded.get().autostart_initialized);
        assert!(!dir.path().join("settings.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "{not json").unwrap();
        let store = SettingsStore::load(dir.path());
        assert_eq!(store.get(), Settings::default());
        assert!(dir.path().join("settings.json.corrupt").exists());
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(FILE_NAME),
            r#"{"language":"en","futureField":1}"#,
        )
        .unwrap();
        let store = SettingsStore::load(dir.path());
        assert_eq!(store.get().language, LanguagePreference::En);
        assert!(!store.get().autostart_initialized);
    }

    #[test]
    fn language_wire_format_matches_frontend() {
        let json = serde_json::to_string(&LanguagePreference::ZhCn).unwrap();
        assert_eq!(json, r#""zh-CN""#);
    }
}

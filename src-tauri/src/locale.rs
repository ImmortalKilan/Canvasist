//! Resolves the effective UI language and provides the few strings the Rust
//! side renders itself (the tray menu). All other UI text lives in the frontend.

use serde::Serialize;

use crate::settings::LanguagePreference;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Locale {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

/// Picks the UI locale from the user's preference, falling back to the system
/// locale (a BCP 47 tag such as `zh-CN` or `en-US`) and finally to English.
pub fn resolve(preference: LanguagePreference, system_locale: Option<&str>) -> Locale {
    match preference {
        LanguagePreference::En => Locale::En,
        LanguagePreference::ZhCn => Locale::ZhCn,
        LanguagePreference::System => match system_locale {
            Some(tag) if tag.to_ascii_lowercase().starts_with("zh") => Locale::ZhCn,
            _ => Locale::En,
        },
    }
}

pub fn current(preference: LanguagePreference) -> Locale {
    resolve(preference, sys_locale::get_locale().as_deref())
}

pub struct TrayText {
    pub open: &'static str,
    pub quit: &'static str,
}

pub fn tray_text(locale: Locale) -> TrayText {
    match locale {
        Locale::En => TrayText {
            open: "Open Canvasist",
            quit: "Quit",
        },
        Locale::ZhCn => TrayText {
            open: "打开 Canvasist",
            quit: "退出",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_preference_wins_over_system() {
        assert_eq!(resolve(LanguagePreference::En, Some("zh-CN")), Locale::En);
        assert_eq!(
            resolve(LanguagePreference::ZhCn, Some("en-US")),
            Locale::ZhCn
        );
    }

    #[test]
    fn system_preference_follows_any_chinese_variant() {
        for tag in ["zh-CN", "zh-TW", "zh-Hans-CN", "ZH"] {
            assert_eq!(resolve(LanguagePreference::System, Some(tag)), Locale::ZhCn);
        }
    }

    #[test]
    fn system_preference_falls_back_to_english() {
        assert_eq!(
            resolve(LanguagePreference::System, Some("fr-FR")),
            Locale::En
        );
        assert_eq!(resolve(LanguagePreference::System, None), Locale::En);
    }
}

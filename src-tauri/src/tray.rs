//! System tray icon and its localized menu.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::error::AppResult;
use crate::locale::{self, Locale};
use crate::settings::SettingsStore;
use crate::window;

const TRAY_ID: &str = "main";
const MENU_OPEN: &str = "open";
const MENU_QUIT: &str = "quit";

pub fn create(app: &AppHandle) -> AppResult<()> {
    let menu = build_menu(app, current_locale(app))?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Canvasist")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN => window::show_main(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuilds the tray menu after the UI language changes.
pub fn refresh_language(app: &AppHandle) -> AppResult<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(build_menu(app, current_locale(app))?))?;
    }
    Ok(())
}

fn current_locale(app: &AppHandle) -> Locale {
    locale::current(app.state::<SettingsStore>().get().language)
}

fn build_menu(app: &AppHandle, locale: Locale) -> AppResult<Menu<Wry>> {
    let text = locale::tray_text(locale);
    let open = MenuItem::with_id(app, MENU_OPEN, text.open, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, text.quit, true, None::<&str>)?;
    Ok(Menu::with_items(app, &[&open, &separator, &quit])?)
}

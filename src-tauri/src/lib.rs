//! Canvasist desktop application.

mod account;
mod canvas;
mod commands;
mod domain;
mod dpapi;
mod error;
mod gradescope;
mod http;
mod locale;
mod marks;
mod merge;
mod notify;
mod reminders;
mod scheduler;
mod secure_store;
mod settings;
mod tray;
mod window;

use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::account::Account;
use crate::marks::MarkStore;
use crate::reminders::ReminderLog;
use crate::secure_store::SecureStore;
use crate::settings::SettingsStore;

/// Argument the OS autostart entry launches us with; the app then starts
/// silently in the tray instead of opening its window.
const AUTOSTART_ARG: &str = "--autostart";

pub fn run() {
    let app = tauri::Builder::default()
        // Must be the first plugin so a second launch is redirected to the
        // running instance before anything else initializes.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .plugin(logger())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            app.manage(SettingsStore::load(&config_dir));

            // Must run before any webview exists, while no files are locked.
            account::finish_pending_wipe(app.handle());
            let data_dir = app.path().app_local_data_dir()?;
            app.manage(Account::load(SecureStore::new(data_dir.clone())));
            app.manage(MarkStore::load(SecureStore::new(data_dir.clone())));
            app.manage(ReminderLog::load(SecureStore::new(data_dir)));

            apply_first_run_defaults(app.handle());
            tray::create(app.handle())?;
            scheduler::start(app.handle().clone());

            let started_by_os = std::env::args().any(|arg| arg == AUTOSTART_ARG);
            if !started_by_os {
                window::show_main(app.handle());
            }
            log::info!(
                "Canvasist {} started (autostart: {started_by_os})",
                app.package_info().version
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_language,
            commands::get_autostart,
            commands::set_autostart,
            commands::get_app_info,
            commands::search_schools,
            commands::check_canvas_url,
            commands::start_canvas_login,
            commands::cancel_canvas_login,
            commands::start_gradescope_login,
            commands::get_auth_status,
            commands::sign_out,
            commands::get_snapshot,
            commands::refresh,
            commands::open_external,
            commands::get_marks,
            commands::mark_done,
            commands::dismiss,
            commands::restore,
            commands::get_preferences,
            commands::set_reminder_offsets,
            commands::set_course_hidden,
            commands::set_show_unsubmittable,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the Canvasist application");

    app.run(|_app, event| {
        // Closing the last window keeps Canvasist alive in the tray. Only an
        // explicit exit (tray "Quit", which carries an exit code) ends the process.
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            api.prevent_exit();
        }
    });
}

fn logger() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

    // Logs must never contain credentials, cookies or personal data
    // (course names, assignment titles, grades).
    let own_level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .target(Target::new(TargetKind::LogDir { file_name: None }))
        .target(Target::new(TargetKind::Stdout))
        // Third-party crates only report problems; our own crate logs in more detail.
        .level(log::LevelFilter::Warn)
        .level_for("canvasist_lib", own_level)
        .max_file_size(512 * 1024)
        .rotation_strategy(RotationStrategy::KeepOne)
        .build()
}

/// Turns autostart on the first time a release build runs. Debug builds never
/// register themselves with the OS, and a user's later choice is never overridden.
fn apply_first_run_defaults(app: &AppHandle) {
    let store = app.state::<SettingsStore>();
    if store.get().autostart_initialized || cfg!(debug_assertions) {
        return;
    }
    if let Err(e) = app.autolaunch().enable() {
        log::warn!("could not enable autostart on first run: {e}");
        return;
    }
    if let Err(e) = store.update(|s| s.autostart_initialized = true) {
        log::warn!("could not record first-run autostart default: {e}");
    }
}

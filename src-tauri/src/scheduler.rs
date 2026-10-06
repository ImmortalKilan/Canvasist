//! Background work while Canvasist sits in the tray: hourly refreshes,
//! a refresh after the computer wakes, and deadline reminders.
//!
//! Everything runs on one lightweight async task; no window or browser is
//! involved, which keeps the idle footprint small.

use std::collections::HashSet;
use std::time::{Duration as StdDuration, SystemTime};

use tauri::{AppHandle, Manager};
use time::{Duration, OffsetDateTime};

use crate::account::{self, Account, AuthStatus};
use crate::error::AppError;
use crate::marks::MarkStore;
use crate::notify;
use crate::reminders::{self, ReminderLog, Rules};
use crate::settings::SettingsStore;

/// How often the task wakes up to check reminders.
const TICK: StdDuration = StdDuration::from_secs(30);
/// Regular refresh interval (discussion Round 2: hourly).
const REFRESH_INTERVAL: StdDuration = StdDuration::from_secs(60 * 60);
/// After a failed refresh (e.g. offline), try again sooner than an hour.
const RETRY_AFTER: StdDuration = StdDuration::from_secs(5 * 60);
/// A wall-clock gap this much larger than a tick means the computer slept.
const WAKE_GAP: StdDuration = StdDuration::from_secs(3 * 60);

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut next_refresh = SystemTime::now();
        let mut last_tick = SystemTime::now();
        loop {
            let now = SystemTime::now();
            let woke = now
                .duration_since(last_tick)
                .is_ok_and(|gap| gap > TICK + WAKE_GAP);
            last_tick = now;
            if woke {
                log::info!("scheduler: system resumed, refreshing");
            }

            let signed_in = matches!(app.state::<Account>().status(), AuthStatus::SignedIn { .. });
            if signed_in && (woke || now >= next_refresh) {
                next_refresh = match account::refresh(&app).await {
                    Ok(_) | Err(AppError::Busy) => now + REFRESH_INTERVAL,
                    // Expired sessions are reported by `account::refresh` itself.
                    Err(AppError::SessionExpired) => now + REFRESH_INTERVAL,
                    Err(e) => {
                        log::warn!("scheduled refresh failed: {e}");
                        now + RETRY_AFTER
                    }
                };
            }

            check_reminders(&app);
            tokio::time::sleep(TICK).await;
        }
    });
}

/// Sends any reminders that are due according to the latest snapshot.
fn check_reminders(app: &AppHandle) {
    let account = app.state::<Account>();
    if matches!(account.status(), AuthStatus::SignedOut) {
        return;
    }
    let Some(snapshot) = account.snapshot() else {
        return;
    };
    let settings = app.state::<SettingsStore>().get();
    let offsets: Vec<Duration> = settings
        .reminder_offsets_minutes
        .iter()
        .map(|&m| Duration::minutes(i64::from(m)))
        .collect();
    let hidden: HashSet<String> = settings.hidden_courses.into_iter().collect();
    let marks = app.state::<MarkStore>().get();
    let now = OffsetDateTime::now_utc();

    let log_state = app.state::<ReminderLog>();
    let due = {
        let mut log = log_state.lock();
        let before = log.clone();
        let rules = Rules {
            offsets: &offsets,
            hidden_courses: &hidden,
            marks: &marks,
        };
        let due = reminders::due_reminders(&snapshot, &rules, &mut log, now);
        if *log != before {
            if let Err(e) = log_state.save(&log) {
                log::warn!("could not save reminder log: {e}");
            }
        }
        due
    };
    if !due.is_empty() {
        log::info!("sending {} reminder(s)", due.len());
    }
    for reminder in &due {
        notify::send_reminder(app, reminder, now);
    }
}

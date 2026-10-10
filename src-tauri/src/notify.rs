//! Windows toast notifications for reminders and sign-in problems.
//!
//! Clicking a toast opens the main window: directly through the toast's
//! activation handler, or, if Windows relaunches the app from its Start Menu
//! shortcut instead, through the single-instance handler.

use tauri::{AppHandle, Manager};
use time::OffsetDateTime;

use crate::locale::{self, Locale};
use crate::reminders::{Reminder, ReminderKind};
use crate::settings::SettingsStore;

fn locale(app: &AppHandle) -> Locale {
    locale::current(app.state::<SettingsStore>().get().language)
}

fn duration_text(locale: Locale, minutes: i64) -> String {
    let minutes = minutes.max(0);
    let (days, hours, mins) = (minutes / 1440, minutes / 60, minutes % 60);
    match locale {
        Locale::En if hours >= 48 => format!("{days} days"),
        Locale::En if hours > 0 => format!("{hours} h {mins} min"),
        Locale::En => format!("{mins} min"),
        Locale::ZhCn if hours >= 48 => format!("{days} 天"),
        Locale::ZhCn if hours > 0 => format!("{hours} 小时 {mins} 分"),
        Locale::ZhCn => format!("{mins} 分钟"),
    }
}

/// The toast's two lines for a reminder.
fn reminder_text(locale: Locale, r: &Reminder, now: OffsetDateTime) -> (String, String) {
    let title = if r.course_code.is_empty() {
        r.title.clone()
    } else {
        format!("{} · {}", r.course_code, r.title)
    };
    let left = duration_text(locale, (r.deadline - now).whole_minutes());
    let body = match (locale, r.kind) {
        (Locale::En, ReminderKind::Deadline) => format!("Due in {left}"),
        (Locale::En, ReminderKind::LateDeadline) => format!("Late deadline in {left}"),
        (Locale::ZhCn, ReminderKind::Deadline) => format!("距截止还剩 {left}"),
        (Locale::ZhCn, ReminderKind::LateDeadline) => format!("距迟交截止还剩 {left}"),
    };
    (title, body)
}

pub fn send_reminder(app: &AppHandle, reminder: &Reminder, now: OffsetDateTime) {
    let (title, body) = reminder_text(locale(app), reminder, now);
    show(app, &title, &body);
}

pub fn send_session_expired(app: &AppHandle) {
    let body = match locale(app) {
        Locale::En => "Your Canvas session has expired. Open Canvasist to sign in again.",
        Locale::ZhCn => "Canvas 登录已过期，请打开 Canvasist 重新登录。",
    };
    show(app, "Canvasist", body);
}

pub fn send_gradescope_expired(app: &AppHandle) {
    let body = match locale(app) {
        Locale::En => "Your Gradescope sign-in has expired. Open Canvasist to sign in again.",
        Locale::ZhCn => "Gradescope 登录已过期，请打开 Canvasist 重新登录。",
    };
    show(app, "Canvasist", body);
}

#[cfg(windows)]
fn show(app: &AppHandle, title: &str, body: &str) {
    use tauri_winrt_notification::Toast;

    // Installed builds are registered under the bundle identifier by the
    // installer's Start Menu shortcut. Unregistered development builds must
    // borrow PowerShell's ID, so their toasts are attributed to PowerShell.
    let app_id = if cfg!(debug_assertions) {
        Toast::POWERSHELL_APP_ID.to_owned()
    } else {
        app.config().identifier.clone()
    };
    let handle = app.clone();
    let result = Toast::new(&app_id)
        .title(title)
        .text1(body)
        .on_activated(move |_| {
            crate::window::show_main(&handle);
            Ok(())
        })
        .show();
    // Never log the text: it contains course and assignment names.
    if let Err(e) = result {
        log::warn!("could not show a notification: {e}");
    }
}

#[cfg(not(windows))]
fn show(_app: &AppHandle, _title: &str, _body: &str) {
    log::warn!("notifications are only implemented on Windows");
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;
    use time::Duration;

    const NOW: OffsetDateTime = datetime!(2026-10-07 12:00 UTC);

    fn reminder(minutes_left: i64, kind: ReminderKind) -> Reminder {
        Reminder {
            assignment_id: "1".into(),
            course_code: "ABC 1".into(),
            title: "Homework".into(),
            deadline: NOW + Duration::minutes(minutes_left),
            kind,
        }
    }

    #[test]
    fn english_text() {
        let (title, body) = reminder_text(Locale::En, &reminder(179, ReminderKind::Deadline), NOW);
        assert_eq!(title, "ABC 1 · Homework");
        assert_eq!(body, "Due in 2 h 59 min");
        let (_, body) = reminder_text(Locale::En, &reminder(45, ReminderKind::LateDeadline), NOW);
        assert_eq!(body, "Late deadline in 45 min");
        let (_, body) = reminder_text(Locale::En, &reminder(3 * 1440, ReminderKind::Deadline), NOW);
        assert_eq!(body, "Due in 3 days");
    }

    #[test]
    fn chinese_text() {
        let (_, body) = reminder_text(Locale::ZhCn, &reminder(1440, ReminderKind::Deadline), NOW);
        assert_eq!(body, "距截止还剩 24 小时 0 分");
        let (_, body) = reminder_text(Locale::ZhCn, &reminder(30, ReminderKind::LateDeadline), NOW);
        assert_eq!(body, "距迟交截止还剩 30 分钟");
    }

    #[test]
    fn missing_course_code_uses_title_only() {
        let mut r = reminder(60, ReminderKind::Deadline);
        r.course_code.clear();
        assert_eq!(reminder_text(Locale::En, &r, NOW).0, "Homework");
    }
}

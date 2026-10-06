//! Deciding which deadline reminders are due (lighthouse G4).
//!
//! Rules:
//! - Open work (not submitted, not marked done or dismissed, course not hidden)
//!   is reminded at each configured offset before its deadline.
//! - A reminder whose moment passed while the computer was asleep or off is
//!   sent late, as long as the deadline is still ahead. If several passed at
//!   once, only one (the most urgent) is sent.
//! - After a missed deadline, work with a late deadline gets one more reminder
//!   shortly before the late deadline.
//! - Each reminder is sent once per assignment and deadline; if the deadline
//!   changes, reminders re-arm.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::domain::{Snapshot, Status};
use crate::error::AppResult;
use crate::marks::Marks;
use crate::secure_store::SecureStore;

/// How long before a late deadline its single reminder fires.
pub const LATE_REMINDER: Duration = Duration::hours(3);
/// Sent-log entries are kept this long after their deadline, then pruned.
const LOG_RETENTION: Duration = Duration::days(30);
const RECORD: &str = "reminders-sent";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderKind {
    Deadline,
    LateDeadline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reminder {
    pub assignment_id: String,
    pub course_code: String,
    pub title: String,
    pub deadline: OffsetDateTime,
    pub kind: ReminderKind,
}

/// Which reminders were already sent: key -> the deadline it belonged to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SentLog {
    #[serde(with = "unix_map")]
    sent: BTreeMap<String, OffsetDateTime>,
}

impl SentLog {
    fn key(id: &str, deadline: OffsetDateTime, slot: &str) -> String {
        format!("{id}|{}|{slot}", deadline.unix_timestamp())
    }

    fn contains(&self, key: &str) -> bool {
        self.sent.contains_key(key)
    }

    fn insert(&mut self, key: String, deadline: OffsetDateTime) {
        self.sent.insert(key, deadline);
    }

    fn prune(&mut self, now: OffsetDateTime) {
        self.sent
            .retain(|_, deadline| now - *deadline < LOG_RETENTION);
    }
}

pub struct Rules<'a> {
    /// Offsets before the deadline, in any order.
    pub offsets: &'a [Duration],
    pub hidden_courses: &'a HashSet<String>,
    pub marks: &'a Marks,
}

fn is_open(status: Status) -> bool {
    matches!(status, Status::NotSubmitted | Status::Missing)
}

/// Returns the reminders due at `now` and records them in `sent`.
pub fn due_reminders(
    snapshot: &Snapshot,
    rules: &Rules<'_>,
    sent: &mut SentLog,
    now: OffsetDateTime,
) -> Vec<Reminder> {
    let codes: BTreeMap<&str, &str> = snapshot
        .courses
        .iter()
        .map(|c| (c.id.as_str(), c.code.as_str()))
        .collect();
    let mut out = Vec::new();

    for a in &snapshot.assignments {
        if !is_open(a.status)
            || rules.hidden_courses.contains(&a.course_id)
            || rules.marks.done.contains_key(&a.id)
            || rules.marks.dismissed.contains_key(&a.id)
        {
            continue;
        }
        let reminder = |deadline, kind| Reminder {
            assignment_id: a.id.clone(),
            course_code: codes
                .get(a.course_id.as_str())
                .copied()
                .unwrap_or_default()
                .to_owned(),
            title: a.title.clone(),
            deadline,
            kind,
        };

        if now < a.due_at {
            let passed: Vec<Duration> = rules
                .offsets
                .iter()
                .copied()
                .filter(|&o| now >= a.due_at - o)
                .collect();
            let keys: Vec<String> = passed
                .iter()
                .map(|o| SentLog::key(&a.id, a.due_at, &o.whole_minutes().to_string()))
                .collect();
            if keys.iter().any(|k| !sent.contains(k)) {
                // Mark every passed offset as sent so a catch-up never fires twice.
                for key in keys {
                    sent.insert(key, a.due_at);
                }
                out.push(reminder(a.due_at, ReminderKind::Deadline));
            }
        } else if let Some(late) = a.late_due_at.filter(|&late| now < late) {
            let key = SentLog::key(&a.id, late, "late");
            if now >= late - LATE_REMINDER && !sent.contains(&key) {
                sent.insert(key, late);
                out.push(reminder(late, ReminderKind::LateDeadline));
            }
        }
    }
    sent.prune(now);
    out
}

/// The persisted sent-log (encrypted like everything else on disk).
pub struct ReminderLog {
    store: SecureStore,
    log: Mutex<SentLog>,
}

impl ReminderLog {
    pub fn load(store: SecureStore) -> Self {
        let log = store.load(RECORD).unwrap_or(None).unwrap_or_default();
        Self {
            store,
            log: Mutex::new(log),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, SentLog> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn save(&self, log: &SentLog) -> AppResult<()> {
        self.store.save(RECORD, log)
    }

    pub fn clear(&self) -> AppResult<()> {
        *self.lock() = SentLog::default();
        self.store.delete(RECORD)
    }
}

/// Stores deadlines as Unix timestamps.
mod unix_map {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use time::OffsetDateTime;

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<String, OffsetDateTime>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        map.iter()
            .map(|(k, v)| (k, v.unix_timestamp()))
            .collect::<BTreeMap<_, _>>()
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<String, OffsetDateTime>, D::Error> {
        BTreeMap::<String, i64>::deserialize(deserializer)?
            .into_iter()
            .map(|(k, v)| {
                OffsetDateTime::from_unix_timestamp(v)
                    .map(|t| (k, t))
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Assignment, Course, GradescopeState, Source, SubmissionKind};
    use time::macros::datetime;

    const DUE: OffsetDateTime = datetime!(2026-10-10 06:59 UTC);
    const OFFSETS: [Duration; 2] = [Duration::hours(24), Duration::hours(3)];

    fn assignment(id: &str, status: Status) -> Assignment {
        Assignment {
            id: id.into(),
            course_id: "c1".into(),
            title: "Homework".into(),
            due_at: DUE,
            late_due_at: None,
            url: None,
            kind: SubmissionKind::Online,
            status,
            source: Source::Canvas,
            links_to_gradescope: false,
            also_in_canvas: false,
            canvas_due_at: None,
        }
    }

    fn snapshot(assignments: Vec<Assignment>) -> Snapshot {
        Snapshot {
            fetched_at: DUE,
            courses: vec![Course {
                id: "c1".into(),
                code: "ABC 1".into(),
                name: "Course".into(),
                source: Source::Canvas,
            }],
            assignments,
            gradescope: GradescopeState::Ok,
        }
    }

    fn run(snap: &Snapshot, sent: &mut SentLog, now: OffsetDateTime) -> Vec<Reminder> {
        run_with(snap, sent, now, &HashSet::new(), &Marks::default())
    }

    fn run_with(
        snap: &Snapshot,
        sent: &mut SentLog,
        now: OffsetDateTime,
        hidden: &HashSet<String>,
        marks: &Marks,
    ) -> Vec<Reminder> {
        let rules = Rules {
            offsets: &OFFSETS,
            hidden_courses: hidden,
            marks,
        };
        due_reminders(snap, &rules, sent, now)
    }

    #[test]
    fn fires_once_at_each_offset() {
        let snap = snapshot(vec![assignment("1", Status::NotSubmitted)]);
        let mut sent = SentLog::default();
        assert!(run(&snap, &mut sent, DUE - Duration::hours(25)).is_empty());

        let first = run(&snap, &mut sent, DUE - Duration::hours(24));
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].course_code, "ABC 1");
        assert_eq!(first[0].kind, ReminderKind::Deadline);
        assert!(
            run(&snap, &mut sent, DUE - Duration::hours(20)).is_empty(),
            "no repeat"
        );

        assert_eq!(run(&snap, &mut sent, DUE - Duration::hours(3)).len(), 1);
        assert!(run(&snap, &mut sent, DUE - Duration::hours(1)).is_empty());
        assert!(
            run(&snap, &mut sent, DUE + Duration::hours(1)).is_empty(),
            "nothing after due"
        );
    }

    #[test]
    fn catch_up_sends_a_single_reminder() {
        // Asleep through both offsets; woke up 2 hours before the deadline.
        let snap = snapshot(vec![assignment("1", Status::NotSubmitted)]);
        let mut sent = SentLog::default();
        assert_eq!(run(&snap, &mut sent, DUE - Duration::hours(2)).len(), 1);
        assert!(run(&snap, &mut sent, DUE - Duration::hours(1)).is_empty());
    }

    #[test]
    fn missed_deadline_is_not_caught_up() {
        let snap = snapshot(vec![assignment("1", Status::Missing)]);
        let mut sent = SentLog::default();
        assert!(run(&snap, &mut sent, DUE + Duration::minutes(5)).is_empty());
    }

    #[test]
    fn finished_hidden_or_marked_work_is_skipped() {
        let mut sent = SentLog::default();
        let now = DUE - Duration::hours(2);
        for status in [
            Status::Submitted,
            Status::Late,
            Status::Graded,
            Status::Excused,
        ] {
            assert!(run(&snapshot(vec![assignment("1", status)]), &mut sent, now).is_empty());
        }
        let snap = snapshot(vec![assignment("1", Status::NotSubmitted)]);
        let hidden: HashSet<String> = ["c1".to_owned()].into();
        assert!(run_with(&snap, &mut sent, now, &hidden, &Marks::default()).is_empty());
        let mut marks = Marks::default();
        marks.dismissed.insert("1".into(), now);
        assert!(run_with(&snap, &mut sent, now, &HashSet::new(), &marks).is_empty());
        let mut marks = Marks::default();
        marks.done.insert("1".into(), now);
        assert!(run_with(&snap, &mut sent, now, &HashSet::new(), &marks).is_empty());
    }

    #[test]
    fn late_deadline_gets_one_reminder() {
        let mut a = assignment("1", Status::Missing);
        let late = DUE + Duration::days(2);
        a.late_due_at = Some(late);
        let snap = snapshot(vec![a]);
        let mut sent = SentLog::default();
        assert!(run(&snap, &mut sent, late - Duration::hours(4)).is_empty());
        let r = run(&snap, &mut sent, late - Duration::hours(2));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, ReminderKind::LateDeadline);
        assert_eq!(r[0].deadline, late);
        assert!(run(&snap, &mut sent, late - Duration::hours(1)).is_empty());
        assert!(run(&snap, &mut sent, late + Duration::hours(1)).is_empty());
    }

    #[test]
    fn changed_deadline_rearms() {
        let mut sent = SentLog::default();
        let snap = snapshot(vec![assignment("1", Status::NotSubmitted)]);
        assert_eq!(run(&snap, &mut sent, DUE - Duration::hours(2)).len(), 1);

        let mut moved = assignment("1", Status::NotSubmitted);
        moved.due_at = DUE + Duration::days(1);
        let snap = snapshot(vec![moved]);
        assert_eq!(run(&snap, &mut sent, DUE + Duration::hours(22)).len(), 1);
    }

    #[test]
    fn log_round_trips_and_prunes() {
        let mut sent = SentLog::default();
        sent.insert("a".into(), DUE);
        let json = serde_json::to_string(&sent).unwrap();
        assert_eq!(serde_json::from_str::<SentLog>(&json).unwrap(), sent);
        sent.prune(DUE + Duration::days(31));
        assert!(sent.sent.is_empty());
    }
}

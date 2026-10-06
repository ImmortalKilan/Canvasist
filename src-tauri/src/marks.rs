//! The user's own annotations on assignments: "marked as done" and
//! "dismissed". They live only on this computer (encrypted with DPAPI) and
//! never change anything on Canvas or Gradescope.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::error::{AppError, AppResult};
use crate::secure_store::SecureStore;

const RECORD: &str = "marks";
/// Marks for assignments from long-finished terms are dropped after this.
const RETENTION: Duration = Duration::days(240);
const MAX_ID_LEN: usize = 200;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Marks {
    /// Assignment ID -> when the user marked it as done.
    #[serde(with = "timestamp_map")]
    pub done: BTreeMap<String, OffsetDateTime>,
    /// Assignment ID -> when the user dismissed it.
    #[serde(with = "timestamp_map")]
    pub dismissed: BTreeMap<String, OffsetDateTime>,
}

impl Marks {
    fn prune(&mut self, now: OffsetDateTime) {
        let keep = |at: &mut OffsetDateTime| now - *at < RETENTION;
        self.done.retain(|_, at| keep(at));
        self.dismissed.retain(|_, at| keep(at));
    }
}

pub struct MarkStore {
    store: SecureStore,
    marks: Mutex<Marks>,
}

impl MarkStore {
    pub fn load(store: SecureStore) -> Self {
        let mut marks: Marks = store
            .load(RECORD)
            .unwrap_or_else(|e| {
                log::warn!("could not read marks: {e}");
                None
            })
            .unwrap_or_default();
        marks.prune(OffsetDateTime::now_utc());
        Self {
            store,
            marks: Mutex::new(marks),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Marks> {
        self.marks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn get(&self) -> Marks {
        self.lock().clone()
    }

    /// Applies `change` to the marks for `id` and persists the result. The
    /// in-memory state only changes once the write succeeded.
    fn update(&self, id: &str, change: impl FnOnce(&mut Marks, String)) -> AppResult<Marks> {
        validate_id(id)?;
        let mut guard = self.lock();
        let mut next = guard.clone();
        change(&mut next, id.to_owned());
        self.store.save(RECORD, &next)?;
        *guard = next.clone();
        Ok(next)
    }

    pub fn mark_done(&self, id: &str) -> AppResult<Marks> {
        self.update(id, |m, id| {
            m.dismissed.remove(&id);
            m.done.insert(id, OffsetDateTime::now_utc());
        })
    }

    pub fn dismiss(&self, id: &str) -> AppResult<Marks> {
        self.update(id, |m, id| {
            m.done.remove(&id);
            m.dismissed.insert(id, OffsetDateTime::now_utc());
        })
    }

    /// Undoes either mark.
    pub fn restore(&self, id: &str) -> AppResult<Marks> {
        self.update(id, |m, id| {
            m.done.remove(&id);
            m.dismissed.remove(&id);
        })
    }

    /// Deletes all marks (used by sign-out).
    pub fn clear(&self) -> AppResult<()> {
        let mut guard = self.lock();
        self.store.delete(RECORD)?;
        *guard = Marks::default();
        Ok(())
    }
}

/// IDs come from the frontend, so only the shapes Canvasist produces are accepted.
fn validate_id(id: &str) -> AppResult<()> {
    let ok = !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b':' || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(AppError::InvalidInput)
    }
}

/// Serializes `BTreeMap<String, OffsetDateTime>` with RFC 3339 timestamps.
mod timestamp_map {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<String, OffsetDateTime>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut out = BTreeMap::new();
        for (id, at) in map {
            out.insert(id, at.format(&Rfc3339).map_err(serde::ser::Error::custom)?);
        }
        out.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<String, OffsetDateTime>, D::Error> {
        let raw = BTreeMap::<String, String>::deserialize(deserializer)?;
        raw.into_iter()
            .map(|(id, at)| {
                OffsetDateTime::parse(&at, &Rfc3339)
                    .map(|t| (id, t))
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, MarkStore) {
        let dir = tempfile::tempdir().unwrap();
        let marks = MarkStore::load(SecureStore::new(dir.path().into()));
        (dir, marks)
    }

    #[test]
    fn marks_are_exclusive_and_restorable() {
        let (_dir, marks) = store();
        let m = marks.mark_done("123").unwrap();
        assert!(m.done.contains_key("123"));
        let m = marks.dismiss("123").unwrap();
        assert!(!m.done.contains_key("123") && m.dismissed.contains_key("123"));
        let m = marks.restore("123").unwrap();
        assert!(m.done.is_empty() && m.dismissed.is_empty());
    }

    #[test]
    fn marks_persist_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        MarkStore::load(SecureStore::new(dir.path().into()))
            .dismiss("gs:101:5002")
            .unwrap();
        let reloaded = MarkStore::load(SecureStore::new(dir.path().into()));
        assert!(reloaded.get().dismissed.contains_key("gs:101:5002"));
    }

    #[test]
    fn rejects_unexpected_ids() {
        let (_dir, marks) = store();
        for bad in ["", "../etc", "a b", "<script>", &"x".repeat(MAX_ID_LEN + 1)] {
            assert!(marks.mark_done(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn clear_removes_everything() {
        let (dir, marks) = store();
        marks.mark_done("1").unwrap();
        marks.clear().unwrap();
        assert_eq!(marks.get(), Marks::default());
        assert!(!dir.path().join("marks.bin").exists());
    }

    #[test]
    fn old_marks_are_pruned() {
        let now = OffsetDateTime::now_utc();
        let mut m = Marks::default();
        m.done.insert("old".into(), now - Duration::days(300));
        m.done.insert("new".into(), now - Duration::days(3));
        m.prune(now);
        assert_eq!(m.done.keys().collect::<Vec<_>>(), ["new"]);
    }
}

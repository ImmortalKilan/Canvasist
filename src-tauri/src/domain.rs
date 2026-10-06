//! Source-neutral data model shown to the user: courses and assignments from
//! Canvas and Gradescope, and the snapshot that bundles them.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Canvas,
    Gradescope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    /// Unique across sources (Gradescope IDs carry a `gs:` prefix).
    pub id: String,
    /// Short label such as "CSE 110"; falls back to the full name.
    pub code: String,
    pub name: String,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    NotSubmitted,
    Submitted,
    Late,
    Graded,
    /// Past the deadline with nothing submitted; stays visible until dismissed.
    Missing,
    /// The instructor exempted the student.
    Excused,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SubmissionKind {
    /// Submitted through Canvas, Gradescope or another online tool.
    Online,
    /// Handed in on paper; can only be marked done manually.
    OnPaper,
    /// Nothing to hand in (e.g. an exam score entered by the instructor).
    NoSubmission,
    NotGraded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    /// Unique across sources (Gradescope IDs carry a `gs:` prefix).
    pub id: String,
    pub course_id: String,
    pub title: String,
    #[serde(with = "time::serde::rfc3339")]
    pub due_at: OffsetDateTime,
    /// Gradescope's optional late-submission deadline.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub late_due_at: Option<OffsetDateTime>,
    pub url: Option<String>,
    pub kind: SubmissionKind,
    pub status: Status,
    pub source: Source,
    /// True when a Canvas assignment launches Gradescope (used to merge duplicates).
    pub links_to_gradescope: bool,
}

/// Outcome of the Gradescope part of a refresh. Gradescope problems never block
/// the Canvas list; the UI uses this to explain what is missing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GradescopeState {
    Ok,
    /// No current Canvas course links to Gradescope.
    NotLinked,
    /// Connecting to Gradescope needs a fresh Canvas sign-in.
    NeedsCanvasLogin,
    /// Gradescope could not be reached or understood this time.
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
    pub courses: Vec<Course>,
    pub assignments: Vec<Assignment>,
    pub gradescope: GradescopeState,
}

pub fn is_gradescope_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    ["gradescope.com", "gradescope.ca", "gradescope.eu"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradescope_hosts() {
        assert!(is_gradescope_host("www.gradescope.com"));
        assert!(is_gradescope_host("gradescope.ca"));
        assert!(is_gradescope_host("WWW.GRADESCOPE.EU"));
        assert!(!is_gradescope_host("notgradescope.com"));
        assert!(!is_gradescope_host("gradescope.com.evil.example"));
    }
}

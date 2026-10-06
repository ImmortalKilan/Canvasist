//! Canvas API payloads and the rules that turn them into Canvasist assignments.

use serde::{Deserialize, Deserializer};
use time::OffsetDateTime;

use crate::domain::{is_gradescope_host, Assignment, Course, Source, Status, SubmissionKind};

/// Canvas sends `null` for many booleans and lists; treat it like a missing field.
fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

// ---------- raw API payloads (only the fields we use) ----------

#[derive(Debug, Clone, Deserialize)]
pub struct RawCourse {
    pub id: String,
    pub name: Option<String>,
    pub course_code: Option<String>,
    pub term: Option<RawTerm>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub start_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub end_at: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "null_default")]
    pub access_restricted_by_date: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawTerm {
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub start_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub end_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawAssignment {
    pub id: String,
    pub name: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub due_at: Option<OffsetDateTime>,
    pub html_url: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub submission_types: Vec<String>,
    pub submission: Option<RawSubmission>,
    pub external_tool_tag_attributes: Option<RawExternalTool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawSubmission {
    pub workflow_state: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub submitted_at: Option<OffsetDateTime>,
    #[serde(default, deserialize_with = "null_default")]
    pub late: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub missing: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub excused: bool,
    pub score: Option<f64>,
    pub grade: Option<String>,
}

/// A course navigation tab (`/api/v1/courses/:id/tabs`).
#[derive(Debug, Clone, Deserialize)]
pub struct RawTab {
    pub html_url: Option<String>,
    pub label: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawExternalTool {
    pub url: Option<String>,
}

// ---------- rules ----------

/// Absolute URL of a Gradescope external-tool tab on the same Canvas site.
pub fn gradescope_tab_url(tab: &RawTab, origin: &url::Url) -> Option<url::Url> {
    let is_external = tab.kind.as_deref() == Some("external");
    let is_gradescope = tab
        .label
        .as_deref()
        .is_some_and(|l| l.to_ascii_lowercase().contains("gradescope"));
    if !is_external || !is_gradescope {
        return None;
    }
    let url = origin.join(tab.html_url.as_deref()?).ok()?;
    (url.origin() == origin.origin()).then_some(url)
}

/// A course counts as current when today falls inside its term dates. Without
/// term dates, the course's own dates are used; with no dates at all it is kept
/// (the user can hide it).
pub fn is_current_course(course: &RawCourse, now: OffsetDateTime) -> bool {
    if course.access_restricted_by_date {
        return false;
    }
    let term = course.term.clone().unwrap_or_default();
    let (start, end) = if term.start_at.is_some() || term.end_at.is_some() {
        (term.start_at, term.end_at)
    } else {
        (course.start_at, course.end_at)
    };
    start.is_none_or(|s| s <= now) && end.is_none_or(|e| now <= e)
}

pub fn to_course(raw: &RawCourse) -> Course {
    let name = raw
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Untitled course")
        .to_owned();
    let code = raw
        .course_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| name.clone(), str::to_owned);
    Course {
        id: raw.id.clone(),
        code,
        name,
        source: Source::Canvas,
    }
}

pub fn submission_kind(types: &[String]) -> SubmissionKind {
    let has = |t: &str| types.iter().any(|x| x == t);
    if has("not_graded") {
        SubmissionKind::NotGraded
    } else if types.is_empty() || types.iter().all(|t| t == "none") {
        SubmissionKind::NoSubmission
    } else if types.iter().all(|t| t == "on_paper" || t == "none") {
        SubmissionKind::OnPaper
    } else {
        SubmissionKind::Online
    }
}

pub fn status(
    submission: Option<&RawSubmission>,
    due_at: OffsetDateTime,
    now: OffsetDateTime,
) -> Status {
    let sub = submission.cloned().unwrap_or_default();
    let state = sub.workflow_state.as_deref().unwrap_or("unsubmitted");
    if sub.excused {
        return Status::Excused;
    }
    let has_grade = sub.score.is_some() || sub.grade.as_deref().is_some_and(|g| !g.is_empty());
    if state == "graded" && has_grade {
        return Status::Graded;
    }
    if sub.submitted_at.is_some() || matches!(state, "submitted" | "pending_review") {
        return if sub.late {
            Status::Late
        } else {
            Status::Submitted
        };
    }
    if sub.missing || now > due_at {
        Status::Missing
    } else {
        Status::NotSubmitted
    }
}

/// Applies the display rules from the lighthouse: assignments without a due
/// date are never shown, and "no submission" / "not graded" items only when the
/// user opts in.
pub fn to_assignment(
    raw: &RawAssignment,
    course_id: &str,
    now: OffsetDateTime,
    show_unsubmittable: bool,
) -> Option<Assignment> {
    let due_at = raw.due_at?;
    let kind = submission_kind(&raw.submission_types);
    if !show_unsubmittable
        && matches!(
            kind,
            SubmissionKind::NoSubmission | SubmissionKind::NotGraded
        )
    {
        return None;
    }
    let links_to_gradescope = raw
        .external_tool_tag_attributes
        .as_ref()
        .and_then(|t| t.url.as_deref())
        .and_then(|u| url::Url::parse(u).ok())
        .and_then(|u| u.host_str().map(is_gradescope_host))
        .unwrap_or(false);
    Some(Assignment {
        id: raw.id.clone(),
        course_id: course_id.to_owned(),
        title: raw
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Untitled assignment")
            .to_owned(),
        due_at,
        url: raw.html_url.clone().filter(|u| u.starts_with("https://")),
        kind,
        status: status(raw.submission.as_ref(), due_at, now),
        late_due_at: None,
        source: Source::Canvas,
        links_to_gradescope,
        also_in_canvas: false,
        canvas_due_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-10-06 12:00 UTC);

    fn course(term: Option<(Option<OffsetDateTime>, Option<OffsetDateTime>)>) -> RawCourse {
        RawCourse {
            id: "1".into(),
            name: Some("Software Engineering".into()),
            course_code: Some("CSE 110".into()),
            term: term.map(|(start_at, end_at)| RawTerm { start_at, end_at }),
            start_at: None,
            end_at: None,
            access_restricted_by_date: false,
        }
    }

    fn sub(state: &str) -> RawSubmission {
        RawSubmission {
            workflow_state: Some(state.into()),
            ..Default::default()
        }
    }

    #[test]
    fn current_course_by_term_dates() {
        let fall = Some((
            Some(datetime!(2026-09-20 0:00 UTC)),
            Some(datetime!(2026-12-15 0:00 UTC)),
        ));
        let spring = Some((
            Some(datetime!(2026-03-20 0:00 UTC)),
            Some(datetime!(2026-06-15 0:00 UTC)),
        ));
        assert!(is_current_course(&course(fall), NOW));
        assert!(!is_current_course(&course(spring), NOW));
    }

    #[test]
    fn course_without_any_dates_is_kept() {
        assert!(is_current_course(&course(None), NOW));
        assert!(is_current_course(&course(Some((None, None))), NOW));
    }

    #[test]
    fn course_dates_used_when_term_has_none() {
        let mut c = course(Some((None, None)));
        c.end_at = Some(datetime!(2026-06-01 0:00 UTC));
        assert!(!is_current_course(&c, NOW));
    }

    #[test]
    fn restricted_course_is_excluded() {
        let mut c = course(None);
        c.access_restricted_by_date = true;
        assert!(!is_current_course(&c, NOW));
    }

    #[test]
    fn course_code_falls_back_to_name() {
        let mut c = course(None);
        c.course_code = Some("  ".into());
        assert_eq!(to_course(&c).code, "Software Engineering");
    }

    #[test]
    fn submission_kinds() {
        let k = |t: &[&str]| submission_kind(&t.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(k(&["online_upload"]), SubmissionKind::Online);
        assert_eq!(k(&["online_upload", "on_paper"]), SubmissionKind::Online);
        assert_eq!(k(&["external_tool"]), SubmissionKind::Online);
        assert_eq!(k(&["on_paper"]), SubmissionKind::OnPaper);
        assert_eq!(k(&["none"]), SubmissionKind::NoSubmission);
        assert_eq!(k(&[]), SubmissionKind::NoSubmission);
        assert_eq!(k(&["not_graded"]), SubmissionKind::NotGraded);
    }

    #[test]
    fn status_rules() {
        let future = datetime!(2026-10-10 0:00 UTC);
        let past = datetime!(2026-10-01 0:00 UTC);

        assert_eq!(status(None, future, NOW), Status::NotSubmitted);
        assert_eq!(
            status(Some(&sub("unsubmitted")), future, NOW),
            Status::NotSubmitted
        );
        assert_eq!(
            status(Some(&sub("unsubmitted")), past, NOW),
            Status::Missing
        );

        let mut missing = sub("unsubmitted");
        missing.missing = true;
        assert_eq!(status(Some(&missing), future, NOW), Status::Missing);

        let mut submitted = sub("submitted");
        submitted.submitted_at = Some(past);
        assert_eq!(status(Some(&submitted), future, NOW), Status::Submitted);
        submitted.late = true;
        assert_eq!(status(Some(&submitted), past, NOW), Status::Late);

        let mut graded = sub("graded");
        graded.score = Some(9.5);
        assert_eq!(status(Some(&graded), past, NOW), Status::Graded);

        // "graded" without a score or grade has not actually been graded yet.
        assert_eq!(status(Some(&sub("graded")), past, NOW), Status::Missing);

        let mut excused = sub("unsubmitted");
        excused.excused = true;
        assert_eq!(status(Some(&excused), past, NOW), Status::Excused);

        assert_eq!(
            status(Some(&sub("pending_review")), future, NOW),
            Status::Submitted
        );
    }

    fn raw(types: &[&str], due: Option<OffsetDateTime>) -> RawAssignment {
        RawAssignment {
            id: "9".into(),
            name: Some("Homework 1".into()),
            due_at: due,
            html_url: Some("https://canvas.school.edu/courses/1/assignments/9".into()),
            submission_types: types.iter().map(|s| s.to_string()).collect(),
            submission: None,
            external_tool_tag_attributes: None,
        }
    }

    #[test]
    fn display_rules() {
        let due = Some(datetime!(2026-10-10 0:00 UTC));
        assert!(to_assignment(&raw(&["online_upload"], None), "1", NOW, false).is_none());
        assert!(to_assignment(&raw(&["online_upload"], due), "1", NOW, false).is_some());
        assert!(to_assignment(&raw(&["on_paper"], due), "1", NOW, false).is_some());
        assert!(to_assignment(&raw(&["none"], due), "1", NOW, false).is_none());
        assert!(to_assignment(&raw(&["not_graded"], due), "1", NOW, false).is_none());
        assert!(to_assignment(&raw(&["none"], due), "1", NOW, true).is_some());
    }

    #[test]
    fn only_https_links_are_kept() {
        let mut r = raw(&["online_upload"], Some(datetime!(2026-10-10 0:00 UTC)));
        r.html_url = Some("javascript:alert(1)".into());
        assert_eq!(to_assignment(&r, "1", NOW, false).unwrap().url, None);
    }

    #[test]
    fn detects_gradescope_links() {
        let mut r = raw(&["external_tool"], Some(datetime!(2026-10-10 0:00 UTC)));
        r.external_tool_tag_attributes = Some(RawExternalTool {
            url: Some("https://www.gradescope.com/auth/lti/callback".into()),
        });
        assert!(
            to_assignment(&r, "1", NOW, false)
                .unwrap()
                .links_to_gradescope
        );
        assert!(is_gradescope_host("gradescope.ca"));
        assert!(!is_gradescope_host("notgradescope.com"));
    }

    #[test]
    fn finds_gradescope_tab_on_same_site_only() {
        let origin = url::Url::parse("https://canvas.example.edu").unwrap();
        let tab = |label: &str, kind: &str, href: &str| RawTab {
            html_url: Some(href.into()),
            label: Some(label.into()),
            kind: Some(kind.into()),
        };
        assert_eq!(
            gradescope_tab_url(
                &tab("Gradescope", "external", "/courses/1/external_tools/77"),
                &origin
            )
            .unwrap()
            .as_str(),
            "https://canvas.example.edu/courses/1/external_tools/77"
        );
        assert!(
            gradescope_tab_url(&tab("Grades", "internal", "/courses/1/grades"), &origin).is_none()
        );
        assert!(gradescope_tab_url(
            &tab("Gradescope", "external", "https://evil.example.com/x"),
            &origin
        )
        .is_none());
    }

    #[test]
    fn parses_canvas_json() {
        let json = r#"{
            "id": "123450000000000099",
            "name": "Lab 1",
            "due_at": "2026-10-10T06:59:59Z",
            "html_url": "https://canvas.school.edu/courses/1/assignments/2",
            "submission_types": ["online_upload"],
            "submission": {"workflow_state": "submitted", "submitted_at": "2026-10-09T20:00:00Z",
                           "late": false, "missing": false, "excused": null, "score": null, "grade": null}
        }"#;
        let parsed: RawAssignment = serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}"));
        let a = to_assignment(&parsed, "1", NOW, false).unwrap();
        assert_eq!(a.id, "123450000000000099");
        assert_eq!(a.status, Status::Submitted);
    }
}

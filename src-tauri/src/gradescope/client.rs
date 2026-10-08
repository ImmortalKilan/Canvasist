//! Reads Gradescope pages with the session cookies obtained through Canvas.

use std::sync::Arc;

use reqwest::StatusCode;
use time::OffsetDateTime;
use url::Url;

use super::parse::{self, ParsedAssignment, ParsedCourse, ParsedStatus};
use crate::canvas::cookies::SharedCookies;
use crate::domain::{Assignment, Course, Source, Status, SubmissionKind};
use crate::error::{AppError, AppResult};
use crate::http;

pub struct GradescopeClient {
    origin: Url,
    http: reqwest::Client,
}

impl GradescopeClient {
    pub fn new(origin: Url, cookies: Arc<SharedCookies>) -> AppResult<Self> {
        Ok(Self {
            origin,
            http: http::session_client(cookies)?,
        })
    }

    pub(crate) async fn get_html(&self, path: &str) -> AppResult<String> {
        let url = self.origin.join(path).map_err(|_| AppError::InvalidUrl)?;
        if url.origin() != self.origin.origin() {
            return Err(AppError::InvalidUrl);
        }
        let response = self.http.get(url).send().await?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(AppError::GradescopeAuth)
            }
            s if s.is_redirection() => return Err(AppError::GradescopeAuth),
            s => return Err(AppError::GradescopeStatus(s.as_u16())),
        }
        let html = response.text().await?;
        if parse::is_login_page(&html) {
            return Err(AppError::GradescopeAuth);
        }
        Ok(html)
    }

    /// Fetches current-term courses and their assignments.
    pub async fn fetch(&self) -> AppResult<(Vec<Course>, Vec<Assignment>)> {
        let now = OffsetDateTime::now_utc();
        let all = parse::parse_account(&self.get_html("/account").await?);
        let current = parse::current_term(&all);

        let mut courses = Vec::new();
        let mut assignments = Vec::new();
        let mut rows_total = 0;
        // Sequential on purpose: a student has a handful of courses, and being
        // gentle with an undocumented site matters more than shaving a second.
        for course in &current {
            let rows =
                parse::parse_course(&self.get_html(&format!("/courses/{}", course.id)).await?);
            rows_total += rows.len();
            assignments.extend(
                rows.iter()
                    .filter_map(|r| self.to_assignment(r, course, now)),
            );
            courses.push(to_course(course));
        }
        let mut terms: Vec<&str> = all.iter().map(|c| c.term.as_str()).collect();
        terms.dedup();
        // Counts only: logs never contain course names or assignment titles.
        log::info!(
            "gradescope refresh: {} of {} course(s) in current term ({} term(s) listed), {} row(s), {} assignment(s) shown",
            current.len(),
            all.len(),
            terms.len(),
            rows_total,
            assignments.len()
        );
        Ok((courses, assignments))
    }

    fn to_assignment(
        &self,
        row: &ParsedAssignment,
        course: &ParsedCourse,
        now: OffsetDateTime,
    ) -> Option<Assignment> {
        // Lighthouse rule: assignments without a deadline are not shown.
        let due_at = row.due_at?;
        let page = match &row.id {
            Some(id) => format!("/courses/{}/assignments/{id}", course.id),
            None => format!("/courses/{}", course.id),
        };
        let local_id = row
            .id
            .clone()
            .unwrap_or_else(|| format!("t{:016x}", fnv1a(&format!("{}|{}", row.title, due_at))));
        Some(Assignment {
            id: format!("gs:{}:{local_id}", course.id),
            course_id: format!("gs:{}", course.id),
            title: row.title.clone(),
            due_at,
            late_due_at: row.late_due_at,
            url: self.origin.join(&page).ok().map(String::from),
            kind: SubmissionKind::Online,
            status: status(row.status, due_at, now),
            source: Source::Gradescope,
            links_to_gradescope: false,
            also_in_canvas: false,
            canvas_due_at: None,
        })
    }
}

fn to_course(course: &ParsedCourse) -> Course {
    let name = if course.name.is_empty() {
        course.short_name.clone()
    } else {
        course.name.clone()
    };
    let code = if course.short_name.is_empty() {
        name.clone()
    } else {
        course.short_name.clone()
    };
    Course {
        id: format!("gs:{}", course.id),
        code,
        name,
        source: Source::Gradescope,
    }
}

fn status(parsed: ParsedStatus, due_at: OffsetDateTime, now: OffsetDateTime) -> Status {
    match parsed {
        ParsedStatus::Graded => Status::Graded,
        ParsedStatus::Late => Status::Late,
        ParsedStatus::Submitted => Status::Submitted,
        ParsedStatus::NotSubmitted if now > due_at => Status::Missing,
        ParsedStatus::NotSubmitted => Status::NotSubmitted,
    }
}

/// Stable 64-bit FNV-1a hash, used to give ID-less rows a deterministic ID.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn status_mapping() {
        let due = datetime!(2026-10-10 0:00 UTC);
        let before = datetime!(2026-10-09 0:00 UTC);
        let after = datetime!(2026-10-11 0:00 UTC);
        assert_eq!(
            status(ParsedStatus::NotSubmitted, due, before),
            Status::NotSubmitted
        );
        assert_eq!(
            status(ParsedStatus::NotSubmitted, due, after),
            Status::Missing
        );
        assert_eq!(
            status(ParsedStatus::Submitted, due, after),
            Status::Submitted
        );
        assert_eq!(status(ParsedStatus::Late, due, after), Status::Late);
        assert_eq!(status(ParsedStatus::Graded, due, after), Status::Graded);
    }

    #[test]
    fn rows_become_namespaced_assignments() {
        let client = GradescopeClient::new(
            Url::parse("https://www.gradescope.com").unwrap(),
            Arc::new(SharedCookies::default()),
        )
        .unwrap();
        let course = ParsedCourse {
            id: "101".into(),
            short_name: "ABC 1".into(),
            name: String::new(),
            term: "Fall 2026".into(),
        };
        let row = ParsedAssignment {
            id: Some("5002".into()),
            title: "Homework 2".into(),
            due_at: Some(datetime!(2026-10-11 06:59 UTC)),
            late_due_at: None,
            status: ParsedStatus::NotSubmitted,
        };
        let a = client
            .to_assignment(&row, &course, datetime!(2026-10-07 0:00 UTC))
            .unwrap();
        assert_eq!(a.id, "gs:101:5002");
        assert_eq!(a.course_id, "gs:101");
        assert_eq!(a.source, Source::Gradescope);
        assert_eq!(
            a.url.as_deref(),
            Some("https://www.gradescope.com/courses/101/assignments/5002")
        );
        assert_eq!(to_course(&course).name, "ABC 1");

        let no_due = ParsedAssignment {
            due_at: None,
            ..row.clone()
        };
        assert!(client
            .to_assignment(&no_due, &course, datetime!(2026-10-07 0:00 UTC))
            .is_none());

        let no_id = ParsedAssignment { id: None, ..row };
        let first = client
            .to_assignment(&no_id, &course, datetime!(2026-10-07 0:00 UTC))
            .unwrap();
        let second = client
            .to_assignment(&no_id, &course, datetime!(2026-10-07 0:00 UTC))
            .unwrap();
        assert_eq!(first.id, second.id, "ID-less rows get a stable ID");
        assert_eq!(
            first.url.as_deref(),
            Some("https://www.gradescope.com/courses/101")
        );
    }
}

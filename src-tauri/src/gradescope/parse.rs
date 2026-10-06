//! Parsers for the Gradescope pages Canvasist reads: the account page (course
//! list grouped by term) and a course page (the student assignment table).
//!
//! Gradescope has no public API, so these parsers depend on its HTML. They are
//! written defensively: anything unrecognized is skipped rather than guessed,
//! and the unit tests pin the structure we rely on.

use scraper::{ElementRef, Html, Selector};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCourse {
    /// Numeric Gradescope course ID.
    pub id: String,
    pub short_name: String,
    pub name: String,
    pub term: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedAssignment {
    /// Numeric Gradescope assignment ID, when the row exposes one.
    pub id: Option<String>,
    pub title: String,
    pub due_at: Option<OffsetDateTime>,
    pub late_due_at: Option<OffsetDateTime>,
    pub status: ParsedStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedStatus {
    NotSubmitted,
    Submitted,
    Late,
    Graded,
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("static selector is valid")
}

fn text_of(el: ElementRef<'_>) -> String {
    el.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when Gradescope served its login page instead of the requested page.
pub fn is_login_page(html: &str) -> bool {
    let doc = Html::parse_document(html);
    doc.select(&selector(
        "form[action='/login'], input[name='session[password]']",
    ))
    .next()
    .is_some()
}

/// Student courses from `/account`, in page order (newest term first).
/// Courses listed under an "Instructor Courses" heading are skipped.
pub fn parse_account(html: &str) -> Vec<ParsedCourse> {
    let doc = Html::parse_document(html);
    let items = selector("h1, h2, .courseList--term, a.courseBox");
    let short = selector(".courseBox--shortname");
    let name = selector(".courseBox--name");

    let mut courses = Vec::new();
    let mut in_instructor_section = false;
    let mut term = String::new();
    for el in doc.select(&items) {
        let tag = el.value().name();
        if tag == "h1" || tag == "h2" {
            let heading = text_of(el).to_ascii_lowercase();
            if heading.contains("instructor courses") {
                in_instructor_section = true;
            } else if heading.contains("student courses") {
                in_instructor_section = false;
            }
        } else if el
            .value()
            .has_class("courseList--term", scraper::CaseSensitivity::CaseSensitive)
        {
            term = text_of(el);
        } else if !in_instructor_section {
            let Some(id) = el.value().attr("href").and_then(course_id_from_href) else {
                continue;
            };
            let short_name = el.select(&short).next().map(text_of).unwrap_or_default();
            let full_name = el.select(&name).next().map(text_of).unwrap_or_default();
            courses.push(ParsedCourse {
                id,
                short_name,
                name: full_name,
                term: term.clone(),
            });
        }
    }
    courses
}

/// Courses of the most recent term (the first term on the account page).
pub fn current_term(courses: &[ParsedCourse]) -> Vec<ParsedCourse> {
    let Some(first) = courses.first() else {
        return Vec::new();
    };
    courses
        .iter()
        .filter(|c| c.term == first.term)
        .cloned()
        .collect()
}

/// `/courses/123` -> `123`
fn course_id_from_href(href: &str) -> Option<String> {
    let id = href.strip_prefix("/courses/")?;
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then(|| id.to_owned())
}

/// `/courses/1/assignments/456/...` -> `456`
fn assignment_id_from_href(href: &str) -> Option<String> {
    let rest = href.split("/assignments/").nth(1)?;
    let id: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (!id.is_empty()).then_some(id)
}

/// Parses the rows of the student assignment table on a course page.
pub fn parse_course(html: &str) -> Vec<ParsedAssignment> {
    let doc = Html::parse_document(html);
    let rows = selector("#assignments-student-table tbody tr");
    let header = selector("th");
    let link = selector("a[href]");
    let button = selector("[data-assignment-id]");
    let status_cell = selector(".submissionStatus");
    let score = selector(".submissionStatus--score");
    let due = selector("time.submissionTimeChart--dueDate[datetime]");

    let mut out = Vec::new();
    for row in doc.select(&rows) {
        let Some(th) = row.select(&header).next() else {
            continue;
        };
        let button_el = th.select(&button).next();
        let link_el = th.select(&link).next();
        let title = button_el
            .and_then(|b| b.value().attr("data-assignment-title"))
            .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|t| !t.is_empty())
            .or_else(|| link_el.map(text_of).filter(|t| !t.is_empty()))
            .unwrap_or_else(|| text_of(th));
        if title.is_empty() {
            continue;
        }
        let id = button_el
            .and_then(|b| b.value().attr("data-assignment-id"))
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
            .map(str::to_owned)
            .or_else(|| {
                link_el
                    .and_then(|a| a.value().attr("href"))
                    .and_then(assignment_id_from_href)
            });

        // The first due-date element is the deadline; a second one, labelled
        // "Late Due Date", is the late-submission deadline.
        let mut due_at = None;
        let mut late_due_at = None;
        for time_el in row.select(&due) {
            let parsed = time_el.value().attr("datetime").and_then(parse_time);
            if text_of(time_el).to_ascii_lowercase().contains("late") {
                late_due_at = late_due_at.or(parsed);
            } else {
                due_at = due_at.or(parsed);
            }
        }

        let cell = row.select(&status_cell).next();
        let has_score = cell
            .and_then(|c| c.select(&score).next())
            .map(text_of)
            .is_some_and(|s| s.contains('/') && s.chars().any(|ch| ch.is_ascii_digit()));
        let status_text = cell.map(text_of).unwrap_or_default().to_ascii_lowercase();
        let status = if has_score {
            ParsedStatus::Graded
        } else if status_text.contains("submitted") && !status_text.contains("no submission") {
            if status_text.contains("late") {
                ParsedStatus::Late
            } else {
                ParsedStatus::Submitted
            }
        } else {
            ParsedStatus::NotSubmitted
        };

        out.push(ParsedAssignment {
            id,
            title,
            due_at,
            late_due_at,
            status,
        });
    }
    out
}

/// Gradescope writes `datetime` attributes like `2026-10-10 23:59:00 -0700`;
/// RFC 3339 is accepted too in case that changes.
pub fn parse_time(value: &str) -> Option<OffsetDateTime> {
    let value = value.trim();
    OffsetDateTime::parse(value, &Rfc3339)
        .or_else(|_| {
            OffsetDateTime::parse(
                value,
                format_description!(
                    "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]"
                ),
            )
        })
        .or_else(|_| {
            OffsetDateTime::parse(
                value,
                format_description!(
                    "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory]:[offset_minute]"
                ),
            )
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const ACCOUNT: &str = r#"
      <html><body>
        <div class="pageHeading"><h1 class="pageHeading--title">Instructor Courses</h1></div>
        <div class="courseList">
          <div class="courseList--term">Fall 2026</div>
          <div class="courseList--coursesForTerm">
            <a class="courseBox" href="/courses/900"><h3 class="courseBox--shortname">TA 1</h3></a>
          </div>
        </div>
        <div class="pageHeading"><h1 class="pageHeading--title">Student Courses</h1></div>
        <div class="courseList">
          <div class="courseList--term">Fall 2026</div>
          <div class="courseList--coursesForTerm">
            <a class="courseBox" href="/courses/101">
              <h3 class="courseBox--shortname">ABC 1</h3>
              <div class="courseBox--name">Intro to Things</div>
            </a>
            <a class="courseBox" href="/courses/102">
              <h3 class="courseBox--shortname">XYZ 2</h3>
              <div class="courseBox--name">More Things</div>
            </a>
            <button class="courseBox courseBox-new">Add a course</button>
          </div>
          <div class="courseList--term">Spring 2026</div>
          <div class="courseList--coursesForTerm">
            <a class="courseBox" href="/courses/55"><h3 class="courseBox--shortname">OLD 1</h3></a>
          </div>
        </div>
      </body></html>"#;

    const COURSE: &str = r#"
      <table id="assignments-student-table"><tbody>
        <tr role="row">
          <th class="table--primaryLink" scope="row">
            <a aria-label="View Homework 1" href="/courses/101/assignments/5001/submissions/77">Homework 1</a>
          </th>
          <td class="submissionStatus"><div class="submissionStatus--score">9.5 / 10.0</div></td>
          <td><div class="submissionTimeChart">
            <time class="submissionTimeChart--releaseDate" datetime="2026-09-28 09:00:00 -0700">Sep 28</time>
            <time class="submissionTimeChart--dueDate" datetime="2026-10-03 23:59:00 -0700">Oct 03 at 11:59PM</time>
          </div></td>
        </tr>
        <tr role="row">
          <th class="table--primaryLink" scope="row">
            <button class="js-submitAssignment" data-assignment-id="5002" data-assignment-title="Homework   2">Homework 2</button>
          </th>
          <td class="submissionStatus"><div class="submissionStatus--text">No Submission</div></td>
          <td><div class="submissionTimeChart">
            <time class="submissionTimeChart--dueDate" datetime="2026-10-10 23:59:00 -0700">Oct 10 at 11:59PM</time>
            <time class="submissionTimeChart--dueDate" datetime="2026-10-12 23:59:00 -0700">Late Due Date: Oct 12 at 11:59PM</time>
          </div></td>
        </tr>
        <tr role="row">
          <th class="table--primaryLink" scope="row">
            <a href="/courses/101/assignments/5003/submissions/78">Lab 1</a>
          </th>
          <td class="submissionStatus"><div class="submissionStatus--text">Submitted</div>
            <span class="submissionStatus--warning">Late</span></td>
          <td><time class="submissionTimeChart--dueDate" datetime="2026-10-01T06:59:00Z">Sep 30</time></td>
        </tr>
        <tr role="row">
          <th class="table--primaryLink" scope="row">Exam 1</th>
          <td class="submissionStatus"><div class="submissionStatus--text">Submitted</div></td>
          <td></td>
        </tr>
      </tbody></table>"#;

    #[test]
    fn account_lists_student_courses_by_term() {
        let courses = parse_account(ACCOUNT);
        let ids: Vec<_> = courses.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["101", "102", "55"]);
        assert_eq!(courses[0].short_name, "ABC 1");
        assert_eq!(courses[0].name, "Intro to Things");
        assert_eq!(courses[0].term, "Fall 2026");
        assert_eq!(courses[2].term, "Spring 2026");
    }

    #[test]
    fn current_term_is_the_first_term() {
        let current = current_term(&parse_account(ACCOUNT));
        assert_eq!(
            current.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["101", "102"]
        );
        assert!(current_term(&[]).is_empty());
    }

    #[test]
    fn course_rows_are_parsed() {
        let rows = parse_course(COURSE);
        assert_eq!(rows.len(), 4);

        assert_eq!(rows[0].id.as_deref(), Some("5001"));
        assert_eq!(rows[0].title, "Homework 1");
        assert_eq!(rows[0].status, ParsedStatus::Graded);
        assert_eq!(rows[0].due_at, Some(datetime!(2026-10-04 06:59 UTC)));
        assert_eq!(rows[0].late_due_at, None);

        assert_eq!(rows[1].id.as_deref(), Some("5002"));
        assert_eq!(rows[1].title, "Homework 2");
        assert_eq!(rows[1].status, ParsedStatus::NotSubmitted);
        assert_eq!(rows[1].late_due_at, Some(datetime!(2026-10-13 06:59 UTC)));

        assert_eq!(rows[2].status, ParsedStatus::Late);
        assert_eq!(rows[2].due_at, Some(datetime!(2026-10-01 06:59 UTC)));

        assert_eq!(rows[3].id, None);
        assert_eq!(rows[3].title, "Exam 1");
        assert_eq!(rows[3].due_at, None);
        assert_eq!(rows[3].status, ParsedStatus::Submitted);
    }

    #[test]
    fn page_without_table_yields_nothing() {
        assert!(parse_course("<html><body><p>Hello</p></body></html>").is_empty());
    }

    #[test]
    fn detects_login_page() {
        let login =
            r#"<form action="/login" method="post"><input name="session[password]"></form>"#;
        assert!(is_login_page(login));
        assert!(!is_login_page(COURSE));
    }

    #[test]
    fn time_formats() {
        assert_eq!(
            parse_time("2026-10-10 23:59:00 -0700"),
            Some(datetime!(2026-10-11 06:59 UTC))
        );
        assert_eq!(
            parse_time("2026-10-10 23:59:00 -07:00"),
            Some(datetime!(2026-10-11 06:59 UTC))
        );
        assert_eq!(
            parse_time("2026-10-11T06:59:00Z"),
            Some(datetime!(2026-10-11 06:59 UTC))
        );
        assert_eq!(parse_time("Oct 10"), None);
    }

    #[test]
    fn hrefs() {
        assert_eq!(course_id_from_href("/courses/123").as_deref(), Some("123"));
        assert_eq!(course_id_from_href("/courses/123/assignments"), None);
        assert_eq!(course_id_from_href("https://evil/courses/1"), None);
        assert_eq!(
            assignment_id_from_href("/courses/1/assignments/456/submissions/9").as_deref(),
            Some("456")
        );
        assert_eq!(assignment_id_from_href("/courses/1"), None);
    }
}

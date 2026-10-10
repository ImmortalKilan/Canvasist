//! Everything that talks to Gradescope.

pub mod bridge;
pub mod client;
pub mod login;
pub mod parse;

use std::sync::Arc;

use tauri::webview::Cookie;
use tauri::AppHandle;
use url::Url;

use crate::canvas::client::CanvasClient;
use crate::canvas::cookies::SharedCookies;
use crate::domain::{Assignment, Course, GradescopeState};
use crate::error::{AppError, AppResult};
use bridge::Launch;
use client::GradescopeClient;

/// Most courses whose Gradescope tab is tried in one refresh. Each course that
/// is not linked to Gradescope costs several seconds in a hidden window.
const MAX_LAUNCHES: usize = 5;

pub struct GradescopeResult {
    pub courses: Vec<Course>,
    pub assignments: Vec<Assignment>,
    pub state: GradescopeState,
    /// The signed-in Gradescope origin, to remember for the next refresh.
    pub origin: Option<Url>,
    /// Set when Gradescope opened from every course but did not sign the user
    /// in: the origin to sign in to directly.
    pub declined: Option<Url>,
}

impl GradescopeResult {
    fn empty(state: GradescopeState, origin: Option<Url>) -> Self {
        Self {
            courses: Vec::new(),
            assignments: Vec::new(),
            state,
            origin,
            declined: None,
        }
    }
}

/// What a Gradescope refresh starts from.
pub struct RefreshInput<'a> {
    pub canvas: &'a CanvasClient,
    pub canvas_origin: &'a Url,
    /// Current Canvas courses, in the order their Gradescope tabs are tried.
    pub course_ids: Vec<String>,
    pub jar: Arc<SharedCookies>,
    /// The Gradescope origin of the saved session, if any.
    pub known_origin: Option<Url>,
    /// False when launching through Canvas is pointless or must wait: it
    /// recently declined, or the direct sign-in window is open (a launch clears
    /// the Gradescope cookies that window shares).
    pub may_launch: bool,
}

/// Fetches Gradescope data, (re)connecting through Canvas when the saved
/// Gradescope session is missing or no longer signed in. Never fails: problems
/// are reported through [`GradescopeResult::state`] so Canvas data still shows.
pub async fn refresh(app: &AppHandle, input: RefreshInput<'_>) -> GradescopeResult {
    let RefreshInput {
        canvas,
        canvas_origin,
        course_ids,
        jar,
        known_origin,
        may_launch,
    } = input;
    if let Some(origin) = known_origin {
        match fetch(&origin, jar.clone()).await {
            Ok(result) => return result,
            Err(AppError::GradescopeAuth) => log::info!("gradescope: session ended, reconnecting"),
            Err(e) => {
                log::warn!("gradescope: refresh failed: {e}");
                return GradescopeResult::empty(GradescopeState::Unavailable, Some(origin));
            }
        }
    }
    if !may_launch {
        return GradescopeResult::empty(GradescopeState::NeedsGradescopeLogin, None);
    }

    let tabs = match canvas.find_gradescope_tabs(&course_ids, MAX_LAUNCHES).await {
        Ok(tabs) if tabs.is_empty() => {
            log::info!(
                "gradescope: no Gradescope tab in {} current course(s)",
                course_ids.len()
            );
            return GradescopeResult::empty(GradescopeState::NotLinked, None);
        }
        Ok(tabs) => tabs,
        Err(AppError::SessionExpired) => {
            return GradescopeResult::empty(GradescopeState::NeedsCanvasLogin, None)
        }
        Err(e) => {
            log::warn!("gradescope: could not look up the Canvas launch point: {e}");
            return GradescopeResult::empty(GradescopeState::Unavailable, None);
        }
    };

    match bridge::connect(app, canvas_origin, &tabs, &jar).await {
        Ok(Launch::Connected(origin)) => fetch(&origin, jar).await.unwrap_or_else(|e| {
            log::warn!("gradescope: fetch after connecting failed: {e}");
            GradescopeResult::empty(GradescopeState::Unavailable, Some(origin))
        }),
        Ok(Launch::Declined(origin)) => GradescopeResult {
            declined: Some(origin),
            ..GradescopeResult::empty(GradescopeState::NeedsGradescopeLogin, None)
        },
        Err(AppError::SessionExpired) => {
            GradescopeResult::empty(GradescopeState::NeedsCanvasLogin, None)
        }
        Err(e) => {
            log::warn!("gradescope: could not connect through Canvas: {e}");
            GradescopeResult::empty(GradescopeState::Unavailable, None)
        }
    }
}

/// Orders course IDs so that courses with a Canvas assignment that opens
/// Gradescope come first: those are the courses linked to Gradescope.
pub fn launch_order(course_ids: &[String], assignments: &[Assignment]) -> Vec<String> {
    let (mut linked, rest): (Vec<String>, Vec<String>) =
        course_ids.iter().cloned().partition(|id| {
            assignments
                .iter()
                .any(|a| a.links_to_gradescope && &a.course_id == id)
        });
    linked.extend(rest);
    linked
}

/// Asks Gradescope whether these cookies belong to a signed-in user.
async fn signed_in(origin: &Url, cookies: &[Cookie<'static>]) -> AppResult<bool> {
    let candidate = Arc::new(SharedCookies::default());
    for cookie in cookies {
        candidate.insert_set_cookie(&cookie.to_string(), origin);
    }
    match GradescopeClient::new(origin.clone(), candidate)?
        .verify_session()
        .await
    {
        Ok(()) => Ok(true),
        Err(AppError::GradescopeAuth) => Ok(false),
        Err(e) => Err(e),
    }
}

async fn fetch(origin: &Url, jar: Arc<SharedCookies>) -> Result<GradescopeResult, AppError> {
    let (courses, assignments) = GradescopeClient::new(origin.clone(), jar)?.fetch().await?;
    Ok(GradescopeResult {
        courses,
        assignments,
        state: GradescopeState::Ok,
        origin: Some(origin.clone()),
        declined: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Source, Status, SubmissionKind};
    use time::macros::datetime;

    fn assignment(course_id: &str, links_to_gradescope: bool) -> Assignment {
        Assignment {
            id: format!("{course_id}-1"),
            course_id: course_id.into(),
            title: "Homework".into(),
            due_at: datetime!(2026-10-10 0:00 UTC),
            late_due_at: None,
            url: None,
            kind: SubmissionKind::Online,
            status: Status::NotSubmitted,
            source: Source::Canvas,
            links_to_gradescope,
            also_in_canvas: false,
            canvas_due_at: None,
        }
    }

    #[test]
    fn courses_linked_to_gradescope_are_tried_first() {
        let ids: Vec<String> = ["1", "2", "3", "4"].map(String::from).into();
        let assignments = [
            assignment("1", false),
            assignment("3", true),
            assignment("4", true),
        ];
        assert_eq!(launch_order(&ids, &assignments), ["3", "4", "1", "2"]);
        assert_eq!(launch_order(&ids, &[]), ids);
    }
}

/// Manual parser check against the developer's own Gradescope account, across
/// all terms. Never runs in CI (`#[ignore]`) and prints only counts and
/// normalized status labels: no course names, titles or scores.
///
/// Run with: `cargo test probe_all_terms -- --ignored --nocapture`
#[cfg(test)]
mod live_probe {
    use std::collections::BTreeMap;

    use super::client::GradescopeClient;
    use super::parse;

    #[test]
    #[ignore = "uses the developer's real Gradescope session"]
    fn probe_all_terms() {
        let dir = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap())
            .join("io.github.immortalkilan.canvasist");
        let (jar, origin) = crate::account::load_saved_session(dir).expect("no saved session");
        let origin = origin.expect("no Gradescope session saved yet");
        let client = GradescopeClient::new(origin, jar).unwrap();

        tauri::async_runtime::block_on(async {
            let courses = parse::parse_account(&client.get_html("/account").await.unwrap());
            let mut terms: Vec<String> = Vec::new();
            for c in &courses {
                if !terms.contains(&c.term) {
                    terms.push(c.term.clone());
                }
            }
            println!("courses: {}, terms: {}", courses.len(), terms.len());
            for (ti, term) in terms.iter().enumerate() {
                let mut statuses: BTreeMap<String, usize> = BTreeMap::new();
                let mut labels: BTreeMap<String, usize> = BTreeMap::new();
                let mut no_due: BTreeMap<String, usize> = BTreeMap::new();
                let (mut rows, mut due, mut late, mut ids) = (0, 0, 0, 0);
                for course in courses.iter().filter(|c| &c.term == term) {
                    let html = client
                        .get_html(&format!("/courses/{}", course.id))
                        .await
                        .unwrap();
                    for r in parse::parse_course(&html) {
                        rows += 1;
                        due += usize::from(r.due_at.is_some());
                        late += usize::from(r.late_due_at.is_some());
                        ids += usize::from(r.id.is_some());
                        *statuses.entry(format!("{:?}", r.status)).or_default() += 1;
                    }
                    for label in raw_status_labels(&html) {
                        *labels.entry(label).or_default() += 1;
                    }
                    for shape in rows_without_due(&html) {
                        *no_due.entry(shape).or_default() += 1;
                    }
                }
                println!(
                    "term #{ti} ({term}): rows={rows} with_due={due} with_late_due={late} with_id={ids}"
                );
                println!("  parsed: {statuses:?}");
                println!("  raw labels: {labels:?}");
                println!("  rows without a due date (status | time classes): {no_due:?}");
            }
        });
    }

    /// For rows lacking a due-date element: masked status text plus the class
    /// names of any <time> elements in the row (structure only, no content).
    fn rows_without_due(html: &str) -> Vec<String> {
        let doc = scraper::Html::parse_document(html);
        let row = scraper::Selector::parse("#assignments-student-table tbody tr").unwrap();
        let due = scraper::Selector::parse("time.submissionTimeChart--dueDate[datetime]").unwrap();
        let time =
            scraper::Selector::parse("time, .submissionTimeChart, .submissionTimeChart--dueDate")
                .unwrap();
        let status = scraper::Selector::parse(".submissionStatus").unwrap();
        doc.select(&row)
            .filter(|r| r.select(&due).next().is_none())
            .map(|r| {
                let label: String = r
                    .select(&status)
                    .next()
                    .map(|c| c.text().collect::<Vec<_>>().join(" "))
                    .unwrap_or_default()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .map(|ch| if ch.is_ascii_digit() { '#' } else { ch })
                    .collect();
                let classes: Vec<String> = r
                    .select(&time)
                    .map(|t| {
                        format!(
                            "{}.{}",
                            t.value().name(),
                            t.value().classes().collect::<Vec<_>>().join(".")
                        )
                    })
                    .collect();
                format!(
                    "{label} | {}",
                    if classes.is_empty() {
                        "-".into()
                    } else {
                        classes.join(",")
                    }
                )
            })
            .collect()
    }

    /// Status-cell texts with every digit masked, so scores never appear.
    fn raw_status_labels(html: &str) -> Vec<String> {
        let doc = scraper::Html::parse_document(html);
        let cell =
            scraper::Selector::parse("#assignments-student-table tbody tr .submissionStatus")
                .unwrap();
        doc.select(&cell)
            .map(|c| {
                let text = c.text().collect::<Vec<_>>().join(" ");
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                text.chars()
                    .map(|ch| if ch.is_ascii_digit() { '#' } else { ch })
                    .collect()
            })
            .collect()
    }
}

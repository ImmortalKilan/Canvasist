//! Everything that talks to Gradescope.

pub mod bridge;
pub mod client;
pub mod parse;

use std::sync::Arc;

use tauri::AppHandle;
use url::Url;

use crate::canvas::client::CanvasClient;
use crate::canvas::cookies::SharedCookies;
use crate::domain::{Assignment, Course, GradescopeState};
use crate::error::AppError;
use client::GradescopeClient;

pub struct GradescopeResult {
    pub courses: Vec<Course>,
    pub assignments: Vec<Assignment>,
    pub state: GradescopeState,
    /// The signed-in Gradescope origin, to remember for the next refresh.
    pub origin: Option<Url>,
}

impl GradescopeResult {
    fn empty(state: GradescopeState, origin: Option<Url>) -> Self {
        Self {
            courses: Vec::new(),
            assignments: Vec::new(),
            state,
            origin,
        }
    }
}

/// Fetches Gradescope data, (re)connecting through Canvas when the saved
/// Gradescope session is missing or no longer signed in. Never fails: problems
/// are reported through [`GradescopeResult::state`] so Canvas data still shows.
pub async fn refresh(
    app: &AppHandle,
    canvas: &CanvasClient,
    canvas_origin: &Url,
    course_ids: &[String],
    jar: Arc<SharedCookies>,
    known_origin: Option<Url>,
) -> GradescopeResult {
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

    let tab = match canvas.find_gradescope_tab(course_ids).await {
        Ok(Some(tab)) => tab,
        Ok(None) => return GradescopeResult::empty(GradescopeState::NotLinked, None),
        Err(AppError::SessionExpired) => {
            return GradescopeResult::empty(GradescopeState::NeedsCanvasLogin, None)
        }
        Err(e) => {
            log::warn!("gradescope: could not look up the Canvas launch point: {e}");
            return GradescopeResult::empty(GradescopeState::Unavailable, None);
        }
    };

    match bridge::connect(app, canvas_origin, &tab, &jar).await {
        Ok(origin) => fetch(&origin, jar).await.unwrap_or_else(|e| {
            log::warn!("gradescope: fetch after connecting failed: {e}");
            GradescopeResult::empty(GradescopeState::Unavailable, Some(origin))
        }),
        Err(AppError::SessionExpired) => {
            GradescopeResult::empty(GradescopeState::NeedsCanvasLogin, None)
        }
        Err(e) => {
            log::warn!("gradescope: could not connect through Canvas: {e}");
            GradescopeResult::empty(GradescopeState::Unavailable, None)
        }
    }
}

async fn fetch(origin: &Url, jar: Arc<SharedCookies>) -> Result<GradescopeResult, AppError> {
    let (courses, assignments) = GradescopeClient::new(origin.clone(), jar)?.fetch().await?;
    Ok(GradescopeResult {
        courses,
        assignments,
        state: GradescopeState::Ok,
        origin: Some(origin.clone()),
    })
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

//! Canvas REST API client authenticated by the user's login session cookies.

use std::sync::Arc;

use futures_util::stream::{self, StreamExt, TryStreamExt};
use reqwest::header::{HeaderValue, ACCEPT, LINK};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use time::OffsetDateTime;
use url::Url;

use super::cookies::SharedCookies;
use super::model::{self, CanvasSnapshot, RawAssignment, RawCourse};
use crate::error::{AppError, AppResult};
use crate::http;

/// Pagination guard: no legitimate listing we request comes close to this.
const MAX_PAGES: usize = 50;
/// Courses fetched in parallel; low enough to stay well within Canvas rate limits.
const COURSE_CONCURRENCY: usize = 4;

pub struct CanvasClient {
    origin: Url,
    http: reqwest::Client,
}

impl CanvasClient {
    pub fn new(origin: Url, cookies: Arc<SharedCookies>) -> AppResult<Self> {
        Ok(Self {
            origin,
            http: http::session_client(cookies)?,
        })
    }

    /// Succeeds when the session is signed in.
    pub async fn verify_session(&self) -> AppResult<()> {
        let _: serde_json::Value = self.get_json("/api/v1/users/self").await?;
        Ok(())
    }

    pub async fn fetch_snapshot(&self, show_unsubmittable: bool) -> AppResult<CanvasSnapshot> {
        let now = OffsetDateTime::now_utc();
        let courses: Vec<RawCourse> = self
            .get_paginated("/api/v1/courses?enrollment_state=active&include[]=term&per_page=100")
            .await?;
        let current: Vec<RawCourse> = courses
            .into_iter()
            .filter(|c| model::is_current_course(c, now))
            .collect();

        // Owned IDs keep the futures free of borrowed course data, so they stay `Send`.
        let course_ids: Vec<String> = current.iter().map(|c| c.id.clone()).collect();
        let per_course: Vec<(String, Vec<RawAssignment>)> = stream::iter(course_ids)
            .map(|course_id| {
                let path = format!(
                    "/api/v1/courses/{course_id}/assignments?include[]=submission&order_by=due_at&per_page=100"
                );
                async move {
                    match self.get_paginated::<RawAssignment>(&path).await {
                        Ok(list) => Ok((course_id, list)),
                        // Some courses hide their assignment list; skip them instead of failing.
                        Err(AppError::CanvasStatus(403 | 404)) => Ok((course_id, Vec::new())),
                        Err(e) => Err(e),
                    }
                }
            })
            .buffer_unordered(COURSE_CONCURRENCY)
            .try_collect()
            .await?;

        let mut assignments: Vec<_> = per_course
            .iter()
            .flat_map(|(course_id, list)| {
                list.iter().filter_map(move |raw| {
                    model::to_assignment(raw, course_id, now, show_unsubmittable)
                })
            })
            .collect();
        assignments.sort_by(|a, b| a.due_at.cmp(&b.due_at).then_with(|| a.id.cmp(&b.id)));

        let snapshot = CanvasSnapshot {
            fetched_at: now,
            courses: current.iter().map(model::to_course).collect(),
            assignments,
        };
        // Counts only: logs never contain course names or assignment titles.
        log::info!(
            "canvas refresh: {} current course(s), {} assignment(s) shown",
            snapshot.courses.len(),
            snapshot.assignments.len()
        );
        Ok(snapshot)
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> AppResult<T> {
        let url = self.origin.join(path).map_err(|_| AppError::InvalidUrl)?;
        let (value, _) = self.get_page(url).await?;
        Ok(value)
    }

    async fn get_paginated<T: DeserializeOwned>(&self, path: &str) -> AppResult<Vec<T>> {
        let mut next = Some(self.origin.join(path).map_err(|_| AppError::InvalidUrl)?);
        let mut items = Vec::new();
        for _ in 0..MAX_PAGES {
            let Some(url) = next.take() else {
                return Ok(items);
            };
            let (page, link): (Vec<T>, Option<Url>) = self.get_page(url).await?;
            items.extend(page);
            next = link;
        }
        log::warn!("pagination stopped after {MAX_PAGES} pages");
        Ok(items)
    }

    async fn get_page<T: DeserializeOwned>(&self, url: Url) -> AppResult<(T, Option<Url>)> {
        // Session cookies must only ever go to the signed-in Canvas origin.
        if url.origin() != self.origin.origin() {
            return Err(AppError::InvalidUrl);
        }
        let response = self
            .http
            .get(url)
            // String IDs: sharded Canvas instances use IDs beyond JavaScript's safe integers.
            .header(
                ACCEPT,
                HeaderValue::from_static("application/json+canvas-string-ids"),
            )
            .send()
            .await?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED => return Err(AppError::SessionExpired),
            other if other.is_redirection() => return Err(AppError::SessionExpired),
            other => return Err(AppError::CanvasStatus(other.as_u16())),
        }
        let next = next_link(response.headers().get(LINK), &self.origin);
        let body = response.text().await?;
        Ok((serde_json::from_str(strip_json_guard(&body))?, next))
    }
}

/// Session-authenticated Canvas responses may start with an anti-JSON-hijacking guard.
fn strip_json_guard(body: &str) -> &str {
    body.strip_prefix("while(1);").unwrap_or(body)
}

/// Parses the `rel="next"` URL from a Link header, accepting only same-origin URLs.
fn next_link(header: Option<&HeaderValue>, origin: &Url) -> Option<Url> {
    let header = header?.to_str().ok()?;
    header.split(',').find_map(|part| {
        let (target, params) = part.split_once(';')?;
        if !params.split(';').any(|p| p.trim() == r#"rel="next""#) {
            return None;
        }
        let url = Url::parse(target.trim().strip_prefix('<')?.strip_suffix('>')?).ok()?;
        (url.origin() == origin.origin()).then_some(url)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin() -> Url {
        Url::parse("https://canvas.school.edu").unwrap()
    }

    #[test]
    fn strips_guard_prefix() {
        assert_eq!(strip_json_guard("while(1);[1]"), "[1]");
        assert_eq!(strip_json_guard("[1]"), "[1]");
    }

    #[test]
    fn finds_next_link() {
        let header = HeaderValue::from_static(
            r#"<https://canvas.school.edu/api/v1/courses?page=1>; rel="current", <https://canvas.school.edu/api/v1/courses?page=2>; rel="next", <https://canvas.school.edu/api/v1/courses?page=5>; rel="last""#,
        );
        assert_eq!(
            next_link(Some(&header), &origin()).unwrap().as_str(),
            "https://canvas.school.edu/api/v1/courses?page=2"
        );
    }

    #[test]
    fn no_next_link_on_last_page() {
        let header = HeaderValue::from_static(
            r#"<https://canvas.school.edu/api/v1/courses?page=1>; rel="first""#,
        );
        assert!(next_link(Some(&header), &origin()).is_none());
        assert!(next_link(None, &origin()).is_none());
    }

    #[test]
    fn rejects_cross_origin_next_link() {
        let header = HeaderValue::from_static(r#"<https://evil.example.com/steal>; rel="next""#);
        assert!(next_link(Some(&header), &origin()).is_none());
    }
}

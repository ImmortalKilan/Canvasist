//! Finding a school's Canvas address.
//!
//! School search uses Instructure's public account-search endpoint (the same one
//! the official Canvas apps use). Only the typed school name is sent.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{AppError, AppResult};

const SEARCH_ENDPOINT: &str = "https://canvas.instructure.com/api/v1/accounts/search";
const MAX_RESULTS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct School {
    pub name: String,
    /// Normalized https origin of the school's Canvas site.
    pub url: String,
}

#[derive(Deserialize)]
struct RawAccount {
    name: Option<String>,
    domain: Option<String>,
}

pub async fn search_schools(http: &reqwest::Client, query: &str) -> AppResult<Vec<School>> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let query: String = query.chars().take(100).collect();
    let response = http
        .get(SEARCH_ENDPOINT)
        .query(&[("search_term", query.as_str()), ("per_page", "50")])
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AppError::CanvasStatus(response.status().as_u16()));
    }
    let accounts: Vec<RawAccount> = response.json().await?;
    Ok(to_schools(accounts))
}

fn to_schools(accounts: Vec<RawAccount>) -> Vec<School> {
    let mut schools: Vec<School> = Vec::new();
    for account in accounts {
        let (Some(name), Some(domain)) = (account.name, account.domain) else {
            continue;
        };
        let Ok(url) = super::url::normalize(&domain) else {
            continue;
        };
        let school = School {
            name: name.trim().to_owned(),
            url: origin_string(&url),
        };
        if !school.name.is_empty() && !schools.contains(&school) {
            schools.push(school);
        }
        if schools.len() == MAX_RESULTS {
            break;
        }
    }
    schools
}

/// Formats an origin without the trailing slash `Url` adds.
pub fn origin_string(url: &Url) -> String {
    url.origin().ascii_serialization()
}

/// Checks that `origin` serves Canvas: an anonymous request for the current user
/// gets Canvas's distinctive `unauthenticated` JSON reply.
pub async fn looks_like_canvas(http: &reqwest::Client, origin: &Url) -> AppResult<bool> {
    let probe = origin
        .join("/api/v1/users/self")
        .map_err(|_| AppError::InvalidUrl)?;
    let response = match http.get(probe).send().await {
        Ok(response) => response,
        Err(e) if e.is_connect() || e.is_timeout() => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    if response.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(false);
    }
    let body = response.text().await.unwrap_or_default();
    Ok(body.contains("unauthenticated"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: Option<&str>, domain: Option<&str>) -> RawAccount {
        RawAccount {
            name: name.map(Into::into),
            domain: domain.map(Into::into),
        }
    }

    #[test]
    fn converts_and_dedupes_accounts() {
        let schools = to_schools(vec![
            account(Some("Example University"), Some("example.instructure.com")),
            account(Some("Example University"), Some("example.instructure.com")),
            account(Some("No Domain"), None),
            account(None, Some("x.instructure.com")),
            account(Some("Bad Domain"), Some("localhost")),
            account(Some("  Extended Studies "), Some("ext.example.edu")),
        ]);
        assert_eq!(
            schools,
            vec![
                School {
                    name: "Example University".into(),
                    url: "https://example.instructure.com".into()
                },
                School {
                    name: "Extended Studies".into(),
                    url: "https://ext.example.edu".into()
                },
            ]
        );
    }

    #[test]
    fn caps_result_count() {
        let many = (0..50)
            .map(|i| {
                account(
                    Some(&format!("School {i}")),
                    Some(&format!("s{i}.instructure.com")),
                )
            })
            .collect();
        assert_eq!(to_schools(many).len(), MAX_RESULTS);
    }
}

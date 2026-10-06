//! Shared HTTP client configuration.

use std::sync::Arc;
use std::time::Duration;

use reqwest::redirect::Policy;

use crate::canvas::cookies::SharedCookies;
use crate::error::AppResult;

const USER_AGENT: &str = concat!(
    "Canvasist/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/ImmortalKilan/Canvasist)"
);

fn base() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .https_only(true)
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
}

/// Client for anonymous requests (school search, Canvas probes).
pub fn anonymous_client() -> AppResult<reqwest::Client> {
    Ok(base().redirect(Policy::limited(5)).build()?)
}

/// Client that sends the stored session cookies. Redirects are not followed: an
/// expired Canvas session answers API calls with a redirect to the login page,
/// and following it could carry cookies to other hosts.
pub fn session_client(cookies: Arc<SharedCookies>) -> AppResult<reqwest::Client> {
    Ok(base()
        .redirect(Policy::none())
        .cookie_provider(cookies)
        .build()?)
}

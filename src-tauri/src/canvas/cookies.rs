//! A cookie jar shared between the HTTP client and the encrypted session file.
//!
//! The HTTP client updates it from `Set-Cookie` responses (Canvas rotates some
//! cookies), and the whole jar, including session cookies without an expiry,
//! is persisted so a saved login survives app restarts.

use std::sync::{Mutex, MutexGuard, PoisonError};

use cookie_store::CookieStore;
use reqwest::header::HeaderValue;
use url::Url;

use crate::error::AppResult;

#[derive(Default)]
pub struct SharedCookies {
    store: Mutex<CookieStore>,
}

impl SharedCookies {
    fn lock(&self) -> MutexGuard<'_, CookieStore> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds a cookie given in `Set-Cookie` syntax, scoped as if it had been set by `url`.
    pub fn insert_set_cookie(&self, set_cookie: &str, url: &Url) {
        if let Err(e) = self.lock().parse(set_cookie, url) {
            // The error never includes the cookie value.
            log::debug!("ignored a cookie that does not apply to its URL: {e}");
        }
    }

    #[cfg(test)]
    pub fn has_cookies_for(&self, url: &Url) -> bool {
        self.lock().get_request_values(url).next().is_some()
    }

    pub fn to_json(&self) -> AppResult<String> {
        let mut out = Vec::new();
        cookie_store::serde::json::save_incl_expired_and_nonpersistent(&self.lock(), &mut out)
            .map_err(|e| crate::error::AppError::Crypto(e.to_string()))?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    pub fn from_json(json: &str) -> AppResult<Self> {
        let store = cookie_store::serde::json::load_all(json.as_bytes())
            .map_err(|e| crate::error::AppError::Crypto(e.to_string()))?;
        Ok(Self {
            store: Mutex::new(store),
        })
    }
}

impl reqwest::cookie::CookieStore for SharedCookies {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let mut store = self.lock();
        for header in cookie_headers {
            if let Ok(text) = header.to_str() {
                let _ = store.parse(text, url);
            }
        }
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let header = self
            .lock()
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        if header.is_empty() {
            None
        } else {
            HeaderValue::from_str(&header).ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::cookie::CookieStore as _;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn session_cookies_survive_serialization() {
        let jar = SharedCookies::default();
        let canvas = url("https://canvas.school.edu/");
        jar.insert_set_cookie("canvas_session=abc; Path=/; Secure; HttpOnly", &canvas);
        jar.insert_set_cookie("_csrf_token=xyz; Path=/; Secure", &canvas);

        let restored = SharedCookies::from_json(&jar.to_json().unwrap()).unwrap();
        let header = restored.cookies(&canvas).unwrap();
        let header = header.to_str().unwrap();
        assert!(header.contains("canvas_session=abc"));
        assert!(header.contains("_csrf_token=xyz"));
    }

    #[test]
    fn cookies_are_scoped_to_their_host() {
        let jar = SharedCookies::default();
        jar.insert_set_cookie(
            "canvas_session=abc; Path=/",
            &url("https://canvas.school.edu/"),
        );
        assert!(jar.has_cookies_for(&url("https://canvas.school.edu/api/v1/courses")));
        assert!(!jar.has_cookies_for(&url("https://evil.example.com/")));
        assert!(jar.cookies(&url("https://evil.example.com/")).is_none());
    }

    #[test]
    fn responses_update_rotated_cookies() {
        let jar = SharedCookies::default();
        let canvas = url("https://canvas.school.edu/");
        jar.insert_set_cookie("canvas_session=old; Path=/", &canvas);
        let new = HeaderValue::from_static("canvas_session=new; Path=/");
        jar.set_cookies(&mut std::iter::once(&new), &canvas);
        assert_eq!(jar.cookies(&canvas).unwrap(), "canvas_session=new");
    }
}

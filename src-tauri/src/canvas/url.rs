//! Normalizes user-entered Canvas addresses into a bare https origin.

use std::net::IpAddr;

use url::{Host, Url};

use crate::error::{AppError, AppResult};

/// Turns input such as `canvas.school.edu`, `https://canvas.school.edu/courses/1`
/// or `http://Canvas.School.edu` into `https://canvas.school.edu`.
///
/// Rejected: credentials in the URL, IP addresses, single-label hosts such as
/// `localhost`, and schemes other than http(s). Plain http is upgraded, since
/// Canvas is always served over https.
pub fn normalize(input: &str) -> AppResult<Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > 255 {
        return Err(AppError::InvalidUrl);
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = Url::parse(&with_scheme).map_err(|_| AppError::InvalidUrl)?;

    if !matches!(parsed.scheme(), "https" | "http") {
        return Err(AppError::InvalidUrl);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::InvalidUrl);
    }
    let host = match parsed.host() {
        Some(Host::Domain(domain)) => domain.to_ascii_lowercase(),
        _ => return Err(AppError::InvalidUrl),
    };
    if host.parse::<IpAddr>().is_ok() || !host.contains('.') || host.ends_with('.') {
        return Err(AppError::InvalidUrl);
    }

    let mut origin = format!("https://{host}");
    if let Some(port) = parsed.port().filter(|&p| p != 443 && p != 80) {
        origin.push_str(&format!(":{port}"));
    }
    Url::parse(&origin).map_err(|_| AppError::InvalidUrl)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(input: &str) -> String {
        normalize(input).unwrap().to_string()
    }

    #[test]
    fn adds_scheme_and_strips_path() {
        assert_eq!(ok("canvas.example.edu"), "https://canvas.example.edu/");
        assert_eq!(
            ok("https://canvas.example.edu/courses/12?x=1#y"),
            "https://canvas.example.edu/"
        );
        assert_eq!(
            ok("  school.instructure.com/  "),
            "https://school.instructure.com/"
        );
    }

    #[test]
    fn upgrades_http_and_lowercases() {
        assert_eq!(ok("http://Canvas.School.EDU"), "https://canvas.school.edu/");
    }

    #[test]
    fn keeps_non_default_port() {
        assert_eq!(
            ok("canvas.school.edu:8443"),
            "https://canvas.school.edu:8443/"
        );
        assert_eq!(ok("canvas.school.edu:443"), "https://canvas.school.edu/");
    }

    #[test]
    fn rejects_unsafe_or_meaningless_input() {
        for bad in [
            "",
            "   ",
            "localhost",
            "canvas",
            "127.0.0.1",
            "https://10.0.0.5",
            "https://[::1]",
            "https://user:pass@canvas.school.edu",
            "ftp://canvas.school.edu",
            "javascript:alert(1)",
            "file:///C:/Windows",
        ] {
            assert!(normalize(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}

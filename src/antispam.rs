//! Honeypot, captcha and rate limit for the public form, as in form-plugin and blog-comments.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::http::HeaderMap;
use serde::Deserialize;

use crate::model::Settings;

// shortcut: in-memory and per plugin process, so limits reset on restart; move to the database if
// the plugin ever runs as more than one process
pub struct RateLimiter {
    window: Duration,
    max: usize,
    entries: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(window: Duration, max: usize) -> Self {
        Self {
            window,
            max,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Records an attempt for `ip` and returns whether it is within the limit. Rejected attempts
    /// are not recorded, so a blocked client gets through again once the window has moved on.
    pub fn check(&self, ip: IpAddr, now: Instant) -> bool {
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|_, attempts| {
            attempts.retain(|at| now.duration_since(*at) < self.window);
            !attempts.is_empty()
        });
        let attempts = entries.entry(ip).or_default();
        if attempts.len() >= self.max {
            return false;
        }
        attempts.push(now);
        true
    }
}

/// The client ip as forwarded by the neleto gateway (rightmost `X-Forwarded-For` entry, then
/// `X-Real-IP`), like form-plugin. The socket peer is always the neleto backend on localhost.
pub fn client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    if let Some(ip) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').map(str::trim).rfind(|e| !e.is_empty()))
        .and_then(|v| v.parse().ok())
    {
        return Some(ip);
    }
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok())
}

/// The site origin the visitor used, e.g. `https://example.com`, for links in mails.
pub fn site_url(headers: &HeaderMap) -> Option<String> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let host = header("x-forwarded-host").or_else(|| header("host"))?;
    let local = host.starts_with("localhost") || host.starts_with("127.0.0.1");
    let scheme = header("x-forwarded-proto").unwrap_or(if local { "http" } else { "https" });
    Some(format!("{scheme}://{host}"))
}

/// The provider, site key and secret, if a captcha is fully configured.
pub fn captcha(settings: &Settings) -> Option<(&str, &str, &str)> {
    fn non_empty(s: &Option<String>) -> Option<&str> {
        s.as_deref().filter(|s| !s.is_empty())
    }
    Some((
        non_empty(&settings.captcha_provider)?,
        non_empty(&settings.captcha_sitekey)?,
        non_empty(&settings.captcha_secret)?,
    ))
}

/// The form field the provider's widget puts its token in.
pub fn captcha_field(provider: &str) -> &'static str {
    match provider {
        "h-captcha" => "h-captcha-response",
        "re-captcha" => "g-recaptcha-response",
        _ => "cf-turnstile-response",
    }
}

#[derive(Deserialize)]
struct CaptchaVerify {
    success: bool,
}

/// `true` when no captcha is configured or the token passes.
pub async fn verify_captcha(settings: &Settings, response: Option<&str>) -> bool {
    let Some((provider, _, secret)) = captcha(settings) else {
        return true;
    };
    let Some(response) = response.filter(|r| !r.is_empty()) else {
        return false;
    };
    let url = match provider {
        "h-captcha" => "https://api.hcaptcha.com/siteverify",
        "re-captcha" => "https://www.google.com/recaptcha/api/siteverify",
        _ => "https://challenges.cloudflare.com/turnstile/v0/siteverify",
    };
    let verified = reqwest::Client::new()
        .post(url)
        .form(&[("secret", secret), ("response", response)])
        .header("Accept", "application/json")
        .send()
        .await;
    match verified {
        Ok(res) => res.json::<CaptchaVerify>().await.is_ok_and(|v| v.success),
        Err(e) => {
            tracing::warn!("captcha verification failed: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_attempts_per_ip() {
        let limiter = RateLimiter::new(Duration::from_secs(60), 2);
        let (a, b): (IpAddr, IpAddr) = ("1.1.1.1".parse().unwrap(), "2.2.2.2".parse().unwrap());
        let t0 = Instant::now();
        assert!(limiter.check(a, t0));
        assert!(limiter.check(a, t0));
        assert!(!limiter.check(a, t0));
        assert!(limiter.check(b, t0));
        assert!(limiter.check(a, t0 + Duration::from_secs(61)));
    }

    #[test]
    fn reads_forwarded_headers() {
        let mut headers = HeaderMap::new();
        assert_eq!(client_ip(&headers), None);
        headers.insert("x-forwarded-for", "6.6.6.6, 5.5.5.5".parse().unwrap());
        assert_eq!(client_ip(&headers), Some("5.5.5.5".parse().unwrap()));
        headers.insert("host", "localhost:8008".parse().unwrap());
        assert_eq!(site_url(&headers).as_deref(), Some("http://localhost:8008"));
        headers.insert("x-forwarded-host", "verein.de".parse().unwrap());
        assert_eq!(site_url(&headers).as_deref(), Some("https://verein.de"));
    }

    #[test]
    fn captcha_needs_all_three_settings() {
        let mut s = Settings {
            captcha_provider: Some("turnstile".into()),
            captcha_sitekey: Some("k".into()),
            ..Settings::default()
        };
        assert!(captcha(&s).is_none());
        s.captcha_secret = Some("s".into());
        assert_eq!(captcha(&s), Some(("turnstile", "k", "s")));
    }
}

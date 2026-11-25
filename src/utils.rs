use axum::{Json, http::StatusCode, response::IntoResponse};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Template(#[from] minijinja::Error),
}

pub type AppResult<T> = Result<T, AppError>;

/// For lookups after the change is committed: a failure there must not report the change as
/// failed, so it is only logged.
pub fn after_commit<T: Default>(result: sqlx::Result<T>) -> T {
    result.unwrap_or_else(|e| {
        tracing::error!("after commit: {e}");
        T::default()
    })
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound => StatusCode::NOT_FOUND,
            _ => {
                tracing::error!("{self}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

/// HTML-escape untrusted text before putting it into server-built markup.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

// shortcut: the CMS has no timezone setting, so dates are shown in Europe/Berlin; read it from
// the CMS once it has one
pub fn local(dt: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono_tz::Tz> {
    dt.with_timezone(&chrono_tz::Europe::Berlin)
}

//! The admin API behind `/api/rest/plugins/event-registration/api`, see `plugin.json` for the
//! permissions.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::header,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState, database, email,
    lang::Lang,
    model::{EventConfigInput, EventOverview, Registration, RegistrationStatus, Settings},
    utils::{AppError, AppResult, local},
};

/// GET /api/events
pub async fn events(State(state): State<AppState>) -> AppResult<Json<Vec<EventOverview>>> {
    Ok(Json(database::overview(&state.db, &state.cms_db).await?))
}

async fn one(state: &AppState, id: i32) -> AppResult<EventOverview> {
    // shortcut: builds the whole list for one row; fine for a few hundred events, filter in SQL
    // if a site ever has thousands
    database::overview(&state.db, &state.cms_db)
        .await?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or(AppError::NotFound)
}

/// GET /api/events/:id
pub async fn event(State(state): State<AppState>, Path(id): Path<i32>) -> AppResult<Json<EventOverview>> {
    Ok(Json(one(&state, id).await?))
}

/// PUT /api/events/:id
pub async fn save_event(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(input): Json<EventConfigInput>,
) -> AppResult<Json<EventOverview>> {
    let bad = |m: &str| Err(AppError::BadRequest(m.into()));
    if input.capacity.is_some_and(|c| c < 1) {
        return bad("capacity must be at least 1");
    }
    if input.max_seats_per_registration < 1 {
        return bad("max_seats_per_registration must be at least 1");
    }
    if let (Some(o), Some(c)) = (input.opens_at, input.closes_at)
        && o >= c
    {
        return bad("opens_at must be before closes_at");
    }
    if input
        .notify_email
        .as_deref()
        .is_some_and(|e| !e.trim().is_empty() && !e.contains('@'))
    {
        return bad("notify_email is not an email address");
    }
    let event = database::cms_event_any(&state.cms_db, id).await?;
    // a deleted event's existing config may still be changed, e.g. switched off
    if event.is_none() && database::get_config(&state.db, id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    let (_, promoted) = database::save_config(&state.db, id, &input).await?;
    if let Some(event) = &event {
        email::promoted(event, &promoted);
    }
    Ok(Json(one(&state, id).await?))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub event_id: Option<i32>,
    pub status: Option<RegistrationStatus>,
    pub q: Option<String>,
}

/// GET /api/registrations?event_id=&status=&q=
pub async fn registrations(State(state): State<AppState>, Query(q): Query<ListQuery>) -> AppResult<Json<Vec<Registration>>> {
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    Ok(Json(
        database::list_registrations(&state.db, q.event_id, q.status, search).await?,
    ))
}

#[derive(Deserialize)]
pub struct StatusChange {
    pub id: Uuid,
    pub status: RegistrationStatus,
}

/// PUT /api/registrations {id, status}
pub async fn set_status(State(state): State<AppState>, Json(change): Json<StatusChange>) -> AppResult<Json<Registration>> {
    let (changed, previous) = match database::set_status(&state.db, change.id, change.status).await? {
        Ok(Some(c)) => c,
        Ok(None) => return Err(AppError::NotFound),
        Err(_) => {
            return Err(AppError::BadRequest(
                "this email address already has an active registration".into(),
            ));
        }
    };
    if let Some(event) = database::cms_event_any(&state.cms_db, changed.registration.event_id).await? {
        let mut promoted = changed.promoted;
        // confirmed by hand from the waitlist: same mail as moving up on its own
        if previous == RegistrationStatus::Waitlisted && change.status == RegistrationStatus::Confirmed {
            promoted.push(changed.registration.clone());
        }
        email::promoted(&event, &promoted);
    }
    Ok(Json(changed.registration))
}

#[derive(Deserialize)]
pub struct IdQuery {
    pub id: Uuid,
}

/// DELETE /api/registrations?id=
pub async fn delete(State(state): State<AppState>, Query(q): Query<IdQuery>) -> AppResult<Json<Value>> {
    let changed = database::delete_registration(&state.db, q.id)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(event) = database::cms_event_any(&state.cms_db, changed.registration.event_id).await? {
        email::promoted(&event, &changed.promoted);
    }
    Ok(Json(json!({ "success": true })))
}

#[derive(Deserialize)]
pub struct ExportQuery {
    pub event_id: i32,
    pub lang: Option<String>,
}

/// GET /api/registrations/export?event_id=&lang=, semicolon-separated with a BOM so Excel opens it
/// with umlauts intact.
pub async fn export(State(state): State<AppState>, Query(q): Query<ExportQuery>) -> AppResult<impl IntoResponse> {
    let lang = Lang::from_code(q.lang.as_deref());
    let rows = database::list_registrations(&state.db, Some(q.event_id), None, None).await?;
    let header = [
        "Name",
        lang.pick("E-Mail", "Email"),
        lang.pick("Telefon", "Phone"),
        lang.pick("Plätze", "Seats"),
        "Status",
        lang.pick("Anmerkung", "Comment"),
        lang.pick("Angemeldet am", "Registered at"),
        lang.pick("Storniert am", "Cancelled at"),
    ];
    let date = |d: chrono::DateTime<chrono::Utc>| local(d).format("%Y-%m-%d %H:%M").to_string();
    let mut csv = String::from("\u{feff}");
    csv.push_str(&csv_line(header.iter().map(|h| h.to_string())));
    for r in rows {
        let status = match r.status {
            RegistrationStatus::Confirmed => lang.pick("bestätigt", "confirmed"),
            RegistrationStatus::Waitlisted => lang.pick("Warteliste", "waitlisted"),
            RegistrationStatus::Cancelled => lang.pick("storniert", "cancelled"),
        };
        csv.push_str(&csv_line(
            [
                r.name,
                r.email,
                r.phone.unwrap_or_default(),
                r.seats.to_string(),
                status.into(),
                r.comment.unwrap_or_default(),
                date(r.created_at),
                r.cancelled_at.map(date).unwrap_or_default(),
            ]
            .into_iter(),
        ));
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(r#"attachment; filename="registrations-{}.csv""#, q.event_id),
            ),
        ],
        csv,
    ))
}

fn csv_line(cells: impl Iterator<Item = String>) -> String {
    let cells: Vec<String> = cells.map(|c| csv_cell(&c)).collect();
    cells.join(";") + "\r\n"
}

fn csv_cell(s: &str) -> String {
    // a leading = + - @ makes spreadsheets evaluate visitor input as a formula
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// GET /api/settings
pub async fn settings(State(state): State<AppState>) -> AppResult<Json<Settings>> {
    Ok(Json(database::get_settings(&state.db).await?))
}

/// PUT /api/settings
pub async fn save_settings(State(state): State<AppState>, Json(body): Json<Settings>) -> AppResult<Json<Settings>> {
    let clean = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let provider = clean(body.captcha_provider);
    if provider
        .as_deref()
        .is_some_and(|p| !["h-captcha", "re-captcha", "turnstile"].contains(&p))
    {
        return Err(AppError::BadRequest("unknown captcha provider".into()));
    }
    let settings = Settings {
        notify_email: clean(body.notify_email),
        captcha_provider: provider,
        captcha_sitekey: clean(body.captcha_sitekey),
        captcha_secret: clean(body.captcha_secret),
    };
    Ok(Json(database::save_settings(&state.db, &settings).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_and_defuses_formulas() {
        assert_eq!(csv_cell(r#"Ann "A" Smith"#), r#""Ann ""A"" Smith""#);
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), r#""'=HYPERLINK(""x"")""#);
        assert_eq!(
            csv_line(["a;b".to_string(), "c".to_string()].into_iter()),
            "\"a;b\";\"c\"\r\n"
        );
    }
}

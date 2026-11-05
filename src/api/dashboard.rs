use axum::{
    extract::{Query, State},
    response::Html,
};
use chrono::Utc;
use minijinja::context;
use serde::{Deserialize, Serialize};

use crate::{
    AppState, database,
    lang::Lang,
    utils::{AppResult, local},
};

#[derive(Deserialize)]
pub struct LangQuery {
    pub lang: Option<String>,
}

#[derive(Serialize)]
struct Row {
    title: String,
    date: String,
    confirmed: i64,
    capacity: Option<i32>,
    /// 0–100, for the fill bar
    percent: i64,
    waitlisted: i64,
}

/// GET /dashboard/upcoming: the next public events that take registrations, with fill level.
pub async fn upcoming(State(state): State<AppState>, Query(q): Query<LangQuery>) -> AppResult<Html<String>> {
    let lang = Lang::from_code(q.lang.as_deref());
    let now = Utc::now().naive_utc();
    let mut events: Vec<_> = database::overview(&state.db, &state.cms_db)
        .await?
        .into_iter()
        .filter(|e| e.public && e.config.as_ref().is_some_and(|c| c.enabled))
        .filter_map(|e| Some((e.event.clone()?, e)))
        .filter(|(event, _)| event.start.is_some_and(|s| s >= now))
        .collect();
    events.sort_by_key(|(event, _)| event.start);
    let rows: Vec<Row> = events
        .into_iter()
        .take(6)
        .map(|(event, e)| {
            let capacity = e.config.as_ref().and_then(|c| c.capacity);
            Row {
                date: event
                    .start_utc()
                    .map(|s| local(s).format("%d.%m.%Y %H:%M").to_string())
                    .unwrap_or_default(),
                title: event.title,
                confirmed: e.confirmed_seats,
                capacity,
                percent: capacity.map_or(0, |c| (e.confirmed_seats * 100 / i64::from(c)).min(100)),
                waitlisted: e.waitlisted_seats,
            }
        })
        .collect();
    let tmpl = state.env.get_template("dashboard_upcoming.html")?;
    Ok(Html(tmpl.render(context! {
        events => rows,
        empty => lang.pick("Keine anstehenden Veranstaltungen mit Anmeldung.", "No upcoming events with registration."),
        waitlist => lang.pick("Warteliste", "waitlist"),
        unlimited => lang.pick("unbegrenzt", "unlimited"),
    })?))
}

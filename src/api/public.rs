//! What visitors reach: the two helpers, the form submit and the cancel page.

use std::{collections::HashMap, time::Instant};

use axum::{
    Form, Json,
    extract::{Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState, antispam,
    availability::{Availability, Rejection, availability},
    database, email,
    lang::Lang,
    model::{CmsEvent, EventConfig, NewRegistration},
    render,
    utils::AppResult,
};

#[derive(Deserialize)]
pub struct PageRenderInput {
    pub language: Option<String>,
}

/// The body Neleto posts to an inline helper.
#[derive(Deserialize)]
pub struct HelperBody {
    pub json_args: Vec<Value>,
    pub page: PageRenderInput,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub route: String,
}

fn data(html: String) -> Json<Value> {
    Json(json!({ "data": html }))
}

/// The public event of the helper's first argument with its config and availability. `None`
/// when there is nothing to show, so the helper renders nothing.
async fn load(state: &AppState, body: &HelperBody) -> AppResult<Option<(CmsEvent, EventConfig, Availability)>> {
    let Some(id) = body
        .json_args
        .first()
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
    else {
        return Ok(None);
    };
    let (Some(event), Some(config)) = (
        database::cms_event(&state.cms_db, id).await?,
        database::get_config(&state.db, id).await?,
    ) else {
        return Ok(None);
    };
    let taken = database::taken_seats(&state.db, id).await?;
    let a = availability(Some(&config), event.start_utc(), Utc::now(), taken);
    Ok(Some((event, config, a)))
}

/// `{{ event_registration(event.id) }}`. Never fails the page render: errors render nothing.
pub async fn registration_helper(State(state): State<AppState>, Json(body): Json<HelperBody>) -> Json<Value> {
    let lang = Lang::from_code(body.page.language.as_deref());
    let loaded = async {
        let Some((event, config, a)) = load(&state, &body).await? else {
            return AppResult::Ok(String::new());
        };
        let settings = database::get_settings(&state.db).await?;
        let path = resolve_path(&body.route, &body.params);
        Ok(render::registration_block(event.id, &path, &a, &config, &settings, lang))
    };
    data(loaded.await.unwrap_or_else(|e| {
        tracing::error!("event_registration helper: {e}");
        String::new()
    }))
}

/// `{{ event_spots(event.id) }}`
pub async fn spots_helper(State(state): State<AppState>, Json(body): Json<HelperBody>) -> Json<Value> {
    let lang = Lang::from_code(body.page.language.as_deref());
    match load(&state, &body).await {
        Ok(Some((_, _, a))) => data(render::spots_badge(&a, lang)),
        Ok(None) => data(String::new()),
        Err(e) => {
            tracing::error!("event_spots helper: {e}");
            data(String::new())
        }
    }
}

/// `/events/:slug` with the page's params → `/events/summer-party`, the form's redirect target.
fn resolve_path(route: &str, params: &HashMap<String, String>) -> String {
    let path = route
        .split('/')
        .map(|seg| match seg.strip_prefix(':') {
            Some(key) => params.get(key.trim_end_matches('*')).cloned().unwrap_or_default(),
            None => seg.to_string(),
        })
        .collect::<Vec<_>>()
        .join("/");
    if path.is_empty() { "/".into() } else { path }
}

/// POST /event-registration/submit. Neleto forwards the urlencoded body of a page without
/// layout unchanged. Always redirects back to the event page with `?registration=<flag>`.
pub async fn submit(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<HashMap<String, String>>) -> Response {
    let redirect = match form.get("redirect") {
        // same-site paths only, no open redirect
        Some(r) if r.starts_with('/') && !r.starts_with("//") => r.split(['?', '#']).next().unwrap_or("/").to_string(),
        _ => "/".into(),
    };
    let (flag, free) = process(&state, &headers, &form).await.unwrap_or_else(|e| {
        tracing::error!("registration submit failed: {e}");
        ("error", None)
    });
    let free = free.map(|f| format!("&free={f}")).unwrap_or_default();
    Redirect::to(&format!("{redirect}?registration={flag}{free}#event-registration")).into_response()
}

async fn process(
    state: &AppState,
    headers: &HeaderMap,
    form: &HashMap<String, String>,
) -> AppResult<(&'static str, Option<i64>)> {
    let get = |key: &str| form.get(key).map(|v| v.trim()).filter(|v| !v.is_empty());
    // a bot filled the field humans don't see: look successful, store nothing
    if get("website").is_some() {
        return Ok(("confirmed", None));
    }
    if let Some(ip) = antispam::client_ip(headers)
        && !state.rate_limiter.check(ip, Instant::now())
    {
        return Ok(("slow_down", None));
    }
    let Some(event_id) = get("event_id").and_then(|v| v.parse::<i32>().ok()) else {
        return Ok(("error", None));
    };
    let Ok(seats) = get("seats").unwrap_or("1").parse::<i32>() else {
        return Ok(("invalid_seats", None));
    };
    let name = get("name").unwrap_or_default();
    let email = get("email").unwrap_or_default();
    let valid_email = email.len() <= 254
        && !email.contains(char::is_whitespace)
        && email.split_once('@').is_some_and(|(a, b)| !a.is_empty() && b.contains('.'));
    let too_long = |key: &str, max: usize| get(key).is_some_and(|v| v.chars().count() > max);
    if name.is_empty() || too_long("name", 120) || !valid_email || too_long("phone", 50) || too_long("comment", 2000) {
        return Ok(("error", None));
    }
    let settings = database::get_settings(&state.db).await?;
    if let Some((provider, _, _)) = antispam::captcha(&settings)
        && !antispam::verify_captcha(&settings, get(antispam::captcha_field(provider))).await
    {
        return Ok(("captcha", None));
    }
    let Some(event) = database::cms_event(&state.cms_db, event_id).await? else {
        return Ok(("disabled", None));
    };
    let input = NewRegistration {
        event_id,
        name: name.into(),
        email: email.into(),
        phone: get("phone").map(Into::into),
        seats,
        comment: get("comment").map(Into::into),
        language: Lang::from_code(get("lang")).code().into(),
        base_url: antispam::site_url(headers),
    };
    let registration = match database::register(&state.db, &event, &input, Utc::now()).await? {
        Ok(r) => r,
        Err(rejection) => return Ok(rejection_flag(rejection)),
    };
    email::to_registrant(&event, &registration);
    let to = database::notify_address(&state.db, event_id).await?;
    email::to_organizer(to.as_deref(), &event, &registration, &[]);
    Ok(match registration.status {
        crate::model::RegistrationStatus::Waitlisted => ("waitlisted", None),
        _ => ("confirmed", None),
    })
}

fn rejection_flag(rejection: Rejection) -> (&'static str, Option<i64>) {
    match rejection {
        Rejection::Disabled => ("disabled", None),
        Rejection::NotOpenYet => ("not_open_yet", None),
        Rejection::Closed => ("closed", None),
        Rejection::Full => ("full", None),
        Rejection::InvalidSeats => ("invalid_seats", None),
        Rejection::NotEnoughSpots { free } => ("not_enough_spots", Some(free)),
        Rejection::AlreadyRegistered => ("already_registered", None),
    }
}

fn header_lang(headers: &HeaderMap) -> Lang {
    Lang::from_code(headers.get("accept-language").and_then(|v| v.to_str().ok()))
}

#[derive(Deserialize)]
pub struct TokenParam {
    pub token: Option<String>,
}

fn token(param: &TokenParam) -> Option<Uuid> {
    param.token.as_deref().and_then(|t| t.trim().parse().ok())
}

/// GET /event-registration/cancel?token=, the link in the mails. Only asks; cancelling is a POST.
pub async fn cancel_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<TokenParam>,
) -> AppResult<Html<String>> {
    let registration = match token(&q) {
        Some(t) => database::registration_by_token(&state.db, t).await?,
        None => None,
    };
    let Some(r) = registration.filter(|r| r.status != crate::model::RegistrationStatus::Cancelled) else {
        return Ok(Html(render::cancel_result(false, header_lang(&headers))));
    };
    let title = database::cms_event_any(&state.cms_db, r.event_id)
        .await?
        .map(|e| e.title)
        .unwrap_or_default();
    Ok(Html(render::cancel_confirm(&r, &title, Lang::from_code(Some(&r.language)))))
}

/// POST /event-registration/cancel
pub async fn cancel(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<TokenParam>) -> AppResult<Html<String>> {
    let changed = match token(&form) {
        Some(t) => database::cancel(&state.db, t).await?,
        None => None,
    };
    let Some(changed) = changed else {
        return Ok(Html(render::cancel_result(false, header_lang(&headers))));
    };
    let r = &changed.registration;
    if let Some(event) = database::cms_event_any(&state.cms_db, r.event_id).await? {
        email::promoted(&event, &changed.promoted);
        let to = database::notify_address(&state.db, r.event_id).await?;
        email::to_organizer(to.as_deref(), &event, r, &changed.promoted);
    }
    Ok(Html(render::cancel_result(true, Lang::from_code(Some(&r.language)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_route_params() {
        let params = HashMap::from([("slug".to_string(), "sommerfest".to_string())]);
        assert_eq!(resolve_path("/events/:slug", &params), "/events/sommerfest");
        assert_eq!(resolve_path("", &params), "/");
    }
}

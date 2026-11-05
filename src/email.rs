//! Mails go through the Neleto backend's internal `/email/send`, like blog-comments, so the
//! plugin holds no SMTP credentials.

use serde_json::json;

/// Best-effort: callers spawn it, so a mail failure never fails a registration. No-op with a
/// warning when `INTERNAL_BACKEND_PORT` isn't set, e.g. in local dev.
pub async fn send(receiver: String, subject: String, html: String) {
    let Ok(port) = std::env::var("INTERNAL_BACKEND_PORT") else {
        tracing::warn!("INTERNAL_BACKEND_PORT not set, skipping email to {receiver}: {subject}");
        return;
    };
    let body = json!({ "receiver": receiver, "subject": subject, "content": html, "attachments": null });
    if let Err(e) = reqwest::Client::new()
        .post(format!("http://localhost:{port}/email/send"))
        .json(&body)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        tracing::warn!("failed to send email to {receiver}: {e}");
    }
}

use crate::{
    lang::Lang,
    model::{CmsEvent, Registration, RegistrationStatus},
    utils::{escape_html, local},
};

fn spawn(receiver: &str, subject: String, html: String) {
    tokio::spawn(send(receiver.to_string(), subject, html));
}

fn when(event: &CmsEvent, lang: Lang) -> String {
    event
        .start_utc()
        .map(|s| format!(" ({})", local(s).format(lang.date_fmt())))
        .unwrap_or_default()
}

fn cancel_link(r: &Registration, lang: Lang) -> String {
    let Some(base) = &r.base_url else {
        return String::new();
    };
    format!(
        r#"<p>{} <a href="{url}">{url}</a></p>"#,
        lang.pick(
            "Falls Sie nicht teilnehmen können, stornieren Sie bitte hier:",
            "If you can't attend, please cancel here:"
        ),
        url = escape_html(&format!("{base}/event-registration/cancel?token={}", r.cancel_token)),
    )
}

/// The confirmation or waitlist mail to the registrant.
pub fn to_registrant(event: &CmsEvent, r: &Registration) {
    let lang = Lang::from_code(Some(&r.language));
    let title = escape_html(&event.title);
    let (subject, text) = match r.status {
        RegistrationStatus::Waitlisted => (
            lang.pick("Warteliste", "Waitlist"),
            lang.pick(
                "Sie stehen auf der Warteliste für <strong>{t}</strong>{w}. Wir schreiben Ihnen, sobald ein Platz frei wird.",
                "You are on the waitlist for <strong>{t}</strong>{w}. We will email you as soon as a spot becomes free.",
            ),
        ),
        _ => (
            lang.pick("Anmeldung bestätigt", "Registration confirmed"),
            lang.pick(
                "Ihre Anmeldung für <strong>{t}</strong>{w} ist bestätigt ({s}).",
                "Your registration for <strong>{t}</strong>{w} is confirmed ({s}).",
            ),
        ),
    };
    let html = format!(
        "<p>{} {},</p><p>{}</p>{}",
        lang.pick("Hallo", "Hello"),
        escape_html(&r.name),
        text.replace("{t}", &title)
            .replace("{w}", &when(event, lang))
            .replace("{s}", &lang.seats(r.seats)),
        cancel_link(r, lang),
    );
    spawn(&r.email, format!("{subject}: {}", event.title), html);
}

/// To registrants who moved up from the waitlist.
pub fn promoted(event: &CmsEvent, promoted: &[Registration]) {
    for r in promoted {
        let lang = Lang::from_code(Some(&r.language));
        let html = format!(
            "<p>{} {},</p><p>{}</p>{}",
            lang.pick("Hallo", "Hello"),
            escape_html(&r.name),
            lang.pick(
                "gute Nachricht: Ein Platz ist frei geworden. Ihre Anmeldung für <strong>{t}</strong>{w} ist jetzt bestätigt ({s}).",
                "good news: a spot became free. Your registration for <strong>{t}</strong>{w} is now confirmed ({s}).",
            )
            .replace("{t}", &escape_html(&event.title))
            .replace("{w}", &when(event, lang))
            .replace("{s}", &lang.seats(r.seats)),
            cancel_link(r, lang),
        );
        let subject = lang.pick("Platz frei geworden", "A spot became free");
        spawn(&r.email, format!("{subject}: {}", event.title), html);
    }
}

/// To the event's or the plugin's notify address, on a new registration or a cancellation.
pub fn to_organizer(to: Option<&str>, event: &CmsEvent, r: &Registration, promoted: &[Registration]) {
    let Some(to) = to.filter(|t| !t.is_empty()) else {
        return;
    };
    let lang = Lang::from_code(Some(&r.language));
    let what = match r.status {
        RegistrationStatus::Confirmed => lang.pick("Neue Anmeldung", "New registration"),
        RegistrationStatus::Waitlisted => lang.pick("Neu auf der Warteliste", "New on the waitlist"),
        RegistrationStatus::Cancelled => lang.pick("Stornierung", "Cancellation"),
    };
    let row = |label: &str, value: &str| format!("<tr><td>{label}</td><td>{}</td></tr>", escape_html(value));
    let mut html = format!(
        "<p><strong>{what}</strong>: {}{}</p><table>",
        escape_html(&event.title),
        when(event, lang)
    );
    html.push_str(&row("Name", &r.name));
    html.push_str(&row(lang.pick("E-Mail", "Email"), &r.email));
    html.push_str(&row(lang.pick("Telefon", "Phone"), r.phone.as_deref().unwrap_or("")));
    html.push_str(&row(lang.pick("Plätze", "Seats"), &r.seats.to_string()));
    html.push_str(&row(lang.pick("Anmerkung", "Comment"), r.comment.as_deref().unwrap_or("")));
    html.push_str("</table>");
    if !promoted.is_empty() {
        let names: Vec<&str> = promoted.iter().map(|p| p.name.as_str()).collect();
        html.push_str(&format!(
            "<p>{} {}</p>",
            lang.pick("Von der Warteliste nachgerückt:", "Moved up from the waitlist:"),
            escape_html(&names.join(", "))
        ));
    }
    spawn(to, format!("{what}: {}", event.title), html);
}

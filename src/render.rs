//! HTML for the public helpers and the cancel page. Everything visitor- or CMS-supplied is
//! escaped here, so this module is the XSS boundary.

use crate::{
    antispam,
    availability::Availability,
    lang::Lang,
    model::{EventConfig, Registration, Settings},
    utils::{escape_html, local},
};

/// `(state class, inline style, html text)` of the spots line, `None` when registration is off.
fn spots(availability: &Availability, lang: Lang) -> Option<(&'static str, &'static str, String)> {
    const OK: &str = "background:#dcfce7;color:#166534";
    const WARN: &str = "background:#fef3c7;color:#92400e";
    const OFF: &str = "background:#f3f4f6;color:#4b5563";
    Some(match availability {
        Availability::Disabled => return None,
        Availability::Open { free: Some(free) } => ("open", OK, lang.spots_left(*free)),
        Availability::Open { free: None } => ("open", OK, lang.pick("Anmeldung offen", "Registration open").into()),
        Availability::WaitlistOnly => (
            "waitlist",
            WARN,
            lang.pick("Ausgebucht – Warteliste offen", "Fully booked – waitlist open")
                .into(),
        ),
        Availability::Full => ("full", WARN, lang.pick("Ausgebucht", "Fully booked").into()),
        Availability::NotOpenYet { opens_at } => (
            "soon",
            OFF,
            format!(
                r#"{} <time datetime="{}">{}</time>"#,
                lang.pick("Anmeldung ab", "Registration opens"),
                opens_at.to_rfc3339(),
                local(*opens_at).format(lang.date_fmt()),
            ),
        ),
        Availability::Closed => (
            "closed",
            OFF,
            lang.pick("Anmeldung geschlossen", "Registration closed").into(),
        ),
    })
}

/// `{{ event_spots(event.id) }}`. Styled inline: it is used in lists, where a `<style>` per item
/// would repeat. Themes can restyle `.er-spots--<state>`.
pub fn spots_badge(availability: &Availability, lang: Lang) -> String {
    let Some((state, style, text)) = spots(availability, lang) else {
        return String::new();
    };
    format!(
        r#"<span class="er-spots er-spots--{state}" style="display:inline-block;padding:2px 10px;border-radius:999px;font-size:.85em;font-weight:600;{style}">{text}</span>"#
    )
}

/// `{{ event_registration(event.id) }}`: free spots and the form, or why registration is closed.
/// `path` is the current page, the form redirects back there.
pub fn registration_block(
    event_id: i32,
    path: &str,
    availability: &Availability,
    config: &EventConfig,
    settings: &Settings,
    lang: Lang,
) -> String {
    let Some((state, _, text)) = spots(availability, lang) else {
        return String::new();
    };
    let mut out = String::from(STYLE);
    out.push_str(&format!(
        r#"<section id="event-registration" class="er"><p class="er-spots er-spots--{state}">{text}</p>"#
    ));
    let max_seats = match availability {
        Availability::WaitlistOnly => Some(config.max_seats_per_registration),
        // without a waitlist, more seats than are free would only be refused
        Availability::Open { free } if !config.waitlist_enabled => Some(free.map_or(config.max_seats_per_registration, |f| {
            f.min(config.max_seats_per_registration.into()) as i32
        })),
        Availability::Open { .. } => Some(config.max_seats_per_registration),
        _ => None,
    };
    if let Some(max_seats) = max_seats {
        out.push_str(&form(
            event_id,
            path,
            max_seats,
            *availability == Availability::WaitlistOnly,
            settings,
            lang,
        ));
    }
    out.push_str("</section>");
    out.push_str(&script(lang));
    out
}

fn form(event_id: i32, path: &str, max_seats: i32, waitlist: bool, settings: &Settings, lang: Lang) -> String {
    let field = |label: &str, input: String| format!(r#"<label class="er-field"><span>{label}</span>{input}</label>"#);
    let mut f = format!(
        r#"<form class="er-form" method="post" action="/event-registration/submit"><input type="hidden" name="event_id" value="{event_id}"><input type="hidden" name="redirect" value="{path}"><input type="hidden" name="lang" value="{lang}"><div class="er-hp" aria-hidden="true"><label>Website<input type="text" name="website" tabindex="-1" autocomplete="off"></label></div>"#,
        path = escape_html(path),
        lang = lang.code(),
    );
    f.push_str(&field(
        lang.pick("Name", "Name"),
        r#"<input name="name" type="text" required maxlength="120" autocomplete="name">"#.into(),
    ));
    f.push_str(&field(
        lang.pick("E-Mail", "Email"),
        r#"<input name="email" type="email" required maxlength="254" autocomplete="email">"#.into(),
    ));
    f.push_str(&field(
        lang.pick("Telefon (optional)", "Phone (optional)"),
        r#"<input name="phone" type="tel" maxlength="50" autocomplete="tel">"#.into(),
    ));
    if max_seats > 1 {
        let options: String = (1..=max_seats)
            .map(|n| format!(r#"<option value="{n}">{n}</option>"#))
            .collect();
        f.push_str(&field(
            lang.pick("Anzahl Plätze", "Number of seats"),
            format!(r#"<select name="seats">{options}</select>"#),
        ));
    } else {
        f.push_str(r#"<input type="hidden" name="seats" value="1">"#);
    }
    f.push_str(&field(
        lang.pick("Anmerkung (optional)", "Comment (optional)"),
        r#"<textarea name="comment" rows="3" maxlength="2000"></textarea>"#.into(),
    ));
    if let Some((provider, sitekey, _)) = antispam::captcha(settings) {
        let (class, src) = match provider {
            "h-captcha" => ("h-captcha", "https://js.hcaptcha.com/1/api.js"),
            "re-captcha" => ("g-recaptcha", "https://www.google.com/recaptcha/api.js"),
            _ => ("cf-turnstile", "https://challenges.cloudflare.com/turnstile/v0/api.js"),
        };
        f.push_str(&format!(
            r#"<div class="{class}" data-sitekey="{}"></div><script src="{src}" async defer></script>"#,
            escape_html(sitekey)
        ));
    }
    let submit = if waitlist {
        lang.pick("Auf die Warteliste", "Join the waitlist")
    } else {
        lang.pick("Anmelden", "Register")
    };
    f.push_str(&format!(
        r#"<button class="er-submit" type="submit">{submit}</button></form>"#
    ));
    f
}

const STYLE: &str = r#"<style>
.er { max-width: 560px; margin: 32px 0; }
.er-spots { font-weight: 600; margin: 0 0 16px; }
.er-spots--open { color: #166534; }
.er-spots--waitlist, .er-spots--full { color: #92400e; }
.er-banner { padding: 10px 14px; border-radius: 8px; margin: 0 0 16px; }
.er-banner--ok { background: #dcfce7; color: #166534; }
.er-banner--err { background: #fee2e2; color: #991b1b; }
.er-form { display: flex; flex-direction: column; gap: 12px; }
.er-field { display: flex; flex-direction: column; gap: 4px; font-size: .9em; font-weight: 600; }
.er-field input, .er-field select, .er-field textarea { padding: 9px 11px; border: 1px solid #d1d5db; border-radius: 8px; font: inherit; font-weight: 400; }
.er-hp { position: absolute; left: -9999px; width: 1px; height: 1px; overflow: hidden; }
.er-submit { align-self: flex-start; padding: 10px 18px; border: 0; border-radius: 8px; background: #111827; color: #fff; font: inherit; font-weight: 600; cursor: pointer; }
</style>"#;

/// Shows the result banner from `?registration=<flag>` in the browser: Neleto's page cache
/// ignores the query string, so a server-rendered banner would be cached for everyone.
/// `inline="true"` keeps Neleto from hoisting the script into `<head>`.
fn script(lang: Lang) -> String {
    let banners: serde_json::Map<String, serde_json::Value> = Lang::FLAGS
        .iter()
        .map(|flag| {
            let (kind, msg) = lang.banner(flag).unwrap();
            (flag.to_string(), serde_json::json!([kind, msg]))
        })
        .collect();
    SCRIPT.replace("/*BANNERS*/null", &serde_json::Value::Object(banners).to_string())
}

const SCRIPT: &str = r#"<script inline="true">
(function(){
  var B = /*BANNERS*/null;
  function show(){
    var params = new URLSearchParams(location.search);
    var b = B[params.get('registration')];
    var sec = document.getElementById('event-registration');
    if (!b || !sec) return;
    var p = document.createElement('p');
    p.className = 'er-banner er-banner--' + b[0];
    p.setAttribute('role', 'status');
    p.textContent = b[1].replace('{free}', params.get('free') || '');
    sec.insertBefore(p, sec.firstChild);
    // so a reload doesn't show it again
    params.delete('registration'); params.delete('free');
    var qs = params.toString();
    history.replaceState(null, '', location.pathname + (qs ? '?' + qs : '') + location.hash);
  }
  if (document.readyState !== 'loading') show(); else document.addEventListener('DOMContentLoaded', show);
})();
</script>"#;

/// The page behind the cancel link: a confirm button, since mail scanners follow links.
pub fn cancel_confirm(registration: &Registration, title: &str, lang: Lang) -> String {
    let seats = lang.seats(registration.seats);
    let body = format!(
        r#"<h1>{heading}</h1><p>{intro} <strong>{title}</strong><br>{name} · {seats}</p><form method="post" action="/event-registration/cancel"><input type="hidden" name="token" value="{token}"><button type="submit">{button}</button></form>"#,
        heading = lang.pick("Anmeldung stornieren", "Cancel registration"),
        intro = lang.pick(
            "Möchten Sie Ihre Anmeldung stornieren?",
            "Do you want to cancel your registration?"
        ),
        title = escape_html(title),
        name = escape_html(&registration.name),
        token = registration.cancel_token,
        button = lang.pick("Jetzt stornieren", "Cancel now"),
    );
    page(&body, lang)
}

/// `cancelled: false` = unknown token or already cancelled.
pub fn cancel_result(cancelled: bool, lang: Lang) -> String {
    let body = if cancelled {
        format!(
            "<h1>{}</h1><p>{}</p>",
            lang.pick("Anmeldung storniert", "Registration cancelled"),
            lang.pick(
                "Ihre Anmeldung wurde storniert. Vielen Dank für die Rückmeldung.",
                "Your registration was cancelled. Thank you for letting us know."
            ),
        )
    } else {
        format!(
            "<h1>{}</h1><p>{}</p>",
            lang.pick("Link ungültig", "Invalid link"),
            lang.pick(
                "Diese Anmeldung gibt es nicht oder sie wurde bereits storniert.",
                "This registration doesn't exist or was already cancelled."
            ),
        )
    };
    page(&body, lang)
}

/// The cancel page has no CMS layout (`allow_select_layout: false`), so it is a whole document.
fn page(body: &str, lang: Lang) -> String {
    format!(
        r#"<!doctype html><html lang="{}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="robots" content="noindex"><title>{}</title><style>
body {{ font-family: system-ui, -apple-system, sans-serif; background: #f9fafb; color: #111827; margin: 0; padding: 48px 16px; }}
main {{ max-width: 480px; margin: 0 auto; background: #fff; border: 1px solid #e5e7eb; border-radius: 12px; padding: 24px; }}
h1 {{ font-size: 20px; margin: 0 0 12px; }}
p {{ line-height: 1.5; }}
button {{ padding: 10px 18px; border: 0; border-radius: 8px; background: #b91c1c; color: #fff; font: inherit; font-weight: 600; cursor: pointer; }}
</style></head><body><main>{}</main></body></html>"#,
        lang.code(),
        lang.pick("Anmeldung stornieren", "Cancel registration"),
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn config(max_seats: i32, waitlist: bool) -> EventConfig {
        EventConfig {
            event_id: 3,
            enabled: true,
            capacity: Some(10),
            max_seats_per_registration: max_seats,
            waitlist_enabled: waitlist,
            opens_at: None,
            closes_at: None,
            notify_email: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn block(a: Availability, c: &EventConfig, s: &Settings) -> String {
        registration_block(3, "/events/<x>", &a, c, s, Lang::De)
    }

    #[test]
    fn form_carries_event_redirect_and_honeypot() {
        let out = block(Availability::Open { free: Some(12) }, &config(1, false), &Settings::default());
        assert!(out.contains("Noch 12 Plätze frei"));
        assert!(out.contains(r#"name="event_id" value="3""#));
        assert!(out.contains(r#"name="redirect" value="/events/&lt;x&gt;""#));
        assert!(out.contains(r#"name="website""#));
        assert!(out.contains(r#"<input type="hidden" name="seats" value="1">"#));
        assert!(out.contains(">Anmelden</button>"));
    }

    #[test]
    fn seat_choice_is_capped_by_free_spots_without_waitlist() {
        let out = block(Availability::Open { free: Some(2) }, &config(4, false), &Settings::default());
        assert!(out.contains(r#"<option value="2">"#) && !out.contains(r#"<option value="3">"#));
        let out = block(Availability::Open { free: Some(2) }, &config(4, true), &Settings::default());
        assert!(out.contains(r#"<option value="4">"#));
    }

    #[test]
    fn no_form_when_closed_or_full() {
        for a in [
            Availability::Full,
            Availability::Closed,
            Availability::NotOpenYet { opens_at: Utc::now() },
        ] {
            let out = block(a, &config(1, false), &Settings::default());
            assert!(!out.contains("<form"), "{a:?}");
        }
        assert_eq!(block(Availability::Disabled, &config(1, false), &Settings::default()), "");
        let out = block(Availability::WaitlistOnly, &config(1, true), &Settings::default());
        assert!(out.contains(">Auf die Warteliste</button>"));
    }

    #[test]
    fn captcha_widget_only_when_configured() {
        let s = Settings {
            captcha_provider: Some("turnstile".into()),
            captcha_sitekey: Some("KEY\"".into()),
            captcha_secret: Some("secret".into()),
            ..Settings::default()
        };
        let out = block(Availability::Open { free: None }, &config(1, false), &s);
        assert!(out.contains(r#"class="cf-turnstile" data-sitekey="KEY&quot;""#));
        assert!(!out.contains("secret"));
    }

    #[test]
    fn badge_states() {
        assert_eq!(spots_badge(&Availability::Disabled, Lang::De), "");
        assert!(spots_badge(&Availability::Open { free: Some(1) }, Lang::En).contains("1 spot left"));
        assert!(spots_badge(&Availability::Full, Lang::De).contains("er-spots--full"));
    }

    #[test]
    fn cancel_page_escapes() {
        let r = Registration {
            id: uuid::Uuid::new_v4(),
            event_id: 3,
            name: "<b>Ann</b>".into(),
            email: "a@example.com".into(),
            phone: None,
            seats: 2,
            comment: None,
            status: crate::model::RegistrationStatus::Confirmed,
            cancel_token: uuid::Uuid::new_v4(),
            created_at: Utc::now(),
            cancelled_at: None,
            language: "de".into(),
            base_url: None,
        };
        let out = cancel_confirm(&r, "Fest & <Feier>", Lang::De);
        assert!(out.contains("&lt;b&gt;Ann") && out.contains("Fest &amp; &lt;Feier&gt;") && out.contains("2 Plätze"));
        assert!(out.contains(&r.cancel_token.to_string()));
    }
}

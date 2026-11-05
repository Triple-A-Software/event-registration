//! The visitor-facing language, from the CMS page (`page.language` in the helper request) and
//! stored on each registration for later mails. German or English, like blog-comments.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    De,
}

impl Lang {
    /// `"de"`, `"de-DE"`, … are German; anything else is English.
    pub fn from_code(code: Option<&str>) -> Self {
        match code {
            Some(c) if c.trim().to_ascii_lowercase().starts_with("de") => Lang::De,
            _ => Lang::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    pub fn pick(self, de: &'static str, en: &'static str) -> &'static str {
        match self {
            Lang::De => de,
            Lang::En => en,
        }
    }

    pub fn spots_left(self, free: i64) -> String {
        match (self, free) {
            (Lang::De, 1) => "Noch 1 Platz frei".into(),
            (Lang::De, n) => format!("Noch {n} Plätze frei"),
            (Lang::En, 1) => "1 spot left".into(),
            (Lang::En, n) => format!("{n} spots left"),
        }
    }

    pub fn seats(self, n: i32) -> String {
        match (self, n) {
            (Lang::De, 1) => "1 Platz".into(),
            (Lang::De, n) => format!("{n} Plätze"),
            (Lang::En, 1) => "1 seat".into(),
            (Lang::En, n) => format!("{n} seats"),
        }
    }

    /// chrono format for dates in mails
    pub fn date_fmt(self) -> &'static str {
        self.pick("%d.%m.%Y, %H:%M Uhr", "%b %-d, %Y, %H:%M")
    }

    /// `(kind, message)` for the `?registration=<flag>` the submit redirects back with. `{free}`
    /// is replaced in the browser by the `free` query param.
    pub fn banner(self, flag: &str) -> Option<(&'static str, &'static str)> {
        let ok = |de, en| Some(("ok", self.pick(de, en)));
        let err = |de, en| Some(("err", self.pick(de, en)));
        match flag {
            "confirmed" => ok(
                "Vielen Dank! Ihre Anmeldung ist bestätigt. Sie erhalten eine E-Mail mit allen Details.",
                "Thank you! Your registration is confirmed. You will receive an email with the details.",
            ),
            "waitlisted" => ok(
                "Sie stehen auf der Warteliste. Wir benachrichtigen Sie per E-Mail, sobald ein Platz frei wird.",
                "You are on the waitlist. We will email you as soon as a spot becomes free.",
            ),
            "already_registered" => err(
                "Mit dieser E-Mail-Adresse besteht bereits eine Anmeldung.",
                "There already is a registration with this email address.",
            ),
            "not_enough_spots" => err("Es sind nur noch {free} Plätze frei.", "Only {free} spots are left."),
            "full" => err(
                "Die Veranstaltung ist leider ausgebucht.",
                "Sorry, the event is fully booked.",
            ),
            "closed" | "disabled" => err("Die Anmeldung ist geschlossen.", "Registration is closed."),
            "not_open_yet" => err("Die Anmeldung ist noch nicht geöffnet.", "Registration is not open yet."),
            "invalid_seats" => err(
                "Bitte wählen Sie eine gültige Anzahl an Plätzen.",
                "Please choose a valid number of seats.",
            ),
            "captcha" => err(
                "Bitte bestätigen Sie, dass Sie kein Roboter sind, und senden Sie erneut.",
                "Please confirm that you are not a robot and submit again.",
            ),
            "slow_down" => err(
                "Zu viele Anmeldungen in kurzer Zeit. Bitte versuchen Sie es später noch einmal.",
                "Too many registrations in a short time. Please try again later.",
            ),
            "error" => err(
                "Ihre Anmeldung konnte nicht gespeichert werden. Bitte prüfen Sie Ihre Eingaben.",
                "Your registration could not be saved. Please check your input.",
            ),
            _ => None,
        }
    }

    pub const FLAGS: [&'static str; 12] = [
        "confirmed",
        "waitlisted",
        "already_registered",
        "not_enough_spots",
        "full",
        "closed",
        "disabled",
        "not_open_yet",
        "invalid_seats",
        "captcha",
        "slow_down",
        "error",
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flag_has_a_banner() {
        for lang in [Lang::De, Lang::En] {
            for flag in Lang::FLAGS {
                assert!(lang.banner(flag).is_some(), "{flag}");
            }
        }
        assert_eq!(Lang::from_code(Some("de-AT")), Lang::De);
        assert_eq!(Lang::from_code(None), Lang::En);
    }
}

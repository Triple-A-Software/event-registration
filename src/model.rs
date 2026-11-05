use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The parts of a CMS `event` row the plugin needs. Read from `CMS_DATABASE_URL`.
#[derive(sqlx::FromRow, Serialize, Clone, Debug)]
pub struct CmsEvent {
    pub id: i32,
    pub slug: String,
    pub title: String,
    /// the CMS stores `timestamp` without time zone, holding UTC
    pub start: Option<NaiveDateTime>,
    pub end: Option<NaiveDateTime>,
}

impl CmsEvent {
    pub fn start_utc(&self) -> Option<DateTime<Utc>> {
        self.start.map(|s| s.and_utc())
    }
}

#[derive(sqlx::Type, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[sqlx(type_name = "registration_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum RegistrationStatus {
    Confirmed,
    Waitlisted,
    Cancelled,
}

/// Registration options of one CMS event, see `migrations/*_init.sql`.
#[derive(sqlx::FromRow, Serialize, Clone, Debug)]
pub struct EventConfig {
    pub event_id: i32,
    pub enabled: bool,
    /// `None` = unlimited
    pub capacity: Option<i32>,
    pub max_seats_per_registration: i32,
    pub waitlist_enabled: bool,
    pub opens_at: Option<DateTime<Utc>>,
    /// `None` = closes when the event starts
    pub closes_at: Option<DateTime<Utc>>,
    pub notify_email: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow, Serialize, Clone, Debug)]
pub struct Registration {
    pub id: Uuid,
    pub event_id: i32,
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub seats: i32,
    pub comment: Option<String>,
    pub status: RegistrationStatus,
    #[serde(skip)]
    pub cancel_token: Uuid,
    pub created_at: DateTime<Utc>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

/// What a visitor submits through the public registration form.
#[derive(Deserialize, Clone, Debug)]
pub struct NewRegistration {
    pub event_id: i32,
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub seats: i32,
    pub comment: Option<String>,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub notify_email: Option<String>,
    pub captcha_provider: Option<String>,
    pub captcha_sitekey: Option<String>,
    pub captcha_secret: Option<String>,
}

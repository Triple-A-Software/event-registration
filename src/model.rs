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
    /// page language at submit, for the mails
    pub language: String,
    /// site origin at submit (`https://example.com`), for the cancel link in later mails
    #[serde(skip)]
    pub base_url: Option<String>,
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
    pub language: String,
    pub base_url: Option<String>,
}

/// What the admin saves for an event, see [`EventConfig`].
#[derive(Deserialize, Clone, Debug)]
pub struct EventConfigInput {
    pub enabled: bool,
    pub capacity: Option<i32>,
    pub max_seats_per_registration: i32,
    pub waitlist_enabled: bool,
    pub opens_at: Option<DateTime<Utc>>,
    pub closes_at: Option<DateTime<Utc>>,
    pub notify_email: Option<String>,
}

/// One row of the admin's event list. `event: None` = the CMS event was deleted, only its
/// config and registrations are left.
#[derive(Serialize, Clone, Debug)]
pub struct EventOverview {
    pub id: i32,
    pub event: Option<CmsEvent>,
    pub public: bool,
    pub config: Option<EventConfig>,
    pub confirmed_seats: i64,
    pub waitlisted_seats: i64,
    /// not cancelled
    pub active_registrations: i64,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Default, Clone, Debug)]
pub struct Settings {
    pub notify_email: Option<String>,
    pub captcha_provider: Option<String>,
    pub captcha_sitekey: Option<String>,
    pub captcha_secret: Option<String>,
}

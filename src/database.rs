use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    availability::{Rejection, decide, free_spots, to_promote},
    model::{
        CmsEvent, EventConfig, EventConfigInput, EventOverview, NewRegistration, Registration, RegistrationStatus,
        Settings,
    },
};

/// A public, non-deleted CMS event.
pub async fn cms_event(cms_db: &PgPool, id: i32) -> sqlx::Result<Option<CmsEvent>> {
    sqlx::query_as(
        r#"select id, slug, title, "start", "end" from "event"
           where id = $1 and status = 'public' and deleted_at is null"#,
    )
    .bind(id)
    .fetch_optional(cms_db)
    .await
}

/// A non-deleted CMS event of any status, for the admin and for mails.
pub async fn cms_event_any(cms_db: &PgPool, id: i32) -> sqlx::Result<Option<CmsEvent>> {
    sqlx::query_as(r#"select id, slug, title, "start", "end" from "event" where id = $1 and deleted_at is null"#)
        .bind(id)
        .fetch_optional(cms_db)
        .await
}

pub async fn get_config(db: &PgPool, event_id: i32) -> sqlx::Result<Option<EventConfig>> {
    sqlx::query_as("select * from event_config where event_id = $1")
        .bind(event_id)
        .fetch_optional(db)
        .await
}

/// Seats held by confirmed registrations, for showing free spots. Writes use the locked variant.
pub async fn taken_seats(db: &PgPool, event_id: i32) -> sqlx::Result<i64> {
    confirmed_seats(&mut *db.acquire().await?, event_id).await
}

/// Locks the event's config row. Every write that changes who holds a spot takes this lock
/// first, so registrations and cancellations of one event run one after the other.
async fn lock_config(conn: &mut PgConnection, event_id: i32) -> sqlx::Result<Option<EventConfig>> {
    sqlx::query_as("select * from event_config where event_id = $1 for update")
        .bind(event_id)
        .fetch_optional(conn)
        .await
}

async fn confirmed_seats(conn: &mut PgConnection, event_id: i32) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select coalesce(sum(seats), 0)::bigint from registration where event_id = $1 and status = 'confirmed'",
    )
    .bind(event_id)
    .fetch_one(conn)
    .await
}

pub async fn register(
    db: &PgPool,
    event: &CmsEvent,
    input: &NewRegistration,
    now: DateTime<Utc>,
) -> sqlx::Result<Result<Registration, Rejection>> {
    let mut tx = db.begin().await?;
    let config = lock_config(&mut tx, event.id).await?;
    let taken = confirmed_seats(&mut tx, event.id).await?;
    let status = match decide(config.as_ref(), event.start_utc(), now, taken, input.seats) {
        Ok(status) => status,
        Err(rejection) => return Ok(Err(rejection)),
    };
    let inserted = sqlx::query_as(
        r#"insert into registration (event_id, name, email, phone, seats, comment, status, language, base_url)
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9) returning *"#,
    )
    .bind(event.id)
    .bind(input.name.trim())
    .bind(input.email.trim())
    .bind(input.phone.as_deref().map(str::trim).filter(|p| !p.is_empty()))
    .bind(input.seats)
    .bind(input.comment.as_deref().map(str::trim).filter(|c| !c.is_empty()))
    .bind(status)
    .bind(&input.language)
    .bind(&input.base_url)
    .fetch_one(&mut *tx)
    .await;
    match inserted {
        Ok(registration) => {
            tx.commit().await?;
            Ok(Ok(registration))
        }
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(Err(Rejection::AlreadyRegistered)),
        Err(e) => Err(e),
    }
}

/// Moves waitlisted registrations up into free spots, see [`to_promote`]. Expects the config row
/// to be locked by the caller's transaction.
async fn promote_waitlist(conn: &mut PgConnection, config: &EventConfig) -> sqlx::Result<Vec<Registration>> {
    let free = free_spots(config, confirmed_seats(conn, config.event_id).await?);
    let waitlist: Vec<(Uuid, i32)> = sqlx::query_as(
        "select id, seats from registration where event_id = $1 and status = 'waitlisted' order by created_at",
    )
    .bind(config.event_id)
    .fetch_all(&mut *conn)
    .await?;
    let ids = to_promote(free, &waitlist);
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as("update registration set status = 'confirmed' where id = any($1) returning *")
        .bind(ids)
        .fetch_all(conn)
        .await
}

#[derive(Debug)]
pub struct Changed {
    pub registration: Registration,
    /// moved up from the waitlist into the freed spots, to be notified
    pub promoted: Vec<Registration>,
}

/// Begins a transaction holding the event's config lock, see [`lock_config`].
async fn begin_locked(db: &PgPool, event_id: i32) -> sqlx::Result<Option<(Transaction<'static, Postgres>, EventConfig)>> {
    let mut tx = db.begin().await?;
    Ok(lock_config(&mut tx, event_id).await?.map(|config| (tx, config)))
}

async fn promote_and_commit(mut tx: Transaction<'static, Postgres>, config: &EventConfig) -> sqlx::Result<Vec<Registration>> {
    let promoted = if config.waitlist_enabled {
        promote_waitlist(&mut tx, config).await?
    } else {
        Vec::new()
    };
    tx.commit().await?;
    Ok(promoted)
}

async fn event_of(db: &PgPool, column: &str, value: Uuid) -> sqlx::Result<Option<i32>> {
    // column is one of two literals, never input
    sqlx::query_scalar(&format!("select event_id from registration where {column} = $1"))
        .bind(value)
        .fetch_optional(db)
        .await
}

/// Cancels by the token from the confirmation email. `None` if the token is unknown or the
/// registration was already cancelled.
pub async fn cancel(db: &PgPool, token: Uuid) -> sqlx::Result<Option<Changed>> {
    let Some(event_id) = event_of(db, "cancel_token", token).await? else {
        return Ok(None);
    };
    // config first, like `register`, so the two can't deadlock
    let Some((mut tx, config)) = begin_locked(db, event_id).await? else {
        return Ok(None);
    };
    let Some(registration) = sqlx::query_as::<_, Registration>(
        r#"update registration set status = 'cancelled', cancelled_at = now()
           where cancel_token = $1 and status <> 'cancelled' returning *"#,
    )
    .bind(token)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(None);
    };
    let promoted = promote_and_commit(tx, &config).await?;
    Ok(Some(Changed { registration, promoted }))
}

pub async fn registration_by_token(db: &PgPool, token: Uuid) -> sqlx::Result<Option<Registration>> {
    sqlx::query_as("select * from registration where cancel_token = $1")
        .bind(token)
        .fetch_optional(db)
        .await
}

/// Admin status change, returns the previous status too. The admin may overbook, so there is no capacity check. Only a
/// cancellation fills the freed spots from the waitlist: moving someone to the waitlist by hand
/// must not move them straight back up.
pub async fn set_status(
    db: &PgPool,
    id: Uuid,
    status: RegistrationStatus,
) -> sqlx::Result<Result<Option<(Changed, RegistrationStatus)>, Rejection>> {
    let Some(event_id) = event_of(db, "id", id).await? else {
        return Ok(Ok(None));
    };
    let Some((mut tx, config)) = begin_locked(db, event_id).await? else {
        return Ok(Ok(None));
    };
    // safe to read before the update: status changes all hold the config lock
    let previous: RegistrationStatus = sqlx::query_scalar("select status from registration where id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let updated = sqlx::query_as::<_, Registration>(
        r#"update registration
           set status = $2, cancelled_at = case when $2 = 'cancelled' then coalesce(cancelled_at, now()) end
           where id = $1 returning *"#,
    )
    .bind(id)
    .bind(status)
    .fetch_one(&mut *tx)
    .await;
    let registration = match updated {
        Ok(r) => r,
        // reactivating an address that registered again in the meantime
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => return Ok(Err(Rejection::AlreadyRegistered)),
        Err(e) => return Err(e),
    };
    let promoted = if status == RegistrationStatus::Cancelled {
        promote_and_commit(tx, &config).await?
    } else {
        tx.commit().await?;
        Vec::new()
    };
    Ok(Ok(Some((Changed { registration, promoted }, previous))))
}

pub async fn delete_registration(db: &PgPool, id: Uuid) -> sqlx::Result<Option<Changed>> {
    let Some(event_id) = event_of(db, "id", id).await? else {
        return Ok(None);
    };
    let Some((mut tx, config)) = begin_locked(db, event_id).await? else {
        return Ok(None);
    };
    let Some(registration) = sqlx::query_as::<_, Registration>("delete from registration where id = $1 returning *")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(None);
    };
    let promoted = promote_and_commit(tx, &config).await?;
    Ok(Some(Changed { registration, promoted }))
}

/// Creates or updates an event's config. A raised capacity (or a newly enabled waitlist) fills
/// the free spots from the waitlist right away. The upsert locks the row like [`lock_config`].
pub async fn save_config(
    db: &PgPool,
    event_id: i32,
    input: &EventConfigInput,
) -> sqlx::Result<(EventConfig, Vec<Registration>)> {
    let mut tx = db.begin().await?;
    let config: EventConfig = sqlx::query_as(
        r#"insert into event_config
             (event_id, enabled, capacity, max_seats_per_registration, waitlist_enabled, opens_at, closes_at, notify_email)
           values ($1, $2, $3, $4, $5, $6, $7, $8)
           on conflict (event_id) do update set
             enabled = excluded.enabled, capacity = excluded.capacity,
             max_seats_per_registration = excluded.max_seats_per_registration,
             waitlist_enabled = excluded.waitlist_enabled, opens_at = excluded.opens_at,
             closes_at = excluded.closes_at, notify_email = excluded.notify_email, updated_at = now()
           returning *"#,
    )
    .bind(event_id)
    .bind(input.enabled)
    .bind(input.capacity)
    .bind(input.max_seats_per_registration)
    .bind(input.waitlist_enabled)
    .bind(input.opens_at)
    .bind(input.closes_at)
    .bind(input.notify_email.as_deref().map(str::trim).filter(|e| !e.is_empty()))
    .fetch_one(&mut *tx)
    .await?;
    let promoted = promote_and_commit(tx, &config).await?;
    Ok((config, promoted))
}

/// All CMS events plus configs whose event was deleted, with their seat counts, newest first.
pub async fn overview(db: &PgPool, cms_db: &PgPool) -> sqlx::Result<Vec<EventOverview>> {
    let events: Vec<(i32, String, String, Option<chrono::NaiveDateTime>, Option<chrono::NaiveDateTime>, bool)> =
        sqlx::query_as(
            r#"select id, slug, title, "start", "end", status::text = 'public' from "event"
               where deleted_at is null order by "start" desc nulls last, id desc"#,
        )
        .fetch_all(cms_db)
        .await?;
    let mut configs: HashMap<i32, EventConfig> = sqlx::query_as::<_, EventConfig>("select * from event_config")
        .fetch_all(db)
        .await?
        .into_iter()
        .map(|c| (c.event_id, c))
        .collect();
    let counts: HashMap<i32, (i64, i64, i64)> = sqlx::query_as::<_, (i32, i64, i64, i64)>(
        r#"select event_id,
             coalesce(sum(seats) filter (where status = 'confirmed'), 0)::bigint,
             coalesce(sum(seats) filter (where status = 'waitlisted'), 0)::bigint,
             count(*) filter (where status <> 'cancelled')
           from registration group by event_id"#,
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|(id, c, w, a)| (id, (c, w, a)))
    .collect();
    let row = |id: i32, event: Option<CmsEvent>, public: bool, config: Option<EventConfig>| {
        let (confirmed_seats, waitlisted_seats, active_registrations) = counts.get(&id).copied().unwrap_or_default();
        EventOverview { id, event, public, config, confirmed_seats, waitlisted_seats, active_registrations }
    };
    let mut rows: Vec<EventOverview> = events
        .into_iter()
        .map(|(id, slug, title, start, end, public)| {
            let config = configs.remove(&id);
            row(id, Some(CmsEvent { id, slug, title, start, end }), public, config)
        })
        .collect();
    let mut orphans: Vec<EventConfig> = configs.into_values().collect();
    orphans.sort_by_key(|c| std::cmp::Reverse(c.event_id));
    rows.extend(orphans.into_iter().map(|c| row(c.event_id, None, false, Some(c))));
    Ok(rows)
}

/// `search` matches name or email.
pub async fn list_registrations(
    db: &PgPool,
    event_id: Option<i32>,
    status: Option<RegistrationStatus>,
    search: Option<&str>,
) -> sqlx::Result<Vec<Registration>> {
    sqlx::query_as(
        r#"select * from registration
           where ($1::integer is null or event_id = $1)
             and ($2::registration_status is null or status = $2)
             and ($3::text is null or name ilike '%' || $3 || '%' or email ilike '%' || $3 || '%')
           order by created_at"#,
    )
    .bind(event_id)
    .bind(status)
    .bind(search)
    .fetch_all(db)
    .await
}

/// The event's notify address, else the plugin's.
pub async fn notify_address(db: &PgPool, event_id: i32) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "select coalesce((select notify_email from event_config where event_id = $1), (select notify_email from settings))",
    )
    .bind(event_id)
    .fetch_one(db)
    .await
}

pub async fn get_settings(db: &PgPool) -> sqlx::Result<Settings> {
    sqlx::query_as("select notify_email, captcha_provider, captcha_sitekey, captcha_secret from settings")
        .fetch_one(db)
        .await
}

pub async fn save_settings(db: &PgPool, s: &Settings) -> sqlx::Result<Settings> {
    sqlx::query_as(
        r#"update settings set notify_email = $1, captcha_provider = $2, captcha_sitekey = $3, captcha_secret = $4
           returning notify_email, captcha_provider, captcha_sitekey, captcha_secret"#,
    )
    .bind(&s.notify_email)
    .bind(&s.captcha_provider)
    .bind(&s.captcha_sitekey)
    .bind(&s.captcha_secret)
    .fetch_one(db)
    .await
}

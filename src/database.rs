use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    availability::{Rejection, decide, free_spots, to_promote},
    model::{CmsEvent, EventConfig, NewRegistration, Registration},
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
        r#"insert into registration (event_id, name, email, phone, seats, comment, status)
           values ($1, $2, $3, $4, $5, $6, $7) returning *"#,
    )
    .bind(event.id)
    .bind(input.name.trim())
    .bind(input.email.trim())
    .bind(input.phone.as_deref().map(str::trim).filter(|p| !p.is_empty()))
    .bind(input.seats)
    .bind(input.comment.as_deref().map(str::trim).filter(|c| !c.is_empty()))
    .bind(status)
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

pub struct Cancelled {
    pub registration: Registration,
    /// moved up from the waitlist into the freed spots, to be notified
    pub promoted: Vec<Registration>,
}

/// Cancels by the token from the confirmation email. `None` if the token is unknown or the
/// registration was already cancelled.
pub async fn cancel(db: &PgPool, token: Uuid) -> sqlx::Result<Option<Cancelled>> {
    let Some(event_id) = sqlx::query_scalar::<_, i32>("select event_id from registration where cancel_token = $1")
        .bind(token)
        .fetch_optional(db)
        .await?
    else {
        return Ok(None);
    };
    let mut tx = db.begin().await?;
    // config first, like `register`, so the two can't deadlock
    let Some(config) = lock_config(&mut tx, event_id).await? else {
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
    let promoted = if config.waitlist_enabled {
        promote_waitlist(&mut tx, &config).await?
    } else {
        Vec::new()
    };
    tx.commit().await?;
    Ok(Some(Cancelled { registration, promoted }))
}

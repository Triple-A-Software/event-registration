//! Needs a Postgres in DATABASE_URL: `cargo test -- --ignored`. `sqlx::test` creates a fresh
//! database per test and runs the migrations.

use chrono::{Duration, Utc};
use event_registration::{
    availability::Rejection,
    database::{cancel, register},
    model::{CmsEvent, NewRegistration, RegistrationStatus},
};
use sqlx::PgPool;

fn event() -> CmsEvent {
    CmsEvent {
        id: 1,
        slug: "event".into(),
        title: "Event".into(),
        start: Some((Utc::now() + Duration::days(7)).naive_utc()),
        end: None,
    }
}

fn input(email: &str, seats: i32) -> NewRegistration {
    NewRegistration {
        event_id: 1,
        name: "Name".into(),
        email: email.into(),
        phone: None,
        seats,
        comment: None,
    }
}

async fn configure(db: &PgPool, capacity: i32, waitlist: bool) {
    sqlx::query(
        "insert into event_config (event_id, enabled, capacity, max_seats_per_registration, waitlist_enabled) values (1, true, $1, 4, $2)",
    )
    .bind(capacity)
    .bind(waitlist)
    .execute(db)
    .await
    .unwrap();
}

#[ignore = "needs DATABASE_URL"]
#[sqlx::test]
async fn waitlist_moves_up_on_cancel(db: PgPool) {
    configure(&db, 2, true).await;
    let event = event();
    let a = register(&db, &event, &input("a@example.com", 2), Utc::now()).await.unwrap().unwrap();
    assert_eq!(a.status, RegistrationStatus::Confirmed);
    let b = register(&db, &event, &input("b@example.com", 1), Utc::now()).await.unwrap().unwrap();
    assert_eq!(b.status, RegistrationStatus::Waitlisted);
    let again = register(&db, &event, &input("A@example.com", 1), Utc::now()).await.unwrap();
    assert_eq!(again.unwrap_err(), Rejection::AlreadyRegistered);

    let cancelled = cancel(&db, a.cancel_token).await.unwrap().unwrap();
    assert_eq!(cancelled.promoted.len(), 1);
    assert_eq!(cancelled.promoted[0].id, b.id);
    assert!(cancel(&db, a.cancel_token).await.unwrap().is_none());
    // a cancelled address may register again
    let a2 = register(&db, &event, &input("a@example.com", 1), Utc::now()).await.unwrap().unwrap();
    assert_eq!(a2.status, RegistrationStatus::Confirmed);
}

#[ignore = "needs DATABASE_URL"]
#[sqlx::test]
async fn concurrent_registrations_never_overbook(db: PgPool) {
    configure(&db, 3, false).await;
    let event = event();
    let tasks: Vec<_> = (0..20)
        .map(|i| {
            let (db, event) = (db.clone(), event.clone());
            tokio::spawn(async move { register(&db, &event, &input(&format!("{i}@example.com"), 1), Utc::now()).await })
        })
        .collect();
    let mut confirmed = 0;
    for task in tasks {
        match task.await.unwrap().unwrap() {
            Ok(r) => {
                assert_eq!(r.status, RegistrationStatus::Confirmed);
                confirmed += 1;
            }
            Err(rejection) => assert_eq!(rejection, Rejection::Full),
        }
    }
    assert_eq!(confirmed, 3);
}

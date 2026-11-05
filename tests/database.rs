//! Needs a Postgres in DATABASE_URL: `cargo test -- --ignored`. `sqlx::test` creates a fresh
//! database per test and runs the migrations.

use chrono::{Duration, Utc};
use event_registration::{
    availability::Rejection,
    database::{cancel, delete_registration, register, save_config, set_status},
    model::{CmsEvent, EventConfigInput, NewRegistration, RegistrationStatus},
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
        language: "de".into(),
        base_url: None,
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

#[ignore = "needs DATABASE_URL"]
#[sqlx::test]
async fn admin_changes_fill_freed_spots(db: PgPool) {
    configure(&db, 1, true).await;
    let event = event();
    let reg = |email: &'static str| {
        let (db, event) = (db.clone(), event.clone());
        async move { register(&db, &event, &input(email, 1), Utc::now()).await.unwrap().unwrap() }
    };
    let a = reg("a@example.com").await;
    let b = reg("b@example.com").await;
    let c = reg("c@example.com").await;
    assert_eq!((b.status, c.status), (RegistrationStatus::Waitlisted, RegistrationStatus::Waitlisted));

    // admin cancel promotes the next in line
    let (changed, previous) = set_status(&db, a.id, RegistrationStatus::Cancelled).await.unwrap().unwrap().unwrap();
    assert_eq!(previous, RegistrationStatus::Confirmed);
    assert_eq!(changed.promoted.iter().map(|r| r.id).collect::<Vec<_>>(), vec![b.id]);

    // moving someone to the waitlist by hand does not move them straight back up
    let (changed, _) = set_status(&db, b.id, RegistrationStatus::Waitlisted).await.unwrap().unwrap().unwrap();
    assert!(changed.promoted.is_empty());

    // raising the capacity promotes
    let config = EventConfigInput {
        enabled: true,
        capacity: Some(2),
        max_seats_per_registration: 4,
        waitlist_enabled: true,
        opens_at: None,
        closes_at: None,
        notify_email: Some("  ".into()),
    };
    let (saved, promoted) = save_config(&db, 1, &config).await.unwrap();
    assert_eq!(saved.notify_email, None);
    let mut ids: Vec<_> = promoted.iter().map(|r| r.id).collect();
    ids.sort();
    let mut expected = vec![b.id, c.id];
    expected.sort();
    assert_eq!(ids, expected);

    // reactivating a cancelled address that registered again is refused
    let a2 = reg("a@example.com").await;
    assert_eq!(a2.status, RegistrationStatus::Waitlisted);
    assert_eq!(set_status(&db, a.id, RegistrationStatus::Confirmed).await.unwrap().unwrap_err(), Rejection::AlreadyRegistered);

    // deleting a confirmed registration promotes too
    let deleted = delete_registration(&db, b.id).await.unwrap().unwrap();
    assert_eq!(deleted.promoted.iter().map(|r| r.id).collect::<Vec<_>>(), vec![a2.id]);
    assert!(delete_registration(&db, b.id).await.unwrap().is_none());
}

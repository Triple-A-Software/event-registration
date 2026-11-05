//! The capacity rules, kept free of IO so they can be tested. `database.rs` runs them inside a
//! transaction that holds the event's config row locked, so two visitors can't take the last spot.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::model::{EventConfig, RegistrationStatus};

/// What the public helpers show for an event.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Availability {
    Disabled,
    NotOpenYet { opens_at: DateTime<Utc> },
    Closed,
    /// `free: None` = unlimited
    Open { free: Option<i64> },
    WaitlistOnly,
    Full,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    Disabled,
    NotOpenYet,
    Closed,
    Full,
    InvalidSeats,
    NotEnoughSpots { free: i64 },
    AlreadyRegistered,
}

pub fn free_spots(config: &EventConfig, confirmed_seats: i64) -> Option<i64> {
    config.capacity.map(|c| (i64::from(c) - confirmed_seats).max(0))
}

pub fn availability(
    config: Option<&EventConfig>,
    event_start: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    confirmed_seats: i64,
) -> Availability {
    let Some(config) = config.filter(|c| c.enabled) else {
        return Availability::Disabled;
    };
    if let Some(opens_at) = config.opens_at.filter(|o| now < *o) {
        return Availability::NotOpenYet { opens_at };
    }
    if config.closes_at.or(event_start).is_some_and(|c| now >= c) {
        return Availability::Closed;
    }
    match free_spots(config, confirmed_seats) {
        Some(0) if config.waitlist_enabled => Availability::WaitlistOnly,
        Some(0) => Availability::Full,
        free => Availability::Open { free },
    }
}

/// Decides how a registration for `seats` seats is stored, or why it is refused.
pub fn decide(
    config: Option<&EventConfig>,
    event_start: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    confirmed_seats: i64,
    seats: i32,
) -> Result<RegistrationStatus, Rejection> {
    let Some(config) = config else {
        return Err(Rejection::Disabled);
    };
    match availability(Some(config), event_start, now, confirmed_seats) {
        Availability::Disabled => Err(Rejection::Disabled),
        Availability::NotOpenYet { .. } => Err(Rejection::NotOpenYet),
        Availability::Closed => Err(Rejection::Closed),
        _ if seats < 1 || seats > config.max_seats_per_registration => Err(Rejection::InvalidSeats),
        Availability::Full => Err(Rejection::Full),
        Availability::WaitlistOnly => Ok(RegistrationStatus::Waitlisted),
        Availability::Open { free: None } => Ok(RegistrationStatus::Confirmed),
        Availability::Open { free: Some(free) } if i64::from(seats) <= free => {
            Ok(RegistrationStatus::Confirmed)
        }
        Availability::Open { .. } if config.waitlist_enabled => Ok(RegistrationStatus::Waitlisted),
        Availability::Open { free: Some(free) } => Err(Rejection::NotEnoughSpots { free }),
    }
}

/// The waitlisted registrations (oldest first, as `(id, seats)`) that move up into `free` spots.
/// Strictly first come, first served: it stops at the first one that doesn't fit, so a group
/// isn't overtaken forever by single registrations behind it.
pub fn to_promote(free: Option<i64>, waitlist: &[(Uuid, i32)]) -> Vec<Uuid> {
    let mut free = free.unwrap_or(i64::MAX);
    waitlist
        .iter()
        .take_while(|(_, seats)| {
            let fits = i64::from(*seats) <= free;
            if fits {
                free -= i64::from(*seats);
            }
            fits
        })
        .map(|(id, _)| *id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn config(capacity: Option<i32>, waitlist: bool) -> EventConfig {
        EventConfig {
            event_id: 1,
            enabled: true,
            capacity,
            max_seats_per_registration: 4,
            waitlist_enabled: waitlist,
            opens_at: None,
            closes_at: None,
            notify_email: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn availability_states() {
        let now = Utc::now();
        assert_eq!(availability(None, None, now, 0), Availability::Disabled);
        let mut c = config(Some(10), false);
        assert_eq!(availability(Some(&c), None, now, 3), Availability::Open { free: Some(7) });
        assert_eq!(availability(Some(&c), None, now, 10), Availability::Full);
        // more confirmed than capacity, e.g. after the admin lowered it
        assert_eq!(availability(Some(&c), None, now, 12), Availability::Full);
        c.waitlist_enabled = true;
        assert_eq!(availability(Some(&c), None, now, 10), Availability::WaitlistOnly);
        // closes at the event start unless closes_at is set
        assert_eq!(availability(Some(&c), Some(now), now, 0), Availability::Closed);
        c.closes_at = Some(now + Duration::hours(1));
        assert_eq!(availability(Some(&c), Some(now), now, 0), Availability::Open { free: Some(10) });
        c.opens_at = Some(now + Duration::minutes(1));
        assert!(matches!(availability(Some(&c), None, now, 0), Availability::NotOpenYet { .. }));
        c.enabled = false;
        assert_eq!(availability(Some(&c), None, now, 0), Availability::Disabled);
        assert_eq!(availability(Some(&config(None, false)), None, now, 500), Availability::Open { free: None });
    }

    #[test]
    fn decide_registrations() {
        let now = Utc::now();
        let c = config(Some(5), false);
        assert_eq!(decide(Some(&c), None, now, 3, 2), Ok(RegistrationStatus::Confirmed));
        assert_eq!(decide(Some(&c), None, now, 3, 3), Err(Rejection::NotEnoughSpots { free: 2 }));
        assert_eq!(decide(Some(&c), None, now, 5, 1), Err(Rejection::Full));
        assert_eq!(decide(Some(&c), None, now, 0, 0), Err(Rejection::InvalidSeats));
        assert_eq!(decide(Some(&c), None, now, 0, 5), Err(Rejection::InvalidSeats));
        let w = config(Some(5), true);
        assert_eq!(decide(Some(&w), None, now, 3, 3), Ok(RegistrationStatus::Waitlisted));
        assert_eq!(decide(Some(&w), None, now, 5, 1), Ok(RegistrationStatus::Waitlisted));
        assert_eq!(decide(None, None, now, 0, 1), Err(Rejection::Disabled));
    }

    #[test]
    fn promotes_first_come_first_served() {
        let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        let waitlist = [(ids[0], 2), (ids[1], 3), (ids[2], 1), (ids[3], 1)];
        assert_eq!(to_promote(Some(4), &waitlist), vec![ids[0]]);
        assert_eq!(to_promote(Some(6), &waitlist), vec![ids[0], ids[1], ids[2]]);
        assert_eq!(to_promote(Some(0), &waitlist), Vec::<Uuid>::new());
        assert_eq!(to_promote(None, &waitlist), ids);
    }
}

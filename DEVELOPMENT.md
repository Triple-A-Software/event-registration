# Development

Status: **model**. The data model, the capacity rules and the register/cancel transactions exist and are tested. The routes in `plugin.json` are not served yet.

## How it works

The CMS owns the events; the plugin only owns what registration adds to them. It reads two databases like blog-comments:

- `DATABASE_URL`: its own `event_config`, `registration` and `settings`
- `CMS_DATABASE_URL`: the `event` table, for title, slug, start/end and whether the event is public (`status = 'public' and deleted_at is null`)

The databases are separate, so `event_config.event_id` has no foreign key and event titles in the admin are a second query, not a join. Neleto has no plugin hooks for event changes. A deleted or unpublished event just stops taking registrations; its registrations stay and are shown as "deleted event" in the admin.

## Data model

```
event_config (1 per CMS event, optional)    registration (n per event_config)
  event_id  PK = CMS event.id                 id uuid, event_id FK restrict
  enabled                                     name, email, phone?, seats, comment?
  capacity?  (null = unlimited)               status: confirmed | waitlisted | cancelled
  max_seats_per_registration                  cancel_token (secret for the cancel link)
  waitlist_enabled                            created_at, cancelled_at?
  opens_at?, closes_at? (null = event start)
  notify_email? (null = settings)           settings (singleton)
                                              notify_email?, captcha_provider/sitekey/secret
```

- Free spots = `capacity - sum(seats of confirmed)`. Waitlisted and cancelled registrations take no spots. Nothing is stored as a counter, so it can't drift.
- One active (not cancelled) registration per email address and event (`registration_active_email`). That also stops double submits. A cancelled address can register again.

## Capacity rules (`src/availability.rs`)

Pure functions, unit tested:

- `availability` → `Disabled | NotOpenYet | Closed | Open { free } | WaitlistOnly | Full`, used by both helpers.
- `decide` → `confirmed`, `waitlisted` or a `Rejection` for a new registration. If the request needs more seats than are free, it goes to the waitlist when that's enabled. Otherwise it's refused with `NotEnoughSpots { free }`, so the form can say "only 2 spots left".
- `to_promote`: the waitlist moves up strictly first come, first served, and stops at the first registration that doesn't fit.

## Concurrency (`src/database.rs`)

`register` and `cancel` lock the event's `event_config` row (`for update`) first. Registrations of one event therefore run one after the other, and two visitors can't take the last spot. `cancel` takes the config lock before the registration row, in the same order as `register`, so the two can't deadlock. `tests/database.rs` runs 20 concurrent registrations against 3 spots. Without the lock it overbooks (6 confirmed).

## Planned routes (`plugin.json`)

| Hook | Route | Purpose |
|---|---|---|
| `helpers` | `/helper/event_registration` | `{{{ event_registration event.id }}}`: free spots + form, or the reason it's closed |
| `helpers` | `/helper/event_spots` | `{{{ event_spots event.id }}}`: free spots badge for event lists |
| `pages` | `/event-registration/submit` | form POST (no layout), redirects back with `?registration=<result>` |
| `pages` | `/event-registration/cancel` | `?token=`: confirm page + POST to cancel |
| `api` | `/api/events` | CMS events with config, confirmed seats, waitlist size |
| `api` | `/api/events/:id` | get/put an event's config; raising the capacity promotes the waitlist |
| `api` | `/api/registrations` | list (by event, status, search), update status, delete |
| `api` | `/api/registrations/export` | CSV per event |
| `api` | `/api/settings` | notify address, captcha |
| `dashboard_cards` | `/dashboard/upcoming` | next events with fill level |
| `ui` | `/ui` | admin UI, Vue + Nuxt UI like form-plugin |

Public form: honeypot, captcha and rate limit as in form-plugin/blog-comments. Emails go through the backend's internal `POST /email/send` like blog-comments: confirmation or waitlist mail with the cancel link to the registrant, a mail to the notify address, and a mail when someone moves up from the waitlist. Each is spawned, so mail never fails a registration.

## Open decisions

- **Extra questions per event** (meal choice, company, …): not modeled. Options: a `fields jsonb` on `event_config` and `data jsonb` on `registration`, or reuse form-plugin forms.
- **Double opt-in** (confirm by email before the spot counts): not modeled. It would need a `pending` status that holds a spot for a limited time.
- **Recurring events / several dates per event**: neleto events have one start, so one config per event.
- **Paid events**: out of scope.

## Running locally

```sh
cp .env.example .env
cargo run
cargo test                                                # capacity rules
DATABASE_URL=postgresql://… cargo test -- --ignored      # register/cancel against Postgres
```

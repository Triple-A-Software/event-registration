# Development

Status: **built**. All routes in `plugin.json` are served: the two helpers, the submit and cancel pages, the admin API and UI, and the dashboard card. It has not run inside a real neleto instance yet.

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
  notify_email? (null = settings)             language, base_url? (for later mails)
                                            settings (singleton)
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

`register` and `cancel` lock the event's `event_config` row (`for update`) first. Registrations of one event therefore run one after the other, and two visitors can't take the last spot. Every other change of who holds a spot (`cancel`, the admin's `set_status`, `delete_registration`, `save_config`) takes the same lock first, in the same order, so none of them can deadlock. Each one then fills freed spots from the waitlist, except an admin moving someone to the waitlist by hand: that must not move them straight back up. `tests/database.rs` runs 20 concurrent registrations against 3 spots. Without the lock it overbooks (6 confirmed).

## Routes (`plugin.json`)

| Hook | Route | Purpose |
|---|---|---|
| `helpers` | `/helper/event_registration` | `{{{ event_registration event.id }}}`: free spots + form, or the reason it's closed |
| `helpers` | `/helper/event_spots` | `{{{ event_spots event.id }}}`: free spots badge for event lists |
| `pages` | `/event-registration/submit` | form POST (no layout), redirects back with `?registration=<flag>` |
| `pages` | `/event-registration/cancel` | GET `?token=`: confirm page; POST: cancel |
| `api` | `/api/events` | CMS events with config, confirmed seats, waitlist size |
| `api` | `/api/events/:id` | GET/PUT an event's config; raising the capacity promotes the waitlist |
| `api` | `/api/registrations` | GET (by `event_id`, `status`, `q`), PUT `{id, status}`, DELETE `?id=` |
| `api` | `/api/registrations/export` | CSV per event (`;`, BOM, formula cells defused) |
| `api` | `/api/settings` | notify address, captcha |
| `dashboard_cards` | `/dashboard/upcoming` | next public events with fill level |
| `ui` | `/ui` | admin UI, Vue + Nuxt UI like form-plugin (`ui/`) |

## Public flow (`src/api/public.rs`, `src/render.rs`)

- Both helpers render nothing for a draft or deleted event, an event without config, or on any error, so they never break a page.
- The submit is a plain form POST and redirects back to the event page. The result banner (`?registration=confirmed`, `not_enough_spots&free=2`, …) is shown by an inline script, like blog-comments: neleto's page cache ignores the query string, so a server-rendered banner would be cached.
- The same cache means **the free-spots count on a cached page can be stale** until the page is re-rendered. The submit always checks against the database, so this can't overbook. It only shows "3 spots left" a little too long.
- Spam: honeypot field, optional captcha (hCaptcha, reCAPTCHA, Turnstile; only with provider, site key and secret all set), and 10 submits per 10 minutes per IP. The rate limit is in memory, like form-plugin's.
- The cancel link only shows a confirm page. The cancellation itself is a POST, because mail scanners follow links.

## Mails (`src/email.rs`)

Sent through the backend's internal `POST /email/send` like blog-comments, each one spawned so mail never fails a registration:

- to the registrant: confirmation or waitlist mail with the cancel link, and a mail when they move up (also when the admin confirms them by hand)
- to the event's notify address, else the one from the settings: each new registration and cancellation

Neleto gives a plugin no site URL, and promotions can be triggered from the admin. So the submit stores the site origin (`X-Forwarded-Host`/`Host`) and the page language on the registration. Dates are shown in Europe/Berlin, since the CMS has no timezone setting.

## Open decisions

- **Extra questions per event** (meal choice, company, …): not modeled. Options: a `fields jsonb` on `event_config` and `data jsonb` on `registration`, or reuse form-plugin forms.
- **Double opt-in** (confirm by email before the spot counts): not modeled. It would need a `pending` status that holds a spot for a limited time.
- **Recurring events / several dates per event**: neleto events have one start, so one config per event.
- **Paid events**: out of scope.
- **Privacy consent checkbox** on the form: not built. Most German sites will want one; it needs a configurable text and link per site.

## Running locally

```sh
cp .env.example .env
cargo run
just uidev                                                # admin UI on vite, proxies /api to :3000
cargo test                                                # rules, rendering, csv, spam
DATABASE_URL=postgresql://… cargo test -- --ignored      # register/cancel/admin changes against Postgres
```

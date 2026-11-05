create type registration_status as enum ('confirmed', 'waitlisted', 'cancelled');

-- Registration options of one CMS event. Without a row (or with enabled = false) the event takes
-- no registrations. event_id is the CMS "event".id; it lives in another database, so no FK.
create table event_config (
    event_id integer primary key,
    enabled boolean not null default false,
    -- null = unlimited
    capacity integer check (capacity > 0),
    max_seats_per_registration integer not null default 1 check (max_seats_per_registration > 0),
    waitlist_enabled boolean not null default false,
    -- null = open right away
    opens_at timestamptz,
    -- null = closes when the event starts
    closes_at timestamptz,
    -- null = settings.notify_email
    notify_email text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table registration (
    id uuid primary key default gen_random_uuid(),
    -- restrict: removing an event's config must not silently delete its registrations
    event_id integer not null references event_config (event_id) on delete restrict,
    name text not null,
    email text not null,
    phone text,
    seats integer not null default 1 check (seats > 0),
    comment text,
    status registration_status not null,
    -- secret for the self-service cancel link in the confirmation email
    cancel_token uuid not null unique default gen_random_uuid(),
    created_at timestamptz not null default now(),
    cancelled_at timestamptz
);

create index registration_event_status on registration (event_id, status, created_at);
-- one active registration per address and event; also stops double submits
create unique index registration_active_email on registration (event_id, lower(email))
    where status <> 'cancelled';

create table settings (
    id text primary key default 'settings' check (id = 'settings'),
    -- receives a mail for every new registration and cancellation
    notify_email text,
    captcha_provider text check (captcha_provider in ('h-captcha', 're-captcha', 'turnstile')),
    captcha_sitekey text,
    captcha_secret text
);
insert into settings default values;

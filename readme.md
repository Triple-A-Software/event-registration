# Event registration

A plugin for [neleto](https://neleto.io/) that adds registrations to neleto's events: visitors sign up for an event on the website, and the page shows how many spots are still free.

## Features

- Registration can be switched on per event, with a capacity (or unlimited), a maximum number of seats per registration and an optional registration window. By default registration closes when the event starts.
- Free spots are shown on the event page and in event lists, e.g. "Noch 12 Plätze frei", "Ausgebucht" or "Warteliste".
- With the waitlist switched on, a fully booked event keeps taking registrations as waitlisted. When someone cancels, the oldest waitlisted registrations move up.
- The confirmation email contains a link to cancel without logging in.
- In the admin you see the registrations of each event, can cancel or delete them and export them as CSV.

## Usage in templates

Switch registration on for an event in the plugin's admin first; without that, both helpers render nothing.

On the event detail page (the page with the `/events/:slug` route), inside the component that shows the event:

```hbs
{{#if relations.global.event as event}}
  …
  {{ event_registration(event.id) }}
{{/if}}
```

In an event list, the free spots badge for each event:

```hbs
{{ event_spots(event.id) }}
```

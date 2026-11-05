-- Mails sent later (waitlist promotion, possibly triggered from the admin) need the registrant's
-- language and the site the cancel link points to. Neither is known outside the submit request.
alter table registration add column language text not null default 'de';
alter table registration add column base_url text;

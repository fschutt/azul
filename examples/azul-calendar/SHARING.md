# Sharing calendars (design, not built)

How AzCalendar shares a calendar through the user's own S3 storage, on the Azlin plan: durable
data is files in S3, end-to-end encrypted (Azlin stores no keys); the database only mints ids,
keeps transient state and the access links (who may read or write which path).

Today "Share Calendar" says "comes later"; FILE > Open & Export (.ics) and FILE > Print are the
stopgaps.

## What the user sees

- HOME > Share Calendar opens a sheet for the calendar selected in "My calendars":
  - **People**: an Azlin address (or a contact), and **Can view** or **Can edit**. A list of
    the people it is shared with, each with Remove.
  - **Subscription link (.ics)** for people outside Azlin (Google Calendar, Outlook, Apple):
    off by default; when on, **Availability only** (default) or **Full details**, a Copy link
    button and Stop publishing. It says plainly: "This link is not encrypted. Anyone who has it
    can read it."
- Calendars shared with you appear in "My calendars" under **Shared with me**, with the owner's
  name. A view-only calendar's events open read-only (no Save, no drag on the grid).
- Removing someone says: "They keep what they already downloaded."

## Where the data is

A shared calendar gets a folder of its own in the owner's bucket. Its objects are encrypted with
the calendar's own key, never with the drive key, so a recipient can read this calendar and
nothing else of the drive.

```
shares/calendars/<calendar id>/
  members.json          owner-signed: member public keys, roles, the key epoch
  keys/<member key id>  the calendar key of each epoch, wrapped for that member's public key
  snapshot-<log key>    all events up to that log entry (encrypted, written on compaction)
  log/<ms>-<device>-<n> one change each, append-only (encrypted and signed)
```

- The calendar key (CK) is 256 random bits, made by the owner's client when the calendar is
  first shared. It is wrapped (X25519 + an AEAD, as the drive key is for team members) once per
  member, the owner included. Objects are sealed with the CK of the current epoch.
- The owner's own files (`events/<uuid>.json`, `calendars/<id>.json`, under the drive key) stay
  as they are: they are the owner's working copy. The first share writes a snapshot of the
  calendar's events into the folder; from then on every change of that calendar is ALSO written
  as a log entry (the same `WriteQueue` and file thread as every other write).
- A recipient keeps a local copy under `<data dir>/shared/<owner>/<calendar id>/` (the snapshot
  plus the log applied), so a shared calendar works offline like an own one.

## Who may do what

Two layers, so neither alone has to be trusted:

1. **Storage grants** (the database's access links, enforced the way
   `azul_storage::ScopedDrive` does: a prefix and `can_write`):
   - Can view: read `shares/calendars/<id>/` (GET, LIST).
   - Can edit: that, plus write under `shares/calendars/<id>/log/` only.
   - Owner: everything, including `members.json`, `keys/` and deleting compacted log entries.
   The Worker hands a member short-lived credentials for exactly that prefix; the member never
   sees the owner's S3 keys.
2. **Signatures**: every log entry is signed with the author's device key; `members.json` is
   signed by the owner. A client applies an entry only when its author has the edit role in
   the members list of that epoch. A view-only member who got write credentials anyway still
   cannot change anyone's calendar.

**Removing a member**: the owner's client deletes the access link, starts a new epoch (a new CK,
wrapped for the remaining members), and seals new entries and the next snapshot with it.

## How changes sync

- A change is one object: `log/<20-digit ms>-<device id>-<counter>`, written with
  `If-None-Match: *` (never overwrites). Decrypted:
  `{ "format": "azcalendar.change", "version": 1, "op": "put" | "delete", "event": {...} |
  "id": "<uuid>", "hlc": "<hybrid logical clock>", "author": "<device key id>" }` - `event` is
  the event file's JSON as `event.rs` writes it, whole (no field diffs).
- Clients poll: LIST `log/` with `StartAfter` = the last key seen minus 10 minutes (keys from a
  device with a skewed clock still come in; entries already applied are skipped by key), every
  `AZCAL_SYNC_SECONDS` while the app runs and at start. Later the database may keep a
  "calendar X changed at T" ping (no content) so clients poll only when needed.
- Merge per event: the entry with the highest `(hlc, author)` wins (last writer wins, as CalDAV
  servers effectively do). A delete is a tombstone with an hlc like any change. When the event
  open in the editor changed underneath, the editor says "Changed by <name> meanwhile" and offers
  Reload or Keep mine (Keep mine writes a newer entry).
- Repeating events stay one event with its rule and `except` list; editing one occurrence is the
  same two changes the editor makes today (a new one-off event, the series' `except`).
- **Compaction**: when `log/` holds more than ~500 entries, the owner's client (or any editor,
  when the owner has not for 30 days) writes `snapshot-<last log key>`. The owner deletes the
  entries a snapshot covers 7 days later, so a member who was offline catches up from the
  snapshot and the newer entries.

## The .ics subscription link

Outside apps cannot decrypt, so this is a plaintext copy - the "publish" case of the privacy
plan: opt-in per calendar, labelled, separate storage.

- The owner's client renders the calendar with `ics::write` (Full details), or with every
  summary "Busy" and no location, notes or attendees (Availability only), from 30 days back to a
  year ahead (repeating events keep their RRULE), and PUTs it to a public bucket as
  `ics/<128-bit random token>.ics`, served as `https://<ics host>/<token>.ics` (and `webcal://`).
- It is rendered again a few seconds after each change of the calendar (debounced, on the file
  thread), so subscribers see changes on their next refresh.
- Stop publishing deletes the object; publishing again makes a new token (old links die).

## Order of work

1. Calendar key, `members.json`, wrapped keys; "Can view" only; the recipient reads the snapshot.
2. The change log, polling and merging; "Can edit".
3. Removing members (epochs), compaction.
4. The .ics subscription link.
5. The database's change ping.

## Open questions

- How a recipient's public key is found from an Azlin address (a key directory in the database,
  or the contact card AzContacts keeps).
- Free/busy across shared calendars for meeting planning (the Schedule View) - a later feature
  on top of the same log.
- Whether an attendee invitation (iTIP REQUEST / REPLY by mail through AzMail) goes through this
  log or stays mail; today attendees are only addresses on the event.

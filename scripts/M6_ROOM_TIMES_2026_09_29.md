# M6 room times: report (2026-09-30)

Task: meeting rooms carry their start / end time, and AzCalendar mints with the event's time, so a link
minted today for next week's event still points at a live room next week.
Branch `wt/m6-room-times` (base `34a8fe46f`, the tip of `fix/input-bugs-2026-09-19`). The Worker work is in
`/Users/fschutt/Development/azul-apps-m1`, branch `cf-workers-meet`. Nothing was compiled with cargo or rustc
(house rule); see "How sure".

## What was built

### 1. The meet Worker (`azul-apps-m1/cf-workers/meet`)
- `POST /rooms` takes optional `starts_at` / `ends_at`, RFC 3339. An offset such as `+02:00` is accepted;
  answers are always UTC (`2026-10-06T09:00:00.000Z`). The parser is `parseRfc3339` in `src/ids.js`, with the
  other input rules. It refuses days and times that do not exist (30 February, 24:00, second 60).
- Lifetime: a room with times lives until `ROOM_GRACE_SECONDS` (default 7200 s, 2 h) after `ends_at`, however far
  ahead it was minted. A room without times keeps the old `ROOM_TTL_SECONDS` (one day) past its last use.
  Announcing never shortens a room. In a room with times, an announcement keeps the room alive for a grace
  period past it, so a meeting that runs over is not cut off. It does not add a whole day.
- Validation, `400 invalid_window` with a message:
  - one time without the other;
  - a value that is not RFC 3339 (a number, a date alone, no seconds, no zone, a day that does not exist);
  - `ends_at <= starts_at`;
  - longer than `MAX_WINDOW_SECONDS` (7 days; exactly 7 days is accepted);
  - `starts_at` more than `MAX_LEAD_SECONDS` (366 days) ahead;
  - `ends_at` already past.

  A meeting that is under way (start past, end ahead) is accepted. `{starts_at: null, ends_at: null}` counts as
  no times.
- The room record has two new columns, `room.starts_at` and `room.ends_at` (milliseconds, NULL when there are
  none). `SCHEMA` creates new tables with them. An older table, from Turso or a persisted `.meet-dev.sqlite`, gets
  them on first use through `ADDED_COLUMNS` and `migrate()` in `src/store.js`: one
  `SELECT name FROM pragma_table_info('room')`, then `ALTER TABLE ... ADD COLUMN` for each missing column. If two
  isolates race, the loser checks the columns again and does not fail. The same SQL runs on both adapters (libSQL
  over HTTP, and node:sqlite).
- Answers: `POST /rooms`, `GET /rooms/<id or code>` as JSON, and `POST /rooms/<id>/peers` all return
  `starts_at` / `ends_at`, with `null` for a room that has no times. AzMeet can use this to show "starts in ...".
- The landing page reads "Starts Tuesday 6 October 2026, 09:00 UTC, ends 10:00 UTC. You can join before it starts."
  Both times are in `<time datetime>` tags, and the end shows its full date when it falls on another day. The page
  has no script, so it shows UTC rather than the reader's time zone.
- Joining and announcing before `starts_at` are allowed. The sweep deletes rooms by `expires_at`, and for a room
  with times that value comes from the window: `ends_at` plus the grace period, or later if someone announced.
- README (API table, a "Meeting times" section, settings, curl example) and `wrangler.toml` (the three new vars).
- Tests: 13 new (handler 9, stores 2 x 2 adapters). `roundTrip` in `entrypoints.test.mjs` now mints with times and
  reads them back, both through the Worker entry (fake Hrana) and through the dev server over HTTP.
  `node --test`: **63 / 63 pass**.

### 2. AzCalendar (`examples/azul-calendar`)
- `meeting::utc_window(zone, date, start, end) -> (DateTime<Utc>, DateTime<Utc>)` reads the event's day and times
  in `zone` and converts them to UTC. The app passes `chrono::Local`. If a clock time happens twice, the first one
  is used. If a time is skipped because DST starts, it is read an hour later, which is the same instant.
- `meeting::mint_body(starts, ends)` builds `{"starts_at":"2026-10-06T07:00:00Z","ends_at":"2026-10-06T08:00:00Z"}`.
  `on_save` computes it from the checked event and passes it to the mint thread in `MintJob.body`, instead of `{}`.
- `Meeting` has two new fields, `starts_at` and `ends_at`: RFC 3339 UTC strings stored as the server sent them, and
  empty when the server kept the room without times. `minted_meeting` reads them from the answer. When an answer
  has no times (an older server), stderr says so and gives the room's `expires`.
- Event file **version 2**: `meeting.starts_at` / `meeting.ends_at`, both left out when empty. `OLDEST_VERSION = 1`,
  so version 1 files still read (with no times) and are written as version 2 when saved again. A version above 2 is
  still refused. The format is documented at the top of `event.rs`. The struct was renamed from `FileV1` to
  `EventFile`.
- A link kept from a failed save is minted again once the form's day or a time changes (`on_day`, `set_time`),
  because its room is held for the old times.
- Event ids: `new_event_id()` is now `azul::uuid::Uuid::from_seed(random_seed())`. `random_seed()` produces one
  `u64` from std's `RandomState`, hashed with a counter, the time and the process id; this is the same source of
  randomness as before. The hand-written hex, version and variant code is deleted. The RED test (256 ids, all
  distinct, v4-shaped, not the marker sequence) is unchanged except that it now also checks the version nibble and
  the variant.
- Unit tests, all RED first:
  - `event.rs`: version 2 on disk, including the meeting times; a version 1 file still reads; the times round-trip
    and are left out when unknown; a newer version is refused (now version-agnostic).
  - `meeting.rs`: UTC conversion at +02:00, at -05:00 across midnight and the new year, and at UTC; the mint body;
    the answer's times, and missing or null times.
  - Counts now: event.rs 19, meeting.rs 10, week.rs 13.
- `scripts/mint-and-join.mjs`:
  - It clicks "Next week" first, so the event is next Monday 09:00 - 10:00. With the default day (today), a run
    after 10:00 would ask for a meeting that is already over, and the Worker rightly refuses that.
  - The file must be version 2.
  - `meeting.starts_at` / `ends_at` must equal the event's day and times in this machine's zone, in UTC. JavaScript
    `new Date("YYYY-MM-DDTHH:MM:00")` is local time, the same as `chrono::Local`, and the start must be in the
    future.
  - The dev server's `GET /rooms/<id>?format=json` must hold the same times, and `expires` must be `ends_at` + 2 h.
- Dry run of the script against Node stand-ins for both apps and the real dev server: **PASS**. The stand-ins mint
  exactly as the new `lib.rs` does. A negative run, with a stand-in that sends no times, **FAILS** at
  "meeting.starts_at is not ... in UTC". Stand-ins: `scratchpad/m6/fake-azcal.mjs`, `fake-azcal-v.mjs`
  (`FAKE_NO_TIMES=1`) and `fake-azmeet.mjs`, in this session's scratchpad. The Rust apps were not run.

## Commits

azul-apps (`/Users/fschutt/Development/azul-apps-m1`, branch `cf-workers-meet`):
- `adfaf66` test(meet): a room minted for a meeting keeps its start and end time (RED)
- `45fae88` feat(meet): rooms keep their meeting's start and end time

azul (`wt/m6-room-times`):
- `553df4102` test(azcalendar): a minted link carries the event's times, in UTC, into the event file (RED)
- `9954a4276` feat(azcalendar): minting sends the event's times in UTC, and the event file keeps them
- `ddbfc42ac` refactor(azcalendar): event ids are azul's Uuid::from_seed of a random seed
- `docs(m6): progress` commits and this report

## api.json

**No change from this task.** It uses one new function that another agent is adding:
`azul::uuid::Uuid::from_seed(seed: u64) -> String` (the api.json `uuid.Uuid.functions.from_seed`, fn_args
`seed: u64`, returns `String`, fn_body `azul_layout::uuid::Uuid::from_seed(seed)`). **`ddbfc42ac` does not compile
until that lands.** The other commits do not depend on it.

## How sure (nothing compiled)
- `rustfmt --check` parses and formats `event.rs`, `meeting.rs` and `lib.rs` cleanly. The chrono 0.4.45 APIs used
  were checked in the registry source: `NaiveDateTime::and_utc`, `TimeDelta::hours`, and `chrono::TimeDelta`
  exported at the crate root. `to_rfc3339_opts` needs `alloc`, which `std` gives.

Least sure, in order:
1. `Uuid::from_seed`: its name and signature, and that the generated Rust wrapper returns `azul::str::String`
   (`.as_str()`), the same as `Uuid::v4`.
2. `meeting::utc_window`'s closure over the generic `zone: &Tz`:
   `zone.from_local_datetime(&local).earliest().or_else(..).map(|t| t.with_timezone(&Utc))`.
3. The `meeting.rs` tests take `DateTime`, `Utc`, `NaiveDate`, `NaiveTime` and the `TimeZone` trait (for
   `Utc.with_ymd_and_hms`) through `use super::*`; `FixedOffset` is imported explicitly.
4. `event.rs` test: `assert_eq!(json["version"], VERSION)` relies on `serde_json::Value: PartialEq<u64>`.

## Test commands for the parent

```sh
# the Worker (63 tests, no install)
cd /Users/fschutt/Development/azul-apps-m1/cf-workers/meet && node --test "test/*.test.mjs"

# AzCalendar unit tests (event 19, meeting 10, week 13, AzMeet's rooms.rs); needs Uuid::from_seed merged
cargo test -p AzCalendar --lib

# E2E, after building AzCalendar + AzMeet + a libazul with the debug server (as in M4)
node examples/azul-calendar/scripts/mint-and-join.mjs \
  --worker-dir /Users/fschutt/Development/azul-apps-m1/cf-workers/meet [--bin ...] [--meet-bin ...] [--keep-logs]
```

## What is left / notes
1. **AzMeet** (not touched; another agent owns it) can now show "starts in ...". `GET /rooms/<id>?format=json`
   (which AzMeet already calls when opening a room) and every announcement answer carry `starts_at` / `ends_at`.
2. **Turso** gets its migration from `pragma_table_info('room')` and `ALTER TABLE ADD COLUMN`. libSQL / sqld
   support both. They are tested against node:sqlite and the fake Hrana server, not against a live Turso.
3. **Entropy**: the Worker does not re-check that the times it answered are the ones AzCalendar asked for. The
   event file keeps what the server answered. `Uuid::from_seed` limits event ids to 64 bits of randomness (the
   seed), where the old code used 122. That was the requested design, and 64 bits is plenty for per-user event
   files.
4. **Past events**: minting a link for an event that is already over is refused by the Worker. The form shows
   "The meeting server answered 400: the meeting is already over".
5. **Recurring events** and time zones in the event itself are still open (`planning/core/calendar.md`).
6. **M4 report**: `scripts/M4_AZCALENDAR_2026_09_29.md` still describes format version 1. This report supersedes
   that part.

# M4 AzCalendar: report (2026-09-29)

Ledger item "new AzCalendar test app: create event -> mint AzMeet link -> share -> second AzMeet client joins".
Branch `wt/m4-azcalendar` (base `8e9a0a683`, the tip of `fix/input-bugs-2026-09-19`). Nothing compiled with cargo
(house rule); see "How sure" for what was checked instead.

## What was built

`examples/azul-calendar/` - package `AzCalendar`, lib `azcalendar` (rlib + cdylib) + bin `AzCalendar`, azul with
`link-dynamic`, `.cargo/config.toml` like AzMeet's (target `target/consumer`), a workspace member next to azul-meet.
Dependencies: serde (derive), serde_json, chrono (`std` + `clock`, the features azul-layout builds it with). All
three are already in Cargo.lock at one version each; the lock got the `AzCalendar` entry by hand.

### Storage (user ruling: durable data is files, the DB only mints)
- One event = one file, `<data dir>/events/<uuid>.json`. The relative path is the object key (`events/<id>.json`,
  `event::object_key`), so the folder can move to S3 / R2 unchanged.
- Data dir: `AZCAL_DATA`, else `<FilePath::get_data_dir()>/AzCalendar`, else `./AzCalendar`.
- Format, version 1 (documented at the top of `event.rs`):
  `{"format": "azcalendar.event", "version": 1, "id", "title", "date": "YYYY-MM-DD", "start": "HH:MM",
  "end": "HH:MM", "meeting": {"link": "azlin://meet/<room id>", "server", "code", "expires"}}` (`meeting` only when
  there is one; `code` / `expires` only when the server sent them). A newer `version` is refused, never guessed at;
  unknown fields are ignored. Wall-clock times on one day, no time zones.
- The id is `Uuid::v4()` (azul's), fixed when the form opens; only lower-case UUID names are ever read or written,
  so nothing else in the folder (temp files, notes, `../`) is touched. Saves are atomic (dot-named temp + rename).
  `load_all` reads every `<uuid>.json` in order, and names on stderr the ones it cannot read or whose id is not
  their file name.
- The meeting link is minted by the meet Worker (`POST /rooms`, the DB side) and stored IN the event file, with
  the server that minted it (a link only means something to its own server).

### The app (`lib.rs`)
- Week view: Monday - Sunday, 08:00 - 20:00 (64 px/hour), today's column tinted; each event a block (title,
  "09:00 - 10:00", and "Join meeting" when it has a link); overlapping events side by side in lanes;
  "+N before 08:00 / after 20:00" in the day header. Toolbar: Previous week / This week / Next week, the week's
  title ("28 September - 4 October 2026"), "New event". Footer: the events folder and the meeting server.
- "New event": a side sheet - Title (`TextInput`, root id `#event-title`), Day (`DatePicker`; turning the month
  rebuilds it, per the widget's documented limit), Starts / Ends (`TimePicker`, 24 h), "Add AzMeet link"
  (`CheckBox`; its label toggles it too; ticked, the sheet says "A new AzMeet link is made when you save."),
  Cancel / "Save event".
- Save: checks the event first (title, end after start). No link: writes the file at once. With a link:
  `POST {AZMEET_WORKER}/rooms` on an azul `Thread` (like AzMeet: `http_request` blocks, so it runs off the UI
  thread and resumes into `on_minted`); the answer carries the form's serial, so an answer for a cancelled form is
  dropped; the link is accepted only if it names the minted room by id (AzMeet's `parse_room_link`); a link minted
  for an event that then could not be written is kept for the next Save, not minted again. Errors show in the
  sheet ("The meeting server at ... is unreachable: ...", 429 -> "Too many new meetings ...", "answered 500: <the
  server's message>"). stdout: `AZCAL_SAVED <file>`, `AZCAL_LINK <link>`.
- Meeting server: `AZMEET_WORKER`, else AzMeet's built-in default (`AZMEET_DEFAULT_WORKER` at build time), the same
  setting as AzMeet; with none, the sheet says how to set it and offers no checkbox.
- Join meeting: starts `AZMEET_BIN`, else `AzMeet` next to the AzCalendar binary, with `AZMEET_JOIN=<link>` and
  `AZMEET_WORKER=<the event's server>`, without this app's `AZ_DEBUG`; prints `AZCAL_JOIN_PID <pid>`; finished
  AzMeet processes are reaped on the next Join. Not found / not startable: copies the link to the clipboard and
  says so in the notice line.

### Pure logic, tests first (36 new tests + AzMeet's 15 rooms tests, compiled in again)
- `event.rs` (16): round trip with and without a meeting; the file names its format and version; a newer version
  refused; unknown fields ignored; not JSON / not an event / bad version / missing field; impossible dates and
  times; title and end-after-start; times kept to the minute; the link must name an AzMeet room by id (a code or a
  web page is refused); ids are lower-case UUIDs (no path traversal); names `events/<id>.json`; the data folder
  setting; atomic save leaves no temp file; saving again replaces; load order, other files ignored, broken and
  misnamed files reported; a missing folder is an empty calendar.
- `week.rs` (13): Monday week starts across months and years; stepping weeks; the week's days; the events of a
  week under their days in order; non-overlapping events take the whole column; three overlapping events in three
  lanes; a chain of overlaps reuses lanes (2 lanes, not 3); touching events do not overlap; clipping to 08-20 and
  counting events wholly outside; an event wholly before 08:00 does not narrow the visible ones; labels; the form's
  default day; the date picker's day clamped to its month.
- `meeting.rs` (7): the server setting (AZMEET_WORKER, built-in, trailing slash, blank); a minted room becomes the
  event's meeting; no `link` in the answer -> made from the room id; links that do not name the minted room are
  refused; failure messages; the AzMeet program next to AzCalendar or AZMEET_BIN; the join environment.
- Link formats are NOT re-implemented: `lib.rs` compiles AzMeet's own `examples/azul-meet/src/rooms.rs` as
  `azcalendar::meet_rooms` (`#[path]`). AzMeet itself is untouched (no `pub mod`, no crate dependency - AzMeet's
  lib has an Android `ctor` that would start AzMeet inside AzCalendar).

### E2E: `examples/azul-calendar/scripts/mint-and-join.mjs`
Modeled on AzMeet's `two-clients.mjs`: dev server (in memory, port 8797) -> AzCalendar headless (`AZ_BACKEND=headless
AZ_DEBUG=8767`, temp `AZCAL_DATA`, `AZMEET_WORKER`, `AZMEET_BIN`, `AZMEET_NAME=Cal`, `AZMEET_RELAY=off`) -> debug ops
`click "New event"`, `focus_node #event-title`, `text_input "Team sync"`, `click "Add AzMeet link"`, `click "Save
event"` -> asserts ONE `events/<uuid>.json`, format / version 1 / id = name / title / 09:00 - 10:00 / the printed link
/ the server, `GET /rooms/<id>?format=json` knows the room, the week shows the title and "Join meeting" -> AzMeet
"Ben" headless with `AZMEET_JOIN=<link>`: the dev server lists Ben in the room -> `click "Join meeting"` in
AzCalendar: `AZCAL_JOIN_PID`, and the dev server lists Cal in the room. While waiting for the save it also reads the
window and stops with the form's error text if one shows. Logs + data dir kept on failure (`--keep-logs` always).
Dry run against Node stand-ins for both apps (scratchpad `m4/fake-azcal.mjs`, `m4/fake-azmeet.mjs`, the real dev
server): PASS. The Rust apps were not run.

## Commits

- `0cc765f17` test(azcalendar): event files, week math and minted links, before they exist (RED)
- `960137a1d` feat(azcalendar): event files, week math and minted links
- `8d8292e42` test(azcalendar): a headless AzCalendar mints a link that AzMeet joins with (RED)
- `4bc9ec077` feat(azcalendar): week view, new events with an AzMeet link, and Join meeting
- `6756d8a45` test(azcalendar): mint-and-join says why nothing was saved, and names what it clicks
- plus `docs(m4): progress` commits and this report

## api.json

**No change.** Existing API used: `Uuid::v4`, `FilePath::get_data_dir` (+ `.inner`), `HttpRequestConfig::create /
with_timeout / with_user_agent / with_header / http_request`, `HttpMethod::Post`, `HttpGetResult::downcast`,
`HttpResponse::body_as_string`, `HttpError` variants, `Thread::create`, `ThreadId::unique`,
`CallbackInfo::add_thread / set_clipboard_content`, `TextInput`, `DatePicker`, `TimePicker::with_24h`,
`CheckBox::with_on_toggle`, `Button`, `Dom::with_id / with_callback`, `EventFilter::Hover(HoverEventFilter::Click)`.

## Files outside `examples/azul-calendar/`
- `Cargo.toml`: one workspace-member line after azul-meet.
- `Cargo.lock`: the `AzCalendar` package entry (azul-dll, chrono, serde, serde_json - all already locked).
- `examples/azul-meet/`: untouched (its `rooms.rs` is compiled by path).
- CI (`.github/workflows/rust.yml`) not touched: AzMeet's lib tests are not a CI step either; a
  `cargo test -p AzCalendar --lib` step like AzPaint's / AzMaps' / AzWriter's / AzReview's is a small follow-up.

## NO DUPLICATION: twins with AzMeet (reported, not merged)
AzMeet keeps these private in `lib.rs`, and the M3 agent is working in AzMeet now, so AzCalendar has its own:
1. HTTP on a Thread: AzCalendar `spawn_mint` / `mint_thread` / `mint_outcome` / `http_error_text` vs AzMeet
   `spawn_http` / `http_thread` / `http_answer` / `http_error_text`. The cause is in the library:
   `HttpRequestConfig::http_request` runs ureq on the calling thread on desktop. If it ran the transport on a worker
   thread itself (the request queue is process-wide and already pumped by the shells), both apps would drop their
   Thread wrappers. Recommended library follow-up.
2. The meeting-server setting: `meeting::worker` + `BUILT_IN_WORKER` vs AzMeet's `meeting_server()` (env part) +
   `PRODUCTION_WORKER` (the same `option_env!("AZMEET_DEFAULT_WORKER")`).
3. The `POST /rooms` answer: `meeting::minted_meeting` (serde) vs AzMeet's `room_info` (azul `Json`), and the failure
   text `meeting::mint_failure` vs AzMeet's `server_trouble`.
Suggested: once M3 is merged, move 2 and 3 into a pure `examples/azul-meet/src/server.rs` next to `rooms.rs` that both
apps compile (AzCalendar already does this for `rooms.rs`).

## How sure (nothing was cargo-built)
- `event.rs`, `week.rs`, `meeting.rs` + AzMeet's `rooms.rs`: type-checked alone, lib and `--test`, with
  `rustc +1.91.0 --emit=metadata` against serde / serde_json / chrono rmeta from `target/release/deps`
  (scratchpad `m4/check_pure.sh`): no errors, no warnings. Not run.
- The whole lib (`lib.rs` with the UI, lib and `--test`): type-checked the same way against the link-dynamic azul
  rmeta in `target/release/deps` (`libazul-2409b233fc20e612.rmeta`, built 01:31 today; cross-checked against
  `libazul-7ada030aea27c33c.rmeta`) (scratchpad `m4/check_lib.sh`): no errors, no warnings. A deliberately wrong
  call was rejected, so the check is real.
- Least sure, then: drift between that rmeta's generated API and the base commit's (the calls listed under
  api.json), and the hand-written Cargo.lock entry (cargo rewrites it if it disagrees).

Runtime assumptions to watch in the E2E:
1. The `POST /rooms` answer reaches `on_minted` through the thread poll and the request pump - AzCalendar runs no
   timer of its own (AzMeet has its 15 ms pump timer running anyway). The script's window reads while it waits
   also wake the loop.
2. `click` by text on the "Add AzMeet link" label (a span with a `Click` callback, a flex item in its row) and on
   "Join meeting" (a Button inside an absolutely positioned block).
3. `focus_node #event-title`: the id is added to the TextInput's root, which is its contenteditable container.
4. The spawned AzMeet inherits `AZ_BACKEND=headless`, `AZMEET_NAME=Cal` and `AZMEET_RELAY=off` from AzCalendar's
   environment; `AZ_DEBUG` is removed.

## Test commands for the parent

```sh
# unit tests: event.rs (16), week.rs (13), meeting.rs (7), and AzMeet's rooms.rs (15) compiled in again
cargo test -p AzCalendar --lib

# build AzCalendar and AzMeet (same profile, so AzMeet sits next to AzCalendar) and a libazul with the debug
# server (AZ_DEBUG / e2e-server), as for AzMeet's two-clients.mjs, then
node examples/azul-calendar/scripts/mint-and-join.mjs \
  --worker-dir /Users/fschutt/Development/azul-apps-m1/cf-workers/meet \
  [--bin <path to AzCalendar>] [--meet-bin <path to AzMeet>] [--timeout 90] [--keep-logs]
```

By hand: `node .../cf-workers/meet/dev-server.mjs`, then `AZMEET_WORKER=http://127.0.0.1:8787 AzCalendar` -> New
event -> title -> Add AzMeet link -> Save event -> the block shows "Join meeting" -> click it: AzMeet opens in the
meeting; `AZMEET_WORKER=http://127.0.0.1:8787 AzMeet` elsewhere, paste the link (`events/<id>.json`, "link") -> Join.

## What is left
1. **Room lifetime vs calendar time**: the Worker forgets a room 24 h after its last announcement; a link minted for
   an event next week points at a room that is gone by then (AzMeet would say "This meeting has ended, or the link
   is wrong."). Needs a Worker change (mint a room for a start time, or long-lived rooms for calendar links that
   only become joinable near the start) - azul-apps, not this task. The event file already keeps `expires`.
2. Share: the link is shared by "Join meeting"'s clipboard fallback and by the file; a "Copy link" on the block and
   invitations (AzMail, iMIP) are not there.
3. Editing / deleting / moving events, all-day and overnight events, recurrence, time zones, month/day/agenda
   views, the mini-month, calendars - the rest of `planning/core/calendar.md`.
4. `DatePicker` is Sunday-first while the week view is Monday-first (the widget's documented limit).
5. The twins above; CI lib-test step.

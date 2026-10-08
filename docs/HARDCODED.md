# Hard-coded values (azul: the Azlin apps and the engine pieces they use)

An audit (HARDCODE15, 2026-10-08) of every value fixed in code that matters when the apps run
against the **fully local stack** - sqld as Turso on `http://127.0.0.1:8080`, the token server on
`http://127.0.0.1:8081`, the S3 balancer on `http://127.0.0.1:9000`, the meet Worker on
`http://127.0.0.1:8790`, an iroh relay on `http://127.0.0.1:3340` - or must keep out of the user's
real folders during an end-to-end run. The server side (azul-apps `iso/`) has its own
`iso/docs/HARDCODED.md`; the local stack and its profile are azul-apps
`iso/docs/GETTING-STARTED.md`, "The fully local stack" (`local/mkprofile.py`, `local/azlin-local.env`).

Test-only values (`#[cfg(test)]`, sample content shown to the user, `example.org` addresses) are
left out.

## Where an app's value comes from

The order, most specific first: **the app's switch** (`--worker`, `--tiles`, `--data-dir`, `--home`,
...) > **the environment variable** (`AZMEET_WORKER`, `AZMAPS_TILES`, `AZLIN_DATA`, ...) > **the
shared Azlin config** (`~/.azlin/config.json`, or the file `AZLIN_CONFIG` names; `AZLIN_CONFIG=off`
reads none; a `--shot` run never reads it) and its `endpoints` section > **the profile's built-in
addresses** (`endpoints.profile`: `local` / `trial` / `production`) > **the app's built-in default**.
AzMeet and AzCalendar keep the meeting server chosen in the app (their settings file) above
`AZMEET_WORKER`, as AzMeet always has.

```json
{
  "currentTheme": "flora",
  "mode": "dark",
  "endpoints": {
    "profile": "local",
    "token": "http://127.0.0.1:8081",
    "s3": "http://127.0.0.1:9000",
    "meet": "http://127.0.0.1:8790",
    "relay": "http://127.0.0.1:3340",
    "tiles": "http://127.0.0.1:8099/{z}/{x}/{y}.pbf"
  }
}
```

Readers: `azul_appkit::shared_endpoint` (`of`, `read`, `in_file`: `endpoints.<name>` through
`AzlinConfig`'s own reader; AzCalendar's `meet`, AzMaps' `tiles`) - a stand-in until
`azul_appkit::azlin_config` carries the typed section with profiles and provenance
(`EndpointsSection`, `resolve_endpoints`; AZCLOUD15), which keeps unknown keys such as `tiles`.

The data root of every kit app: `--data-dir` > `AZLIN_DATA` > `<user data dir>/Azlin` (one folder
per app under it). The apps that keep a folder of their own are listed at the end.

Status: **FIXED** `<commit>` on this branch - **OPEN** not fixed, the key it should come from is
given - **DESIGN** fixed on purpose - **USER** needs a decision - **AZMEET15** / **AZMAIL15** owned
by those agents (listed, not edited here) - **AZCLOUD15** being done in the endpoints work.

## 1. The shared config and the data root (examples/azul-appkit)

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| azul-appkit/src/azlin_config.rs:44-66; ui.rs:228-240, 555-566 | `~/.azlin/config.json` | shared config path, apart from `--data-dir` / `AZLIN_DATA`; a settings save writes the look back, creating the file | med: a run that isolates only its data root still reads (and writes) the user's real file | `AZLIN_CONFIG=<file>` / `off`; the local profile names its own copy | DESIGN; azul-apps `local/mkprofile.py` |
| azul-appkit/src/azlin_config.rs (no `endpoints` before) | - | the endpoints section | high: no app could be pointed at the stack through the shared config | `endpoints.{profile,token,s3,meet,relay}` | AZCLOUD15; stand-in reader `shared_endpoint` FIXED 372f1f238, 870c3532b |
| azul-appkit/src/data.rs:17-37 | `<user data dir>/Azlin` (`./Azlin` without one) | data root of every kit app | low (AzCalendar, AzTasks, AzMail, AzDrive keep other folders: section 9) | `--data-dir`, `AZLIN_DATA` | DESIGN |
| azul-appkit/src/ui.rs:205 | `LocalDrive::new(&data_root)` | the data tree is a local folder; no S3-backed data root yet | low: the :9000 balancer cannot hold the data tree yet | `endpoints.s3` + a bucket | OPEN (the S3 drive is AzDrive's / AZCLOUD15's) |

## 2. AzMeet (examples/azul-meet) - AZMEET15

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| azul-meet/src/rooms.rs:240 (used :291, lib.rs:5756-5758, ui.rs:940; compiled into AzCalendar) | `http://127.0.0.1:8787` | last-resort meeting server (wrangler's / dev-server.mjs' port) | high: with nothing set AzMeet asks :8787, not the stack's :8790, and opens its in-process demo when that does not answer | read `endpoints.meet` (and the profile's `local` 8790) before the built-in | AZMEET15 |
| azul-meet/src/lib.rs:220-223 | `option_env!("AZMEET_DEFAULT_WORKER")` | production Worker baked in at build time | med: a release binary targets production unless the run says otherwise | `endpoints.meet` above the built-in | AZMEET15 |
| azul-meet/src/rooms.rs:280-308; lib.rs:5752-5763 | `--worker` > saved server > `AZMEET_WORKER` > built-in | the order | med: a saved production server beats `AZMEET_WORKER`, which is how AzCalendar's "Join meeting" names the event's server (azul-calendar/src/lib.rs:1118-1130) | AzCalendar should pass `--worker <server> --join <link>`; or `AZMEET_WORKER` above saved | AZMEET15 (+ the AzCalendar launch line) |
| azul-meet/src/rooms.rs:210-217; lib.rs:5654-5664, 5837-5842; dll/src/desktop/extra/iroh/types.rs:50, engine.rs:155-160 | relay `Default` (n0's public relays) for a non-loopback server, `Off` for loopback | iroh relay choice | med: the stack's relay (:3340) is used only when `--relay` / `AZMEET_RELAY` names it | `endpoints.relay` under `--relay` / `AZMEET_RELAY` | AZMEET15 |
| azul-meet/src/lib.rs:215-230, 5735-5744; rooms.rs:14 | polls 2000 ms, re-announce every 10 polls (the Worker's TTL is 120 s), HTTP 5 s, start-up probe 800 ms / 2500 ms | Worker cadence and timeouts | low: a slow local Worker sends AzMeet into its demo | `AZMEET_POLL_MS`, `AZMEET_PROBE_MS` | AZMEET15 |
| azul-meet/src/lib.rs:4106-4108 | `AzMeet/0.1` | User-Agent | low | `CARGO_PKG_VERSION` | AZMEET15 |
| azul-meet/src/lib.rs:4645-4672 | `<tmp>/azmeet-<pid>` | data root of a headless run without `--data-dir` / `AZLIN_DATA` | low (the kit still reads `~/.azlin/config.json`) | `AZLIN_CONFIG=off` in headless runs | AZMEET15 |
| azul-meet/scripts/meet-e2e.mjs:37-42, 160-163; two-clients.mjs:76; three-clients.mjs:76 | `../azul-apps/cf-workers/meet`; each script starts its own dev server on 8787 / 8797 / 8773 | e2e harness | low: cannot use the stack's running Worker on 8790 | `--worker <url>` to reuse it | AZMEET15 |

## 3. AzCalendar (examples/azul-calendar)

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| azul-calendar/src/meeting.rs:36-56; args.rs; lib.rs:1197-1210, 1268-1274 | saved > `AZMEET_WORKER` > `AZMEET_DEFAULT_WORKER` > `http://127.0.0.1:8787`; no switch | the meeting server new links are registered with | high: nothing a local profile can set, no switch for one run | `--worker` > saved > `AZMEET_WORKER` > `endpoints.meet` > built-in > AzMeet's local server | FIXED 96582dc0d / 404dbbdc6 |
| azul-calendar/src/lib.rs:131, 1227-1230; event.rs:101, 596-601 | `AZCAL_DATA`, else `<user data dir>/AzCalendar`; `--data-dir` rejected | the calendar's folder (events, calendars, `settings.txt` with `meeting_server=`) | high: ignores `AZLIN_DATA` - a run that sets only it uses the user's real calendar | `--data`, `AZCAL_DATA` (the local profile sets it); where under the Azlin root the events go (`<root>/calendar/events/` or `<root>/events/` as the S3 split says) | USER |
| azul-calendar/src/tasks.rs:44-52; lib.rs:1239-1244 | `AZTASKS_DATA`, else `<user data dir>/Azlin` | the To-Do bar's task store | high: with only `AZLIN_DATA` set the user's real `<user data>/Azlin/tasks/` was read, written and migrated into | `--data` / `AZCAL_DATA` > `AZTASKS_DATA` > `AZLIN_DATA` > the user's folder | FIXED d2bab5687 / 5677e5cc9 |
| azul-calendar/src/lib.rs:1118-1130; meeting.rs:242-246 | `AZMEET_JOIN` + `AZMEET_WORKER=<event's server>` as variables | "Join meeting" | med: AzMeet prefers its saved server (section 2) | pass `--worker <server> --join <link>` | OPEN (with AZMEET15) |
| azul-calendar/src/lib.rs:132, 913-914 | 10 s, UA `AzCalendar/0.1` | `POST /rooms` timeout | low | `calendar.http_timeout_secs` | OPEN |
| azul-calendar/src/lib.rs:134-137, 1080, 1298-1302 | sync every 30 s (`AZCAL_SYNC_SECONDS`), retry forever without backoff; reminder tick 20 s | cadence | low: a reminder e2e waits 20 s | `AZCAL_REMINDER_TICK_MS` | OPEN |
| azul-calendar/scripts/mint-and-join.mjs:49, 339; offline_links.py:181, 217 | their own dev servers | e2e harness | low | `--worker <url>` | OPEN |

## 4. AzMail (examples/azul-mail) - AZMAIL15

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| azul-mail/src/lib.rs:1412-1429; account.rs:39, 304-322 | `<user data dir>/AzMail` renamed into `<root>/mail` whenever `AZMAIL_DATA` is unset - also when the root came from `--data-dir` / `AZLIN_DATA` | one-time legacy move | **high: a run with only `AZLIN_DATA` set moves the user's real mail into the test folder (and a cleanup deletes it)** | migrate only for the default root, as azul-appkit's migrate.rs:58 does; the local profile sets `AZMAIL_DATA` | AZMAIL15 |
| azul-mail/src/send.rs:91-95, 266-272 | `SendRoute::Direct`, port 25 | default send route: straight to each recipient's mail server | high: a test account without `sending.json` delivers to real mail servers | `endpoints.smtp` (a local sink, 127.0.0.1:2525) / `mail.route` | AZMAIL15 |
| azul-mail/src/send.rs:520-531 | `gmail-smtp-in.l.google.com:25`, `outlook-com.olc.protection.outlook.com:25`; 6 s; every 3600 s | the port-25 probe | med: a local test connects to Google and Microsoft | `sending.json` `port25_probe` (exists) / `mail.port25_probe` | AZMAIL15 |
| azul-mail/src/dkim.rs:369 (microdns resolvers 1.1.1.1 / 8.8.8.8 / 8.8.4.4) | DNS for DKIM / DMARC / SPF and the MX lookups | the resolvers | med: the local stack cannot serve its own records | `mail.dns_resolver` / `AZMAIL_DNS` | AZMAIL15 |
| azul-mail/src/lib.rs:643-649; account.rs:50-53 | `AZMAIL_TEST_CA`, `AZMAIL_TEST_PASSWORD` only with `AZ_BACKEND=headless` | a test IMAP server's certificate and password | med: a windowed run cannot trust a self-signed local IMAP server | per-account `extra_ca_file` | AZMAIL15 |
| azul-mail/src/account.rs:116-240, 266-268; send.rs:241-243; sending.rs:20 | Gmail / Outlook / iCloud / Fastmail servers, `imap.<domain>:993`, `smtp.<domain>:465`, 587; keyring names `AzMail/<id>/imap`, `/dkim` | form prefills and keyring entries | low / med (a windowed test with the user's real address overwrites their stored password) | `mail.imap` / `mail.smtp`; `AZ_KEYRING_SERVICE` | AZMAIL15 |
| azul-mail/src/sample.rs:69-101 | IMAP `localhost:1143` (no TLS), SMTP `localhost:2525` | the `--sample` account | low (start scripts/imap_server.py with `--port 1143`) | `endpoints.imap` / `endpoints.smtp` | AZMAIL15 |
| azul-mail/src/imap_client.rs:25-27; submit.rs:72; pictures.rs:15; ui_main.rs:1889-1893; send.rs:238; sync.rs:151-152 | IMAP 20 s / 120 s, submission 60 s, pictures 20 s, give up after 5 days, batches of 25 / 8 MiB | timeouts and batches | low | `mail.timeouts.*` | AZMAIL15 |
| azul-mail/scripts/sync_e2e.py:394-399 | only `AZMAIL_DATA` is set | e2e environment | med: the kit's `mail/settings.json`, the To-Do store and `~/.azlin/config.json` are the user's | add `AZLIN_DATA` and `AZLIN_CONFIG` | AZMAIL15 |

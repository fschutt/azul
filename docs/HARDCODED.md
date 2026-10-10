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
| azul-meet/src/rooms.rs:240 (used :291, lib.rs:5756-5758, ui.rs:940; compiled into AzCalendar) | `http://127.0.0.1:8787` | last-resort meeting server (wrangler's / dev-server.mjs' port) | high: with nothing set AzMeet asks :8787, not the stack's :8790, and opens its in-process demo when that does not answer | read `endpoints.meet` (and the profile's `local` 8790) before the built-in | FIXED: the constant and the demo are gone (999e99822, fe0e02b28); `meeting_server` resolves through `azlin_config` (e953029e4): `--worker` > saved > `AZMEET_WORKER` > `endpoints.meet` > `AZMEET_DEFAULT_WORKER` > the profile's (`local` 8790) > none, and the start screen asks, with Retry |
| azul-meet/src/lib.rs:220-223 | `option_env!("AZMEET_DEFAULT_WORKER")` | production Worker baked in at build time | med: a release binary targets production unless the run says otherwise | `endpoints.meet` above the built-in | FIXED e953029e4 (the file and the environment outrank it; it outranks only the profile's address) |
| azul-meet/src/rooms.rs:280-308; lib.rs:5752-5763 | `--worker` > saved server > `AZMEET_WORKER` > built-in | the order | med: a saved production server beats `AZMEET_WORKER`, which is how AzCalendar's "Join meeting" names the event's server (azul-calendar/src/lib.rs:1118-1130) | AzCalendar should pass `--worker <server> --join <link>`; or `AZMEET_WORKER` above saved | FIXED e65274ff6 (AzCalendar passes the switches); the saved one still outranks `AZMEET_WORKER` (USER, below) |
| azul-meet/src/rooms.rs:210-217; lib.rs:5654-5664, 5837-5842; dll/src/desktop/extra/iroh/types.rs:50, engine.rs:155-160 | relay `Default` (n0's public relays) for a non-loopback server, `Off` for loopback | iroh relay choice | med: the stack's relay (:3340) is used only when `--relay` / `AZMEET_RELAY` names it | `endpoints.relay` under `--relay` / `AZMEET_RELAY` | FIXED e953029e4: `--relay` > `AZMEET_RELAY` > `endpoints.relay` > the profile's (`local` 3340, `production` n0's) > the old rule |
| azul-meet/src/lib.rs:215-230, 5735-5744; rooms.rs:14 | polls 2000 ms, re-announce every 10 polls (the Worker's TTL is 120 s), HTTP 5 s, start-up probe 800 ms / 2500 ms | Worker cadence and timeouts | low: a slow local Worker sends AzMeet into its demo | `AZMEET_POLL_MS`, `AZMEET_PROBE_MS` | OPEN (no demo any more: a slow Worker is an error with Retry; the re-announce follows the Worker's `peer_ttl_seconds`, fe0e02b28) |
| azul-meet/src/lib.rs:4106-4108 | `AzMeet/0.1` | User-Agent | low | `CARGO_PKG_VERSION` | FIXED 999e99822 |
| azul-meet/src/lib.rs:4645-4672 | `<tmp>/azmeet-<pid>` | data root of a headless run without `--data-dir` / `AZLIN_DATA` | low (the kit still reads `~/.azlin/config.json`) | `AZLIN_CONFIG=off` in headless runs | AZMEET15 |
| azul-meet/scripts/meet-e2e.mjs:37-42, 160-163; two-clients.mjs:76; three-clients.mjs:76 | `../azul-apps/cf-workers/meet`; each script starts its own dev server on 8787 / 8797 / 8773 | e2e harness | low: cannot use the stack's running Worker on 8790 | `--worker <url>` to reuse it | OPEN for the node scripts; scripts/azmeet_e2e.py takes `--worker-url` (with `--sqld-url` / `--sqld-token-file` for its database) and `--relay-url` (1d78b7576) |

## 3. AzCalendar (examples/azul-calendar)

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| azul-calendar/src/meeting.rs:36-56; args.rs; lib.rs:1197-1210, 1268-1274 | saved > `AZMEET_WORKER` > `AZMEET_DEFAULT_WORKER` > `http://127.0.0.1:8787`; no switch | the meeting server new links are registered with | high: nothing a local profile can set, no switch for one run | `--worker` > saved > `AZMEET_WORKER` > `endpoints.meet` > built-in > AzMeet's local server | FIXED 96582dc0d / 404dbbdc6; the last resort is the profile's address (`local` 8790) when none is built in, no 8787 (158356313) |
| azul-calendar/src/lib.rs:131, 1227-1230; event.rs:101, 596-601 | `AZCAL_DATA`, else `<user data dir>/AzCalendar`; `--data-dir` rejected | the calendar's folder (events, calendars, `settings.txt` with `meeting_server=`) | high: ignores `AZLIN_DATA` - a run that sets only it uses the user's real calendar | `--data`, `AZCAL_DATA` (the local profile sets it); where under the Azlin root the events go (`<root>/calendar/events/` or `<root>/events/` as the S3 split says) | USER |
| azul-calendar/src/tasks.rs:44-52; lib.rs:1239-1244 | `AZTASKS_DATA`, else `<user data dir>/Azlin` | the To-Do bar's task store | high: with only `AZLIN_DATA` set the user's real `<user data>/Azlin/tasks/` was read, written and migrated into | `--data` / `AZCAL_DATA` > `AZTASKS_DATA` > `AZLIN_DATA` > the user's folder | FIXED d2bab5687 / 5677e5cc9 |
| azul-calendar/src/lib.rs:1118-1130; meeting.rs:242-246 | `AZMEET_JOIN` + `AZMEET_WORKER=<event's server>` as variables | "Join meeting" | med: AzMeet prefers its saved server (section 2) | pass `--worker <server> --join <link>` | FIXED e65274ff6 |
| azul-calendar/src/lib.rs:132, 913-914 | 10 s, UA `AzCalendar/0.1` | `POST /rooms` timeout | low | `calendar.http_timeout_secs` | OPEN |
| azul-calendar/src/lib.rs:134-137, 1080, 1298-1302 | sync every 30 s (`AZCAL_SYNC_SECONDS`), retry forever without backoff; reminder tick 20 s | cadence | low: a reminder e2e waits 20 s | `AZCAL_REMINDER_TICK_MS` | OPEN |
| azul-calendar/scripts/mint-and-join.mjs:49, 339; offline_links.py:181, 217 | their own dev servers | e2e harness | low | `--worker <url>` | OPEN |

## 4. AzMail (examples/azul-mail) - AZMAIL15

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| azul-mail/src/lib.rs:1412-1429; account.rs:39, 304-322 | `<user data dir>/AzMail` renamed into `<root>/mail` whenever `AZMAIL_DATA` is unset - also when the root came from `--data-dir` / `AZLIN_DATA` | one-time legacy move | **high: a run with only `AZLIN_DATA` set moves the user's real mail into the test folder (and a cleanup deletes it)** | migrate only for the default root, as azul-appkit's migrate.rs:58 does; the local profile sets `AZMAIL_DATA` | AZMAIL15 |
| azul-mail-core/src/send.rs:91-95, 266-272 | `SendRoute::Direct`, port 25 | default send route: straight to each recipient's mail server | high: a test account without `sending.json` delivers to real mail servers | `endpoints.smtp` (a local sink, 127.0.0.1:2525) / `mail.route` | AZMAIL15 |
| azul-mail-core/src/send.rs:520-531 | `gmail-smtp-in.l.google.com:25`, `outlook-com.olc.protection.outlook.com:25`; 6 s; every 3600 s | the port-25 probe | med: a local test connects to Google and Microsoft | `sending.json` `port25_probe` (exists) / `mail.port25_probe` | AZMAIL15 |
| azul-mail-core/src/dkim.rs:369 (microdns resolvers 1.1.1.1 / 8.8.8.8 / 8.8.4.4) | DNS for DKIM / DMARC / SPF and the MX lookups | the resolvers | med: the local stack cannot serve its own records | `mail.dns_resolver` / `AZMAIL_DNS` | AZMAIL15 |
| azul-mail/src/lib.rs:643-649; account.rs:50-53 | `AZMAIL_TEST_CA`, `AZMAIL_TEST_PASSWORD` only with `AZ_BACKEND=headless` | a test IMAP server's certificate and password | med: a windowed run cannot trust a self-signed local IMAP server | per-account `extra_ca_file` | AZMAIL15 |
| azul-mail-core/src/account.rs:116-240, 266-268; send.rs:241-243; sending.rs:20 | Gmail / Outlook / iCloud / Fastmail servers, `imap.<domain>:993`, `smtp.<domain>:465`, 587; keyring names `AzMail/<id>/imap`, `/dkim` | form prefills and keyring entries | low / med (a windowed test with the user's real address overwrites their stored password) | `mail.imap` / `mail.smtp`; `AZ_KEYRING_SERVICE` | AZMAIL15 |
| azul-mail/src/sample.rs:69-101 | IMAP `localhost:1143` (no TLS), SMTP `localhost:2525` | the `--sample` account | low (start scripts/imap_server.py with `--port 1143`) | `endpoints.imap` / `endpoints.smtp` | AZMAIL15 |
| azul-mail/src/imap_client.rs:25-27; submit.rs:72; pictures.rs:15; ui_main.rs:1889-1893; send.rs:238; sync.rs:151-152 | IMAP 20 s / 120 s, submission 60 s, pictures 20 s, give up after 5 days, batches of 25 / 8 MiB | timeouts and batches | low | `mail.timeouts.*` | AZMAIL15 |
| azul-mail/scripts/sync_e2e.py:394-399 | only `AZMAIL_DATA` is set | e2e environment | med: the kit's `mail/settings.json`, the To-Do store and `~/.azlin/config.json` are the user's | add `AZLIN_DATA` and `AZLIN_CONFIG` | AZMAIL15 |

## 5. AzDrive and azul-storage (examples/azul-drive, examples/azul-storage)

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| azul-drive/src/lib.rs:1908-1912, 1945-1950; args.rs:20, 231-233, 342-391 | the user's home folder; `--sample` writes 9 items (`Documents/notes.txt`, `Code/main.rs`, `Pictures/gradient.png`, ...) into it | the Home drive, and the `--sample` target | high: `--sample` with only `AZLIN_DATA` set writes into the user's real home | `--home`, `AZDRIVE_HOME` (the local profile sets it); default to `<root>/home` when a data root is named | USER |
| azul-drive/src/lib.rs:1913-1917 | the OS Downloads folder | where Download saves | med | `--downloads`, `AZDRIVE_DOWNLOADS` | DESIGN (the profile sets it) |
| azul-drive/src/lib.rs:1918-1922; azul-storage/src/config.rs:31, 230-235 | `<OS config dir>/azul-storage/drives.json` | the drives the user added | med: adding the local S3 drive in a test writes it into the user's list | `--drives`, `AZUL_DRIVES` (the profile sets it) | DESIGN |
| azul-drive/src/browse.rs:547, 570, 609; ui_dialogs.rs:202 | region `us-east-1` when blank (the Azlin token server says the same), `path_style: true`, placeholder `https://s3.eu-central-1.amazonaws.com`, no endpoint prefilled | the Add-drive form | low: the right shape for the :9000 balancer; nothing prefills it | prefill from `endpoints.s3` | OPEN (AZCLOUD15's Azlin drive signs up through the token server instead) |
| azul-storage/src/azul_transport.rs:30-32 (AzDrive lib.rs:186, jobs.rs:1033, actions.rs:2380; UA lib.rs:125) | 60 s + 30 s grace; UA `AzDrive/0.2` | S3 request timeout | low (`with_timeout` exists, AzDrive never calls it) | `storage.http_timeout_secs` | OPEN |
| azul-drive/src/actions.rs:2364; azul-storage/src/s3.rs:434 | 7 days | presigned share link expiry (also the maximum) | low | `drive.link_expiry_secs` | OPEN |
| azul-storage/src/config.rs:103-106, 239-240 | `DriveAuth::AccessLink` -> Unsupported; keyring names `azul-storage/s3/<drive id>` | access-link drives; S3 credentials in the keyring | low today; a windowed test leaves the stack's test keys in the user's keychain | `endpoints.token`; `AZ_KEYRING_SERVICE` | AZCLOUD15 / OPEN |
| azul-drive/src/lib.rs:2077; azul-tasks/src/detail.rs:1032 | `<tmp>/AzDrive-open`, `<tmp>/AzTasks-open/<id>` | where Open copies a file for the OS | low: parallel runs share the folder | `<tmp>/<app>-<pid>` | OPEN |
| azul-drive/scripts/browse.py:369-385 | the caller's environment only | AzDrive's e2e | med: `drive/view.json`, the "Azlin" drive and `~/.azlin/config.json` were the user's | `--data-dir <logs>/data`, `AZLIN_CONFIG=off` | FIXED 999636fe7 |
| azul-drive/scripts/browse.py:57-59; s3_server.py:60-62, 716-806 | `AKIDAZDRIVEE2E` / `azdrive-e2e-secret-key`, `us-east-1`; `--port 9000` | mock S3 credentials and port | low: 9000 is the stack's balancer when started by hand | `--port`, `--access-key` | DESIGN (test) |

## 6. AzMaps and the map widget

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| layout/src/widgets/map.rs:351-372; azul-maps/src/lib.rs (`MapTileLayer::default()`) | `https://tiles.openfreemap.org/planet/20260531_080002_pt/{z}/{x}/{y}.pbf`, zoom 0-14 | the vector-tile server, pinned to a dated planet build | med: AzMaps always fetched the public host (no offline e2e); the dated path goes stale (empty tiles) | AzMaps: `--tiles` > `AZMAPS_TILES` > `endpoints.tiles` > the widget's own | FIXED d77ee39f8 / a68c5afc6 (an engine-wide `AZ_MAP_TILES_URL`, and resolving the planet build from its TileJSON, OPEN) |
| dll/src/desktop/extra/map/mod.rs:204-205 -> layout/src/http.rs:204-206 | 30 s, UA `azul-http/1.0`, 100 MB | tile download requests | low | `map.http_timeout_secs` | OPEN |

## 7. The other apps

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| azul-tasks/src/lib.rs:77-83, 419-445 | `--data` > `AZTASKS_DATA` > `<user data dir>/Azlin`; `--data-dir` rejected | AzTasks' data root (the Azlin root: `tasks/`, `aztasks/settings.json`) | high: a run that sets only `AZLIN_DATA` used the user's real root | `--data` > `AZTASKS_DATA` > `AZLIN_DATA` > the user's folder | FIXED d2bab5687 / 5677e5cc9 (`--data-dir` as an alias: OPEN) |
| azul-builder/src/lib.rs:26-41 | `8080` | the debug server AzBuilder opens in the browser | med: the local stack's sqld port - the bind fails or the browser opens sqld | `AZ_DEBUG` > 8765 | FIXED db34c5c6e / cd15020e4 |
| azul-news/scripts/feed_server.py:219 | `--port 8790` | the manual test feed server | med: the stack's meeting server port | a free port, printed (`FEED_SERVER <url>`); the e2e already asked for one | FIXED 999636fe7 |
| azul-news/src/jobs.rs:33-41; fetch.rs:22, 149-162 | feeds 30 s, pictures 20 s, UA `AzNews/0.1`; a bare host becomes `https://` | feed downloads | low: `127.0.0.1:8790/feed.xml` typed without a scheme fails | type the scheme | DESIGN |
| azul-music/src/scan.rs:44-50; app.rs:87, 235-243 | `$HOME/Music` | the music library | low: a test root still scans the user's library | `--music-dir` / `AZMUSIC_DIR` | OPEN |
| azul-player/src/app.rs:106-139; ui.rs:1244-1245 | the OS media folders; the sample URL `https://test-videos.co.uk/...` | media folders, the address dialog's sample | low | `--music-dir` ... exist; `player.sample_url` | DESIGN |
| azul-shells/scripts/shells_e2e.py:155; azul-meet/scripts/meet-e2e.mjs:174-181 | no `AZLIN_CONFIG` | e2e environments | low: they read the user's shared look | `AZLIN_CONFIG=off` | OPEN |
| examples/azul-{maps,meet,paint,widgets,writer}/Dockerfile:13 | `AZ_BACKEND="web://0.0.0.0:8080..."` | the web demos | low: the stack's sqld port inside one container | another port | DESIGN |

## 8. The engine (layout, dll, core) under the apps

| file:line | value | what it is | risk | config key | status |
|---|---|---|---|---|---|
| layout/src/request.rs:483-503, 709-760 | with `AZ_E2E` / `AZ_E2E_TEST` set every unmocked HTTP request is refused; a canned answer replaces, never passes through | the e2e mock policy | high: a scripted run could not reach the local stack at all | `AZ_E2E_ALLOW_HTTP=http://127.0.0.1:*,...` (comma-separated, the mock table's patterns) | FIXED 943f9160b / 84fa9cfcc |
| dll/src/desktop/extra/keyring/apple.rs:25, linux.rs:28/127, windows.rs:27; keyring/mod.rs:46-50 | `com.azul.keyring` | the one OS keyring service every app shares | med: a windowed test writes to (and can overwrite) the user's keychain; headless / `AZ_E2E_TEST` use an in-memory store | `AZ_KEYRING_SERVICE` | OPEN |
| layout/src/http.rs:204-206, 1030, 1509 | 30 s, 100 MB, `azul-http/1.0`; WebPKI roots only; reachability check 10 s; follows `HTTP(S)_PROXY` | default HTTP client | low: an https stack with its own CA cannot be trusted; a proxy in the environment catches 127.0.0.1 unless `NO_PROXY` | `AZ_EXTRA_CA_FILE`, `AZ_HTTP_TIMEOUT_SECS`; the profile sets `NO_PROXY` | OPEN |
| layout/src/telemetry/config.rs:52-68, 214; queue.rs:25, 106-114; mod.rs:1320 | `AZ_OBSERVE=1` -> `http://127.0.0.1:4318` with token `azul-demo-token`; 60 s flush, 20 s upload, 3 s crash upload; queue under `{data_dir}/{app}/telemetry/` | telemetry | low (off by default; every Azlin app reports as `azul-app` unless it sets `updates.app_name`) | `AZ_TELEMETRY*`, `AZ_OBSERVE=<url>`; the profile sets `AZ_TELEMETRY=off` | DESIGN |
| layout/src/telemetry/crash_mail.rs:56, 85-95; dll/src/desktop/app.rs:509-519 | SMTP ports `[25, 587, 2525]`, straight to the mail server of `report_problem`'s domain (`...@localhost` goes to 127.0.0.1) | crash reports by mail | low (no Azlin app sets `report_problem`) | `with_ports`; `AZ_CRASH_MAIL_RELAY` | OPEN |
| layout/src/updater.rs:2300-2316, 2359; core/src/resources.rs:195 | the state folder `<data>/<app_name>`; no manifest URL | the updater | low: inactive until an app sets `AppConfig.updates.manifest_url` (none does) | `endpoints.updates` once an app ships updates | DESIGN |
| dll/src/desktop/extra/sqlite/mod.rs:119, 325-329; core/src/db.rs:584-585 | `<OS local data>/azul-db/<name>.sqlite`; no sync URL | the engine's database and its Turso sync | low (no Azlin app uses it yet) | `DbConfig.backup_sync_url` -> `endpoints.db` (sqld at :8080) | OPEN |
| dll/src/desktop/shader_cache.rs:172; css/src/rice.rs:789, 798; layout/src/icon_remap.rs:182 | `~/Library/Caches/azul/shaders`, `~/.azul`, `<config>/azul/styles/<app>.css` | engine folders | low: outside `AZLIN_DATA`; `HOME` (and `XDG_*`) re-root them | - | DESIGN |

## 9. Data folders, app by app

- **The kit rule** (`--data-dir` > `AZLIN_DATA` > `<user data dir>/Azlin`, a folder per app):
  AzCalculator, AzClock, AzCode, AzContacts, AzDashboard, AzErp, AzKeys (its device secret is a
  keyring entry), AzMaps, AzMonitor, AzNews, AzNotes, AzPaint, AzPdf, AzPhoto, AzReader, AzReview,
  AzSetup, AzSheets, AzShells, AzShow, AzTerm, AzVideoCut, AzWriter; AzMeet (a headless run without
  either uses `<tmp>/azmeet-<pid>`); AzMusic and AzPlayer for their data (their media folders are
  their own settings).
- **The kit rule plus folders of their own**: AzMail (`AZMAIL_DATA`, else `<root>/mail`, with the
  legacy move of section 4); AzDrive (`--home` / `AZDRIVE_HOME`, `--downloads` /
  `AZDRIVE_DOWNLOADS`, `--drives` / `AZUL_DRIVES`, all outside the root).
- **Not the kit rule**: AzCalendar (`--data` > `AZCAL_DATA` > `<user data>/AzCalendar`; its task
  store follows `AZLIN_DATA` since 5677e5cc9) and AzTasks (`--data` > `AZTASKS_DATA` > `AZLIN_DATA`
  since 5677e5cc9 > `<user data>/Azlin`); neither takes `--data-dir`.
- **The engine** writes outside every root: the keyring, telemetry, the updater state, the database
  store, the shader cache, `~/.azul`. On macOS and Linux a `HOME` (and `XDG_*`) pointing at a
  temporary folder re-roots all of them, `~/.azlin` too - except the keyring.

## 10. Decisions for the user

- Where AzCalendar's events go under the Azlin root (`<root>/calendar/events/` like every kit app's
  folder, or `<root>/events/` as the S3 split names it) - until then `AZCAL_DATA` isolates it.
- Whether AzDrive's Home drive (and `--sample`) should follow a named data root (`<root>/home`).
- Whether AzMeet (and AzCalendar, which follows it) should rank `AZMEET_WORKER` above the server
  saved in the app; today `--worker` is the way to override a saved one.
- Whether a keyring service per profile (`AZ_KEYRING_SERVICE`) is wanted, so a windowed local-stack
  run never touches the user's keychain.

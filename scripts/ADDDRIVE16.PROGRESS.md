# ADDDRIVE16 - AzDrive "Add drive" (Buy storage / Connect data source)

Branch: worktree-agent-af6b1d28bc92a26e0 (from fix/input-bugs-2026-09-19 at 0a8caf1a2).
House rules: no cargo / no running apps (the lead builds and runs), RED test commits before
the feature commits, no cargo fmt, api.json untouched, nothing secret in this file.

## Decisions (with the lead's course change)

- Azlin cloud client code lives in a NEW crate `examples/azcloud-kit` (workspace member, public,
  no dependency on private crates): `token` (tiers, dev signup, checkout + poll, credential
  refresh - the routes of the token server), `bundle` (the signup bundle), `session` (what the
  keyring keeps for an Azlin drive), `endpoints` (the token server from azul-appkit's
  `azlin_config::resolve_endpoints`), `drive` (an Azlin drive that refreshes its 12 h
  credentials and hands the rotated drive token back to the app). azul-storage gets NO token
  client; it only learns to READ `{"type": "azlin", ...}` (`DriveAuth::Azlin`).
- One keyring entry per drive (`config::keyring_key(id)`): S3 -> the credentials JSON (as
  before); Azlin -> the session JSON (a superset of the credentials JSON, so an app without
  azcloud-kit still reads 12 h of access); OpenDAL / database -> `SecretOptions` JSON.
  (azcloud-api's CLI file store uses two entries, `azul-storage/s3/<id>` + `azul-storage/azlin/
  <id>`; the GUI keeps ONE because azul's keyring answers one request at a time.)
- OpenDAL 0.59.4 (`opendal`, default-features off): no reqwest. OpenDAL's pluggable
  `HttpTransport` is implemented over azul-storage's own `Transport` seam (AzulTransport in the
  app = azul's HTTP client, a fake in tests), so every HTTP service goes through azul's TLS
  stack. One shared multi-thread tokio runtime (2 workers) for OpenDAL and sqlx; the Drive
  stays blocking (block_on from the azul Thread, the blocking transport on tokio's blocking
  pool).
- Curated services (feature `opendal`): HTTP-only ones + FTP + Redis. Excluded: sftp (Unix only,
  needs the system ssh), hdfs / rocksdb / foundationdb / tikv / etcd (C/C++/Java/protoc),
  Hugging Face (hf-xet pulls reqwest), MongoDB (heavy; later behind its own feature).
  S3-compatible and Local folder stay NATIVE (S3Drive / LocalDrive), available in every build.
- Databases (feature `sql`): tables as folders, implemented over sqlx 0.8 (the driver OpenDAL's
  postgresql / mysql / sqlite services use - no second driver). OpenDAL's own sql services
  (one key/value table) are NOT used. Layout: `<table>/` (Postgres outside `public`:
  `<schema>.<table>/`) holding `<table>.csv` (first 10,000 rows), `schema.json` and `rows/`
  (one JSON file per row, named by the primary key; first 10,000).
- The Add-drive dialog is azul's modal `Dialog` = a `<transient-window>` with
  `TransientAnchor::Viewport` (top layer, backdrop) owned by AzDrive's window; `--dialogs
  inline` keeps the in-window sheet for scripts. Its callbacks answer `RefreshDomAllWindows`
  (the dialog's content is a subtree of the main window's DOM).

## Contract for the E2E (ids, markers)

Ids (src/ids.rs): dialog root `__azdrive_add_drive`; pages `__azdrive_add_choose`,
`__azdrive_add_buy`, `__azdrive_add_sources`, `__azdrive_add_form`; choices
`__azdrive_add_choice_buy`, `__azdrive_add_choice_connect`; `__azdrive_add_back`;
service rows `__azdrive_add_service_<service id>`; fields `__azdrive_add_field_<key>`
(a TextInput; Bool fields a CheckBox row); buttons `__azdrive_add_test`, `__azdrive_add_save`,
`__azdrive_add_cancel`; tiers `__azdrive_add_tier_<index>`; `__azdrive_add_yearly`,
`__azdrive_add_create_test`, `__azdrive_add_buy_button`, `__azdrive_add_stop`; status line
`__azdrive_add_status`, error `__azdrive_add_error`.
Stdout: `AZDRIVE_ADD_PAGE choose|buy|sources|form <service>`, `AZDRIVE_TIERS <n>`,
`AZDRIVE_TESTED ok|error`, `AZDRIVE_ADDED <drive id>`, `AZDRIVE_CHECKOUT <checkout id>`
(the pay URL is not printed), `AZDRIVE_LISTED <drive id> <prefix or /> <n>`.

## DONE (all uncompiled - the lead builds)

- da3ef5fc5 RED / 276042fd0 azul-storage: DriveAuth::Azlin, DriveLocation::Opendal / Database,
  SecretOptions, open_with_secret, catalog (5 groups, 32 sources, forms), Method verbs,
  runtime, Cargo features `opendal` / `sql`.
- 389810579 RED / a3931c0c3 azul-storage: OpendalDrive + TransportHttp (OpenDAL's HTTP through
  the app's Transport).
- 0e7f97158 RED / 4e28fec34 azcloud-kit (new workspace member): token, bundle, session,
  endpoints, drive (AzlinDrive).
- 24b4b02ca RED / 0b30c7e9f (database fork, merged 29116a226) azul-storage: tables.rs (pure
  layout) + database.rs (DatabaseDrive over sqlx), SQLite tests.
- 646b53453 RED / 2cf615ca3 AzDrive: add_drive.rs (the dialog as data) + features + azcloud-kit.
- fa734e079 AzDrive: the dialog (ui_add_drive.rs, add_flow.rs, jobs, Slot::secret, Azlin
  drives through azcloud-kit, rotated sessions to the keyring, read-only sources, ribbon Home /
  Computer, source list, Options, details, Properties; --token-url / --profile).
- 2c671eb32 mock token server: the real price ladder + checkouts.
- 62479727a E2E: scripts/azdrive_add_e2e.py; azdrive_e2e.py / browse.py follow the rename.
- Compile review fork (reading only): no compile error found; 4d2e97a1f (OpenDAL's deprecated
  remove_all -> delete_with(recursive)). 8f6a5c5d9: a late payment's drive does not take over
  the window; the two fields the review found unread are read.

## NEXT

- Nothing open on this branch: the lead builds, runs the suites and the E2E.

## Open questions / gaps found

- ENGINE GAP: azul's `HttpMethod` (layout/src/http.rs:1147) has GET HEAD POST PUT PATCH DELETE
  only; WebDAV needs PROPFIND / MKCOL / MOVE / COPY. Through AzulTransport the OpenDAL WebDAV
  service cannot list until the engine sends custom verbs (proposal: an
  `HttpMethod::Custom(AzString)` / `Other` variant, ureq 3 builds any method through
  `http::Request` + `agent.run`).
- ENGINE GAP: `Url::open` / `Url::open_path` (core/src/url.rs:136, spawn_opener at :248) start
  the system's browser / opener even in a headless or E2E run (the keyring has an in-memory
  stand-in there, the opener none). So the E2E does not drive Buy (a checkout opens its
  payment page). Proposal: under AZ_BACKEND=headless / AZ_E2E_TEST record the request (a log
  line, a debug-server op that lists them) instead of spawning.
- A payment made after "Stop waiting" (or after AzDrive closed) creates the drive at the token
  server, but the signup bundle is handed over to a poll only: the app never sees it. The token
  server needs a way back (e.g. the checkout id + a claim code kept by the app, or the drive
  mailed to the account) - a server-side decision.
- OAuth sources (Google Drive, Dropbox, OneDrive) take a refresh token + client id / secret in
  the form today; an in-app OAuth flow (browser + loopback redirect) is the next step.
- Two AzDrive processes (File > Open new window starts one) each hold an Azlin drive's session:
  when one refreshes (the drive token rotates), the other still holds the spent token, and its
  next refresh makes the token server revoke the device. A fix needs the refresh to re-read the
  keyring under a cross-process lock (azcloud-api's CLI does this with a lock file), but azul's
  keyring is request / answer on the UI thread only (CallbackInfo::keyring_*), so a worker
  thread cannot re-read it - either route the refresh through the UI thread or give the
  keyring a blocking call for threads.
- Supply-chain gate: the 14-day publish-age cooldown refuses opendal 0.59.4 and its 0.59.4
  service crates (published 2026-10-05) until 2026-10-19, and other new crates published
  late September (reqsign 3.x, redis 1.7, asyncband 0.7.3, jiff 0.2.38): wait, pin older
  versions with `cargo update --precise`, or a self-expiring exemption; cargo-vet needs
  exemptions for every new crate.

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
  (one key/value table) are NOT used.
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
`__azdrive_add_cancel`; tiers `__azdrive_add_tier_<index>`; `__azdrive_add_create_test`,
`__azdrive_add_buy_button`; status line `__azdrive_add_status`, error `__azdrive_add_error`.
Stdout: `AZDRIVE_ADD_PAGE choose|buy|sources|form <service>`, `AZDRIVE_TIERS <n>`,
`AZDRIVE_TESTED ok|error`, `AZDRIVE_ADDED <drive id>`, `AZDRIVE_CHECKOUT <checkout id>`
(the pay URL is not printed), `AZDRIVE_LISTED <drive id> <prefix or /> <n>`.

## DONE

- da3ef5fc5 RED / 276042fd0 azul-storage: DriveAuth::Azlin, DriveLocation::Opendal / Database,
  SecretOptions, open_with_secret, catalog (5 groups, 32 sources, forms), Method verbs,
  runtime, Cargo features `opendal` / `sql`.
- 389810579 RED / a3931c0c3 azul-storage: OpendalDrive + TransportHttp (OpenDAL's HTTP through
  the app's Transport).
- 0e7f97158 RED / 4e28fec34 azcloud-kit (new workspace member): token, bundle, session,
  endpoints, drive (AzlinDrive).
- A fork (database helper) builds tables.rs + database.rs (sqlx tables-as-folders) in its own
  worktree; merged when it reports.

## IN PROGRESS

- AzDrive: the Add drive dialog.

## NEXT

4. AzDrive: add_drive.rs (pure dialog model) + ui_add_drive.rs + jobs + wiring (ribbon Home
   and Computer, sidebar row and + menu, Options > Drives), Azlin drives open via azcloud-kit.
5. E2E: scripts/azdrive_add_e2e.py (+ mock token server: prices, checkout), update browse.py
   and azdrive_e2e.py for the renamed "Add drive".

## Open questions / gaps found

- ENGINE GAP: azul's `HttpMethod` (layout/src/http.rs:1147) has GET HEAD POST PUT PATCH DELETE
  only; WebDAV needs PROPFIND / MKCOL / MOVE / COPY. Through AzulTransport the OpenDAL WebDAV
  service cannot list until the engine sends custom verbs (proposal: an
  `HttpMethod::Custom(AzString)` / `Other` variant, ureq 3 builds any method through
  `http::Request` + `agent.run`).

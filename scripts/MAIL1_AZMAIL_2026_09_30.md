# MAIL1 - AzMail: sign in to a real IMAP account and sync the mail to a local folder

Branch `wt/mail1-azmail`, base `a7e18f4df`. Nothing was compiled (house rule): the Rust is checked by reading and by
`rustfmt` as a parse check. The Python IMAP server and its 16 unittest tests do run and pass.

## What was built

A new app, `examples/azul-mail` (package `AzMail`, lib `azmail` + bin `AzMail`, `link-dynamic`, a workspace member,
`.cargo/config.toml` like AzCalendar). It signs in over IMAP on an azul `Thread` and writes every folder to files:

```text
<mail folder>/account.json                          the account (no secret)
<mail folder>/mail/<folder>/<yyyy>/<mm>/<uid>.eml   one message, the exact bytes the server sent
<mail folder>/mail/<folder>/index.jsonl             one line per message
<mail folder>/mail/<folder>/state.json              UIDVALIDITY, last synced UID, message count
<mail folder>/stale/<folder>/<old uidvalidity>/...  a folder's files from before the server renumbered it
```

`<mail folder>` is `<AzMail folder>/<account id>`; the AzMail folder is `AZMAIL_DATA`, else `AzMail` in the user's
data folder (`FilePath::get_data_dir`, `~/Library/Application Support` on macOS). The form's "Local mail folder"
field moves the mail anywhere else. The account id is the address in lower case (`ada@example.org`).
`<yyyy>/<mm>` is the month the server received the message (INTERNALDATE, UTC), so a path is known before the body
is fetched. Spam is literally `mail/spam` (the `\Junk` folder, or the folder called Spam / Junk / Junk E-mail / Bulk
Mail when the server marks none).

| Module | What |
|---|---|
| `account.rs` | provider presets (Gmail, Outlook / Office 365, iCloud, Fastmail) with their sign-in notes; account id, keyring name `AzMail/<id>/imap`; `account.json` (format `azmail.account` v1, no secret field at all); `Secret` (no `Display`, `Debug` prints `Secret(..)`, bytes zeroed on drop); the setup form's defaults and checks; `AZMAIL_TEST_PASSWORD` only under `AZ_BACKEND=headless`; an unencrypted connection only to localhost / 127.x / ::1 |
| `auth.rs` | `AUTHENTICATE PLAIN` when offered, else `LOGIN`, refused under `LOGINDISABLED`; `XOAUTH2` for a token; the SASL payloads |
| `mutf7.rs` | IMAP modified UTF-7 folder names (`Entw&APw-rfe` is `Entwürfe`) |
| `folders.rs` | server mailbox -> one-segment folder key: `inbox`, `spam`, `sent`, `drafts`, `archive`, `trash`, `all`, `flagged` (special use first, then the usual names), anything else its decoded name with the hierarchy as `.` (`Work.Projects`); keys unique even by case (`Notes`, `Notes-2`) |
| `store.rs` | `LocalFolder`: `put` (dot temp file + rename), `get`, `size_of`, `move_prefix`, `folders`; the key functions; `index.jsonl` (one line per UID, a line cut short by a crash is skipped); `state.json` |
| `sync.rs` | the sync over a `MailSource` trait (see below) |
| `imap_client.rs` | the real `MailSource`: the `imap` crate over AzMail's own stream (rustls + RustCrypto + Mozilla roots, or plain TCP to localhost) |
| `message.rs` | `mail-parser` for the index line and the message view; quote levels; UTC and local dates |
| `html.rs` | mail HTML -> well-formed XHTML for azul's XML parser (see "HTML view") |
| `lib.rs` | the window, the keyring flow, the sync thread |

### The sync (`sync.rs`)

Per selectable folder: `EXAMINE` (read-only `SELECT`); a UIDVALIDITY that differs from `state.json` moves the
folder's files to `stale/` and syncs from the start; a UIDNEXT at or below the last UID + 1 means nothing is new (no
search at all); else `UID SEARCH UID <last+1>:*`, keeping only UIDs above the last one (`n:*` always matches the
newest message, RFC 3501); `UID FETCH <set> (UID FLAGS RFC822.SIZE INTERNALDATE)` in chunks of 500; then, in UID
order, batches of at most 25 messages / 8 MiB: a message whose `.eml` already has the server's size (written before a
crash) is read back, the rest come with one `UID FETCH <set> (UID BODY.PEEK[])` (PEEK: nothing is marked read). Every
200 messages (or every tenth of a big index), and on every exit - done, error or "Stop" - the index and then the state
are written whole (fsync'ed; the `.eml` files are not, their size tells a lost one). So a second run fetches
nothing twice, and a run that dies anywhere is picked up by the next one without refetching.

### The window (`lib.rs`)

- azul draws the title row (coordinator ruling): the window is `WindowDecorations::NoTitle`, the body a column flex,
  and its first child `Titlebar::create("AzMail").with_background(white).without_border_bottom()`, white like the
  toolbar under it.
- First run (no account): "Add your mail account": Email address, "Password or app password" (a masked
  `TextInput::create_password`), the app-password note ("Gmail and iCloud need an app password, not your normal
  password.") plus the provider's own note, "Sign in with an OAuth access token (XOAUTH2) instead of a password", IMAP
  server and port, SMTP server and port (kept for sending, later), user name, local mail folder, "Unencrypted
  connection (only for a test server on this computer)". Empty fields show the provider's values as placeholders and
  stand for them. Field ids for scripts: `#acct-email`, `#acct-secret`, `#acct-imap-host`, `#acct-imap-port`,
  `#acct-smtp-host`, `#acct-smtp-port`, `#acct-username`, `#acct-folder`.
- "Save and sync" writes `account.json`, `keyring_store`s the secret, keeps it in memory for this run, and syncs.
  The toolbar shows "Syncing Inbox: 120 of 3400 messages" and a `ProgressBar`; "Stop" removes the thread, which
  writes what it has between batches. Later runs read the secret with `keyring_get` when "Sync now" needs it (the
  answer arrives as the `KeyringResult` window event). A refused sign-in reopens the form with the server's words.
- Sidebar: accounts, the synced folders with their counts (from `state.json`); list: from / subject / local date,
  newest first, unread in bold, 300 rows then "Show more" (from `index.jsonl`); view: subject, From / To / Cc / Date,
  attachments, "Plain text" (quoted lines indented behind a bar coloured by level: blue, green, purple, amber) and
  "HTML".
- Stdout for scripts: `AZMAIL_ACCOUNT_SAVED`, `AZMAIL_SYNC_START`, `AZMAIL_SYNC_DONE fetched=<n> reused=<n>
  folders=<n>`, `AZMAIL_SYNC_FAILED <why>`, `AZMAIL_KEYRING <outcome>`. The secret is never printed.

### HTML view (`html.rs`)

The HTML part is sanitized in the app and rendered by azul's own parser (`Xml::from_str` +
`Dom::create_from_parsed_xml`), remote images off: a lenient tokenizer and an allow-list writer. Scripts, styles,
titles, frames, SVG/MathML and form controls (`select`, `textarea`, `button` with its label) go with their content;
`form` and `input` go (azul would make them live widgets); images become a grey `[image: alt]`, tracking pixels (1x1,
hidden) nothing; `href` only http/https/mailto; `style` keeps ~45 properties whose values name no `url()` /
`expression` / `@import` and no negative margin; `bgcolor`/`align`/`valign`/`width` and `font color` become style,
`font` a span, `center` a centred div; HTML's implied ends for `p`, `li`, `td`, `tr`, `dd`; references decoded
(Latin-1 table, the common typographic ones, numeric incl. Windows-1252 128-159) and the text re-escaped; depth 200.

This deviates from the exploration's recommendation (`scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md`, 0132bf47d,
section 2: `ammonia` 4.2 on html5ever): ammonia 4.2.0 is inside the 14-day publish cooldown today (the exploration
says so), its legacy rewrite needs a pre-pass over html5ever's DOM that ammonia does not expose, and it brings about
15 crates plus a build-script policy for markup5ever. The policy is the exploration's 1.4 list, stated as 14 tests;
`html::sanitize` is the one swap point if ammonia comes in later (the tests stay).

## How to log in to your real account

1. Build AzMail and a `libazul` as for AzCalendar (`cargo build --release -p AzMail`; the binary is
   `target/release/AzMail`, or `target/consumer/release/AzMail` when built from `examples/azul-mail`). Start it:
   `target/release/AzMail`. To keep the test away from your real data folder: `AZMAIL_DATA=~/azmail-test AzMail`.
2. "Add your mail account": type your address. On leaving the field, the IMAP / SMTP placeholders fill in for Gmail,
   Outlook, iCloud and Fastmail (any other domain gets `imap.<domain>:993` / `smtp.<domain>:465`); type over them if
   your provider differs.
3. The password field wants what the provider accepts over IMAP:
   - **Gmail**: an app password, not your Google password. Turn on 2-Step Verification, then create one at
     myaccount.google.com/apppasswords (16 letters, shown in groups of four; paste it without the spaces).
   - **iCloud** (`icloud.com`, `me.com`, `mac.com`): an app-specific password from account.apple.com -> Sign-In and
     Security -> App-Specific Passwords. The user name is the full address.
   - **Fastmail**: an app password (Settings -> Privacy & Security -> Manage app passwords, with IMAP access).
   - **Outlook.com / Microsoft 365**: Microsoft switched password (basic) sign-in over IMAP off for most accounts. Tick
     "Sign in with an OAuth access token (XOAUTH2)" and paste an access token with the IMAP scope
     (`https://outlook.office.com/IMAP.AccessAsUser.All`); getting one needs an app registration and a browser flow,
     which AzMail does not have yet (see "What is left"). Tokens expire after about an hour.
   - Any other IMAP server: its password; AzMail uses `AUTHENTICATE PLAIN` when offered, else `LOGIN`.
4. Leave "Unencrypted connection" off (it is refused for anything but this computer anyway). Port 993 is IMAP over
   TLS; STARTTLS on 143 is not supported.
5. "Save and sync". The password goes to the system keyring (on macOS a Keychain generic password, service
   `com.azul.keyring`, account `AzMail/<address>/imap`); `account.json` holds no secret. The toolbar shows the progress;
   the first sync of a big mailbox takes a while (25 messages per fetch). "Stop" is safe at any time.
6. The mail is in `~/Library/Application Support/AzMail/<address>/mail/` (or your `AZMAIL_DATA` / chosen folder):
   `inbox/`, `sent/`, `spam/`, ... Each `.eml` opens in any mail program. "Sync now" later fetches only new mail.
   Nothing on the server changes (read-only `EXAMINE`, `BODY.PEEK[]`).

Gmail note: Gmail's "All Mail" is a real folder (`mail/all`) and holds every message again, as do INBOX and
`[Gmail]/Sent Mail` for theirs; archived mail is only in All Mail, so all of them are synced. A first Gmail sync
therefore stores most messages twice (see "What is left").

## Testing without the real account

- `examples/azul-mail/scripts/imap_server.py` (stdlib only): every directory under `--root` is a mailbox (listed in
  modified UTF-7; Spam/Junk/Sent/Drafts/Trash/Archive get their RFC 6154 attribute), every `*.eml` a message served
  with CRLF. LOGIN, AUTHENTICATE PLAIN / XOAUTH2, LIST, STATUS, SELECT, EXAMINE, (UID) SEARCH, (UID) FETCH, literals;
  `--tls` serves implicit TLS with a P-256 certificate made by `openssl` for 127.0.0.1 (prints `IMAP_SERVER_CA`). A
  later file gets the next UID, `.uidvalidity` renumbers, `.flags.json` sets flags. `--log` writes JSON lines of
  sign-ins, selects, searches and fetches, never a secret. Tested by `test_imap_server.py` against Python's own
  `imaplib`: 16 tests, all pass (0.9 s).
- `examples/azul-mail/scripts/sample_mail/`: a reply with three quote levels, an HTML newsletter (tables, font,
  center, bgcolor, a script, a dark-mode style block, a remote image, a tracking pixel, a negative margin, a
  `javascript:` link, a form), an attachment, a Latin-1 quoted-printable message, a phishing mail in Spam, Sent, a
  non-ASCII folder (`Entwürfe`) and a nested `Work/Projects`.
- `examples/azul-mail/scripts/sync_e2e.py`: the server, AzMail headless (`AZ_BACKEND=headless AZ_DEBUG=<port>
  AZMAIL_DATA=<tmp> AZMAIL_TEST_PASSWORD=<random>`), the account typed in through the debug server (`focus_node` +
  `text_input` on `#acct-email`, `#acct-imap-host`, `#acct-imap-port`, click "Unencrypted connection", click "Save and
  sync"), then: every `.eml` byte for byte, `index.jsonl` / `state.json`, spam in `mail/spam`, the Latin-1 subject
  decoded, the password in no file and no output; the window's folders, a reply's quotes, the newsletter's HTML part
  (no script, no form); "Sync now" again fetches nothing (AzMail's count and the server log); a new file in the
  server's INBOX is fetched alone. `--tls` runs the same over TLS with `AZMAIL_TEST_CA`.

## Where DRIVE1's shared `Drive` goes

`store.rs`, `LocalFolder` (`examples/azul-mail/src/store.rs`). Everything the sync does to storage goes through five
calls, all whole-object operations an S3 bucket has: `put(key, bytes, durable)` (PutObject; the temp+rename is the
local atomicity), `get(key)` (GetObject), `size_of(key)` (HeadObject), `move_prefix(from, to)` (CopyObject +
DeleteObject per key, only on a UIDVALIDITY change) and `folders()` (ListObjectsV2 `mail/` with delimiter, keeping
prefixes that hold `state.json`). The keys are already S3 keys (`mail/<folder>/<yyyy>/<mm>/<uid>.eml`, `/` only,
validated by `is_valid_key`). The swap: make `sync_account` / `sync_folder` take `&dyn Drive` (or a generic) instead
of `&LocalFolder`, and `LocalFolder` becomes DRIVE1's `LocalDrive`; `lib.rs` builds the drive from the account (local
folder, or an S3 prefix). No second storage layer was built. `index.jsonl` is rewritten whole at checkpoints (S3 has
no append), so it maps directly.

Layout next to the exploration's cloud mailbox (section 3.2, `users/<uid>/mail/<folder>/<yyyy>/<mm>/<ULID>.eml` plus
a per-message `.json` sidecar and `_state/`): the folder names and the `<yyyy>/<mm>` partition are the same; an IMAP
account names its files by UID and keeps one `index.jsonl` / `state.json` per folder (this task's spec). Open
question for the S3 step: where an imported IMAP account sits in a user's bucket (`users/<uid>/mail/` is the Azlin
mailbox's own), e.g. `users/<uid>/imap/<account id>/mail/...`.

## api.json changes

None. AzMail uses only existing API: `CallbackInfo::keyring_store / keyring_get / get_keyring_result`,
`WindowEventFilter::KeyringResult`, `Thread::create` + `ThreadWriteBackMsg`, `TextInput::create_password /
create_email / with_on_focus_lost`, `CheckBox`, `ProgressBar`, `Titlebar::create / with_background /
without_border_bottom`, `WindowDecorations::NoTitle`, `Xml::from_str`, `Dom::create_from_parsed_xml`,
`FilePath::get_data_dir`.

## Crates added (all in `examples/azul-mail` only; none in core / css / layout / dll)

| Crate | Version | Why |
|---|---|---|
| `imap` | =3.0.0-alpha.15, `default-features = false` | The one sync Rust IMAP client: `Client::new(stream)` over any `Read + Write`, blocking calls that fit an azul `Thread`. No TLS feature, so neither native-tls/OpenSSL nor rustls-connector (whose default provider is aws-lc-rs) come in; AzMail hands it its own rustls stream. Pinned: the 3.0 line is only published as alphas (last release 2025-02); its parser is maintained. Rejected: `async-imap` (needs an async runtime + async TLS inside a Thread), `imap-codec` / `imap-next` (sans-I/O, 2.0 alpha; a driver to write), a hand-written client (more protocol code to get right than a sync over a mature parser) |
| `imap-proto` | 0.16.7 (via imap `^0.16.1`) | imap's parser (nom 7, already in the tree); named directly for `NameAttribute` |
| `bufstream` | 0.1.4 | imap's buffered stream |
| `mail-parser` | 0.11.9, `full_encoding` | The exploration's pick: MIME, RFC 2047/2231 headers, 41 charsets through `encoding_rs` (already in the tree), text <-> HTML bodies, fuzzed |
| `hashify` | 0.2.9 (via mail-parser `^0.2`) | mail-parser's compile-time lookup tables (a proc macro: read, token-only, pinned in the build-script policy) |

Reused from the tree, no new crypto: `rustls` 0.23 (`std`, `tls12`, `logging`), `rustls-rustcrypto` 0.0.2-alpha,
`webpki-roots` 1.0 - exactly azul-layout's HTTP TLS stack - and `serde`, `serde_json`, `chrono` (`std`, `clock`).
Everything else the new crates pull (base64 0.22, chrono, nom 7, ouroboros 0.18, regex, lazy_static, encoding_rs,
indexmap, syn) is already locked. All five new versions are past the 14-day cooldown. Entries added:
`scripts/dependency-justifications.toml` (an AzMail section), `supply-chain/config.toml` (five `safe-to-deploy`
exemptions), `scripts/supply-chain/build-script-policy.toml` (hashify, digest computed with
`scan_build_scripts.py`'s own `sc_common.digest_files`).

**Cargo.lock is not updated** (no cargo). The parent's first build adds AzMail and the five crates; commit the
lockfile with it (`--locked` fails until then).

## Rendering gaps to expect (the HTML and plain-text views, by reading; not run)

The e2e checks text only; screenshots of the sample newsletter are the parallel engine task's input. From the code
and the exploration's gap table (section 6):

1. **A parsed document nested in the app's DOM**: `Dom::create_from_parsed_xml` returns an `<html><body>` root, which
   AzMail adds as a child of its scrolling pane div. html/body UA styles (100% height, margins) inside a flex column
   may size or scroll wrongly; there is no public "children of the parsed body" API to embed a fragment instead.
2. **Links** (E-XML-2, E10): `href` survives the sanitizer but is not in the XML attribute table, and inline links
   get no hit area: links are text.
3. **Tables** (E-TABLE): the newsletter's `width: 600px` table with `bgcolor` cells; spanning / %-wide cells were
   seen to crash the CPU renderer or lose columns.
4. **UA defaults** (E-UA, E-BR, E-OL): italic `i`/`em`, line-through `s`, monospace `code`/`pre`, blockquote
   margins, link colour, a `<div><br></div>` blank line, list markers.
5. **Dark mode** (E-MODE): mail is shown in the app's colours; a light "card" for designed mail needs per-subtree
   `color-scheme`. The sanitizer already drops `<style>` blocks (and their `prefers-color-scheme` rules).
6. **Images** (E-IMG): blocked by design; "Load images" and `cid:` parts need a resolve hook or
   `change_node_image` after load.
7. **Plain text**: every line is a `div` with `white-space: pre-wrap` and a span; a 3000-line message builds 6000
   nodes (capped). Long unbroken lines (URLs) need `overflow-wrap`.
8. **Message list**: plain divs, 300 rows per page; 10k-message folders want ListView v2 / virtualisation.

## Commits

| Commit | What |
|---|---|
| `5bd9e7278` | RED: the crate, pure logic as tests over `todo!()` (account, auth, mutf7, folders, store, message, html, sync with a fake server) |
| `15cbcc005` | GREEN: the bodies |
| `613f9d491` | imap_client.rs, the window, keyring flow, sync thread, azul-drawn title row |
| `47ab7f44c` / `cb767f967` | RED / GREEN: tracking pixels leave no placeholder, negative margins go |
| `a59d917e9` | RED: the Python IMAP server's 16 tests over a stub |
| `edd4890e7` | GREEN: imap_server.py, sample mail, sync_e2e.py |
| `6f8cc1f6d` / `e0de9f33d` | RED / GREEN: a button goes with its label |
| `6f9b9cf56` | supply chain: justifications, cargo-vet exemptions, hashify's build-script policy |
| `fde871e37` | test fixture: the fake server built folder by folder (plain argument coercions) |
| (this) | report and progress |

75 Rust unit tests (account 15, sync 15, html 12, folders 9, store 8, message 8, auth 4, mutf7 4) and 16 Python tests.

## Least sure to compile (nothing was built)

1. `imap_client.rs`: the `imap` 3.0.0-alpha.15 API as read from its source (`Client::new`, `read_greeting` through
   `Deref`, `capabilities().has_str`, `authenticate` / `login` returning `(Error, Client)`, `examine`, `uid_search`,
   `uid_fetch`, `Fetch::{uid, size, flags, internal_date, body}`, `Name::{name, delimiter, attributes}`); the match on
   `imap_proto::NameAttribute` (non-exhaustive, has a `_` arm); rustls' `builder_with_provider(...)
   .with_safe_default_protocol_versions()?.with_root_certificates(..).with_no_client_auth()`,
   `pki_types::pem::PemObject::from_pem_file`, `ServerName::try_from(String)`.
2. `lib.rs` against the generated link-dynamic API (read from `target/codegen/reexports.rs` /
   `dll_api_external.rs` of the main checkout, which may be a little newer than the base): `azul::error::KeyringResult`
   and `azul::option::{OptionKeyringResult, OptionThreadSendMsg}` paths, `info.keyring_store(String, &str, bool)`,
   `ButtonType::Default`, `Titlebar::create(..).with_background(ColorU)`, the `WriteBackCallback { cb, ctx }` literal,
   `Xml::from_str` + `ResultXmlXmlError`, closures that capture `data: &RefAny` and build `RefAny::new(FieldRef { .. })`.
3. `message.rs`: mail-parser lifetimes (`body_text(&'x self)` on a local `Message`, fine by covariance),
   `MimeHeaders::attachment_name`, `MessagePart::len`; `chrono::DateTime::<Utc>::from_timestamp`.
4. `sync.rs`: the immediately-called `FnMut` closure in `sync_folder` that captures `index`, `state`, `report`,
   `source` and `progress` and is followed by `write_checkpoint` on the same values; `Option::is_none_or` (Rust 1.82;
   CI is 1.91).
5. `html.rs`: `SAME.iter().find(..)` giving `&&'static str`, `strip_prefix(|c: char| ..)`.

Runtime assumptions to watch in the e2e: a `WindowEventFilter::KeyringResult` callback on the body receives the
headless memory keyring's answer; the debug server's `click` on a list row's subject span reaches the row div's
`Click` callback; focus survives (or is re-taken by the script after) the redraw when the address field loses focus.

## Test commands for the parent

```sh
# Python: the IMAP test server (runs now, no build needed)
python3 examples/azul-mail/scripts/test_imap_server.py -v

# Rust unit tests (75): account, auth, mutf7, folders, store, message, html, sync
cargo test -p AzMail --lib

# E2E: build AzMail and a libazul with the debug server (AZ_DEBUG / e2e-server), as for AzCalendar, then
python3 examples/azul-mail/scripts/sync_e2e.py [--bin <path to AzMail>] [--timeout 90] [--keep-logs]
python3 examples/azul-mail/scripts/sync_e2e.py --tls    # the same over implicit TLS
```

By hand against the test server: `AZMAIL_TEST_PASSWORD=pw python3 examples/azul-mail/scripts/imap_server.py --root
examples/azul-mail/scripts/sample_mail` (prints the port), then `AzMail` -> address `ada@example.org`, IMAP
`127.0.0.1` + the port, password `pw`, tick "Unencrypted connection", "Save and sync". (Outside headless the password
goes to the real keyring under `AzMail/ada@example.org/imap`.)

## What is left

1. **SMTP send and compose**: `lettre` (blocking `SmtpTransport`, `rustls-no-provider` + the RustCrypto provider)
   and `mail-builder`, per the exploration; the SMTP host / port are already in `account.json`.
2. **OAuth**: the browser flow for Gmail / Microsoft 365 (app registration, redirect or device code, refresh tokens
   in the keyring). Today XOAUTH2 takes a pasted access token.
3. **S3 export** through DRIVE1's `Drive` (the swap point above), plus the bucket prefix for imported accounts.
4. **Sync depth**: flag changes after the first sync (CONDSTORE / `CHANGEDSINCE`), deletions and moves on the server
   (a UID set diff), IDLE for new mail, STARTTLS on 143, Gmail de-duplication across All Mail / INBOX / Sent
   (`X-GM-MSGID`), a size cap or partial fetch for huge messages.
5. **Reading**: attachments (open / save), threads, search, "Load images" and `cid:` images, clickable links,
   message list virtualisation, the three-pane widgets of `planning/core/mail.md`.
6. **Sanitizer**: move to ammonia when its cooldown allows and a DOM pre-pass is written, keeping `html.rs`'s tests.
7. CI: a `cargo test -p AzMail --lib` step and the Python server tests; commit Cargo.lock after the first build.

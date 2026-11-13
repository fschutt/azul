# DRIVE1_AZDRIVE: AzDrive and the shared S3-first storage crate (2026-09-30)

Branch `wt/drive1-azdrive` from `a7e18f4df`. Nothing was compiled (house rule); the Python server
and its tests were run (23/23 pass). The Rust unit tests, the AzDrive build and the e2e are for the
parent to run (commands below).

## What was built

### 1. `examples/azul-storage` (lib `azul_storage`, workspace member, `publish = false`)

The storage layer AzDrive and AzMail share. Durable data is files; one bucket (or folder) per
user; the DB never holds content (user ruling, `azlin_cloud_storage_split`).

- `Drive` trait, every call blocking (call it from an azul `Thread`):
  `list(&ListRequest) -> ListPage` (S3 ListObjectsV2 semantics: `prefix`, `delimiter`, `max_keys`,
  `continuation`; `ListPage { folders, objects: Vec<ObjectInfo { key, size, modified, etag }>,
  next }`), `get`, `get_range(key, ByteRange)`, `put`, `delete` (missing key = done, as S3),
  `head`. Blanket impls for `Box<D>` and `Arc<D>`.
- `LocalDrive`: a folder. Keys are `/` paths under the root; `..`, `.`, empty segments, absolute
  (`/x`, `\\server`, `C:`), backslash and NUL keys are refused before any file is touched
  (`key::check_path_key`, tested with traversal keys against a sibling file). One folder level per
  `read_dir` with the `/` delimiter, a walk without it; S3 key order; the continuation token is the
  last key; atomic writes through a temporary sibling.
- `S3Drive`: endpoint (http/https, default ports dropped, a base path kept, IPv6), region, bucket
  (DNS rules checked for virtual-host style), path-style or virtual-host style. ListObjectsV2 with
  continuation tokens, GetObject with a signed `Range` (a server that ignores Range still yields the
  range), PutObject (body hash signed), DeleteObject, HeadObject. XML error bodies become
  `DriveError::Service(ServiceError { status, code, message, region, endpoint, .. })` whose text
  reads e.g. `SignatureDoesNotMatch (HTTP 403): The request signature we calculated does not match
  ... (check the secret key)`; `NoSuchKey` becomes `NotFound`. Works for AWS S3, R2 (`region =
  auto`) and MinIO (path style).
- `sigv4`: canonical request, string to sign, signature, `uri_encode`, `canonical_query`. Tested
  with AWS's published vectors (aws-sig-v4-test-suite: get-vanilla, post-vanilla, query order,
  unreserved, utf8, space, header-value-trim; S3 reference examples: GET with Range, PUT
  `test$file.text`, list `max-keys=2&prefix=J`, `?lifecycle`), plus two S3Drive requests whose
  signatures were computed independently with Python's hashlib/hmac.
- `Transport` seam (`HttpCall` / `HttpReply`): `S3Drive` builds and signs the request, a transport
  sends it. Tests use a recording fake; the apps use `azul_transport::AzulTransport` (feature
  `azul`), which goes through azul's `HttpRequestConfig::http_request` (see "HTTP" below).
- `ScopedDrive<D>`: a drive through a grant (key prefix + read-only flag) - the RBAC seam.
- `config`: `DrivesFile` (`<config dir>/azul-storage/drives.json`, env `AZUL_DRIVES`, format
  `azul-storage.drives` v1, NO secrets), `DriveEntry { id, name, location: Local{root} |
  S3{endpoint, region, bucket, path_style, auth} }`, `DriveAuth::{Keyring, AccessLink{link, prefix,
  can_write}}`, `keyring_key(id) = "azul-storage/s3/<id>"`, `new_drive_id`, `DriveEntry::open`.
- `Credentials { access_key_id, secret_access_key, session_token }`: `to_keyring_secret()` /
  `from_keyring_secret()` (one JSON string per drive in the OS keyring). `Debug` of `Credentials`,
  `S3Drive`, `HttpCall` (Authorization / security token) and AzDrive's form never shows a key;
  nothing logs one.
- `transfer`: `download_to_file` (ONE object: one GET, or 8 MiB ranged GETs for a big one, into a
  temporary file renamed at the end), `download_path` (a free name, `0001 (1).eml`), `upload_file`.

### 2. AzDrive, `examples/azul-drive` (package `AzDrive`, lib `azdrive`, bin `AzDrive`, link-dynamic)

- Sidebar: **Home** (a `LocalDrive` on the home folder, `AZDRIVE_HOME`), the added drives from
  drives.json, **Add drive...**, **Remove drive** (drops the entry and the keyring item).
- **Add an S3 drive**: name, endpoint, region (empty = us-east-1; R2: auto), bucket, access key,
  secret key (password field), path-style checkbox. **Test connection** makes ONE ListObjectsV2
  call (max-keys=1) on a Thread and shows "Connection OK ..." or the service's error text.
  **Save drive** writes drives.json (no secrets) and stores the keys with
  `CallbackInfo::keyring_store`; opening the drive later reads them with `keyring_get` (one keyring
  request in flight at a time: `KeyringResult` does not say which request it answers). A drive whose
  keyring entry is gone reopens the form prefilled ("Enter the drive's keys again").
- Main view: Back / Up / Refresh, `Breadcrumb` (drive, then folders), `ListView` (Name, Size,
  Modified; header click sorts, a second click reverses; folders first; row click selects; a
  double-click opens a folder, or fetches and opens a file). Listings are paged by 200 with
  **Load more**; browsing fetches listings only.
- **Download**: ONE object to `AZDRIVE_DOWNLOADS` / the Downloads folder. **Open**: ONE object into
  a temp folder, then `Url::parse(file://...).open()` (azul's open-URL API: `open` / `xdg-open` /
  `start`). **Upload...**: `FileDialog::open_file`, put into the open folder. **Delete**: files,
  after a confirmation.
- Dialogs use the `Dialog` widget (modal, close button, `with_on_close`). `AZDRIVE_DIALOGS=inline`
  renders the same content as a sheet inside the window (for headless scripts, see open question 1).
- Coordinator ruling for all Az apps: the window is `WindowDecorations::NoTitle`, the body's first
  child is `Titlebar::create("AzDrive").with_background(<toolbar colour>).without_border_bottom()`
  (title colour set in dark mode), and the body is a column flex.
- Colours follow the light / dark MODE (`LayoutCallbackInfo::get_mode`, two palettes); the widgets
  follow the app THEME (flat / flora) since AzDrive passes no `with_theme`.
- Every storage call runs on an azul `Thread` and answers through `ThreadWriteBackMsg`; no callback
  waits on the network. Stdout for scripts: `AZDRIVE_LISTED <drive id> <prefix or /> <n>`,
  `AZDRIVE_TESTED ok|error`, `AZDRIVE_ADDED <id>`, `AZDRIVE_DOWNLOADED <path>`,
  `AZDRIVE_UPLOADED <key>`, `AZDRIVE_DELETED <key>`.
- The view logic is `browse.rs` (no azul types), unit-tested.

### 3. Python

- `examples/azul-drive/scripts/s3_server.py`: stdlib-only S3 subset (default backend): objects in
  `<root>/<bucket>/<key>`; ListObjectsV2 (prefix, delimiter, max-keys, continuation tokens that skip
  an already listed folder, start-after, encoding-type=url), GetObject with one Range (206 / 416),
  PutObject (payload hash checked), DeleteObject (204; empty folders pruned), HeadObject, plus
  CreateBucket / HeadBucket / ListBuckets. Every request must carry a valid SigV4 signature (the
  same vectors); errors are S3 XML (SignatureDoesNotMatch with StringToSign / CanonicalRequest,
  InvalidAccessKeyId, AccessDenied, RequestTimeTooSkewed, AuthorizationHeaderMalformed with Region,
  NoSuchKey, NoSuchBucket, InvalidRange, XAmzContentSHA256Mismatch). Path-style and virtual-host
  (Host header) addressing. Keys that climb out of the folder are refused. Every request is logged:
  `server.requests()`, `server.object_gets()`, `--log file.jsonl`. `--backend moto|auto` wraps
  moto's `ThreadedMotoServer` when installed (not installed here, untested).
- `test_s3_server.py`: 23 unittests (vectors, listing / paging, ranges, round trip, auth errors,
  traversal, payload hash, virtual host, request log). All pass.
- `browse.py`: the e2e (below).

## Commits

| commit | what |
|---|---|
| `6f250c31f` | test(storage): the shared Drive crate states its behaviour (RED) |
| `7d6900a74` | feat(storage): LocalDrive, S3Drive with SigV4, the drives file, ScopedDrive |
| `b7e22683f` | test(azdrive): what the local S3 test server must do (RED) |
| `39966255c` | feat(azdrive): a local S3 test server in Python, stdlib only |
| `12ff29be3` | test(azdrive): what AzDrive's window shows, as plain data (RED) |
| `f8a080f84` | feat(azdrive): AzDrive, an Explorer-like browser of Home and S3 drives |
| `5ba35f6cd` | test(azdrive): browse.py, AzDrive end to end against the local S3 |
| (this) | docs: this report, the progress file, a local rename in AzDrive's layout |

## What AzMail should call to export to a drive

```rust
use azul_storage::{azul_transport::AzulTransport, config, Credentials, Drive, ListRequest};

// UI thread: which drive (the same drives.json AzDrive writes).
let file = config::DrivesFile::load(&config::drives_file(
    std::env::var(config::DRIVES_VAR).ok().as_deref(), config_dir)?)?;   // Option -> handle None
let entry = file.drives.iter().find(|d| d.name == "S3 Drive").cloned();
// Its keys: info.keyring_get(config::keyring_key(&entry.id)); on the KeyringResult window event:
// KeyringResult::Retrieved(s) -> Credentials::from_keyring_secret(s.as_str())?

// On an azul Thread (never in a callback): one object per message.
let drive = entry.open(Some(credentials), Box::new(AzulTransport::new("AzMail/0.1")))?;
drive.put("mail/INBOX/<uidvalidity>-<uid>.eml", &rfc822_bytes)?;
// Incremental: page through drive.list(&ListRequest::recursive("mail/INBOX/")) (or head(key))
// and skip keys already there. "Spam" is just the folder mail/Spam/.
// Local export first: azul_storage::LocalDrive::new(folder) takes the same keys.
```

Proposed layout: `mail/<mailbox>/<uidvalidity>-<uid>.eml` (per account:
`mail/<account>/<mailbox>/...` once there are several). AzDrive shows it immediately (Home or the
S3 drive), which is the "debug the architecture by browsing files" point of the ruling.

## The RBAC / access-link seam

- `config::DriveAuth::AccessLink { link, prefix, can_write }` is a drive entry the DB's access-link
  table grants ("who may view or edit which S3 path"). It round-trips through drives.json today.
- `DriveEntry::open` returns `DriveError::Unsupported` for it. The later task: resolve `link` at the
  server to short-lived credentials (STS style) or a presigning endpoint for exactly `prefix`, then
  return `ScopedDrive::new(S3Drive::new(..), prefix, can_write)`. `ScopedDrive` already enforces the
  prefix (keys shown relative to it, `..` refused) and read-only (`put` / `delete` -> `Denied`),
  whatever the backend or the server does.
- AzDrive shows such a drive in the sidebar; opening it shows the Unsupported text.

## api.json changes

**None.** Everything used exists at `a7e18f4df` (checked with a script against this worktree's
api.json: `http_request`, `keyring_*`, `get_keyring_result`, `WindowEventFilter::KeyringResult`,
`ListView`, `Dialog`, `Breadcrumb`, `TextInput::create_password`, `Titlebar`,
`ThreadWriteBackMsg::create`, `FilePath::get_*_dir`, `Url::open`, ...).

HTTP: `http_request` CAN send a signed PUT with a body and custom headers (4xx/5xx come back as
`Ok` with their XML body, response headers included), so nothing was missing for S3. What it is
not is blocking: it resumes a `ResumeCallback` on the UI thread. `AzulTransport::send` therefore
calls it on the worker Thread and waits (with a timeout) for the resume callback to hand the answer
back through a channel: one UI pump (~16 ms thread-poll tick) per request.

Optional follow-up (not needed, would remove that latency; a design decision because the Rust doc
says the blocking variants are deliberately not public, as they cannot exist on web):

```json
"http_request_blocking": {
  "doc": ["The synchronous transport behind http_request, for a Thread body only (never a",
          "callback). On web it returns HttpError::Other."],
  "fn_args": [{"self": "ref"}, {"method": "HttpMethod"}, {"url": "String"}, {"body": "U8Vec"},
              {"content_type": "String"}],
  "returns": {"type": "ResultHttpResponseHttpError"},
  "fn_body": "object.http_request_blocking(method, url, body, content_type)"
}
```
on `HttpRequestConfig` (the Rust method exists in `layout/src/http.rs`). `AzulTransport::send` would
then be three lines.

## Supply chain

No new crate. `sha2 0.10.9` and `hmac 0.12.1` are already in Cargo.lock through
`rustls-rustcrypto` (the TLS stack) and `azul-layout`; `roxmltree 0.21.1` through azul-layout's
`xml` feature; `serde`, `serde_json`, `chrono` through azul-layout / AzCalendar. They are already in
`scripts/dependency-justifications.toml` (whose CI gate only covers css/core/layout/dll anyway) and
covered by `supply-chain/` (imports / exemptions). Both new crates are `publish = false`
workspace members. Cargo.lock got the two new package entries by hand (`AzDrive`,
`azul-storage`); cargo will re-sort them if needed.

## Least sure to compile

1. `azul_storage::azul_transport` (only built with feature `azul`, i.e. by AzDrive): the generated
   `HttpGetResult::downcast(..).into_option()`, `answer.result.into_result()`,
   `response.headers.as_slice()` and `http_request(method, &str, U8Vec, &str, RefAny, cb)`
   generics. Mirrors AzCalendar / AzMeet usage.
2. AzDrive `lib.rs`: `ThreadWriteBackMsg::create(on_job_done, RefAny)` + `sender.send(..)`;
   `info.get_keyring_result().into_option()` / `KeyringResult::Retrieved(AzString)` from
   `azul::error`; `ListViewRow { cells, height }` literal; `Url::parse(..).into_result()`;
   `window.create_callback = Some(Callback::create(startup)).into()`.
3. Borrow checker in AzDrive callbacks that hold the `DriveState` guard while spawning threads or
   issuing keyring requests (`with_state`, `on_save_drive`, `on_test_connection`), and field-disjoint
   borrows of `s.popup` with `s.drives_file`.
4. `sigv4::hmac_sha256`: `<Hmac<Sha256> as Mac>::new_from_slice` (hmac 0.12 / digest 0.10).
5. `xml.rs`: `roxmltree` 0.21 `Node<'_, '_>` helper and `Document::parse` Display with
   `default-features = false`.
6. The `Drive` blanket impls for `Box<D>` / `Arc<D>` and method calls on `Arc<dyn Drive>`.

## Test commands (for the parent)

```sh
# The storage crate: SigV4 vectors, keys, LocalDrive, S3Drive over a fake, XML, config, scope,
# transfer. No libazul needed (feature `azul` off).
cargo test --release -p azul-storage

# The Python S3 server (stdlib only).
python3 examples/azul-drive/scripts/test_s3_server.py

# AzDrive: build (link-dynamic) and its view-model tests (browse.rs).
AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzDrive
AZ_LINK_PATH=$PWD/target/azul-lib DYLD_LIBRARY_PATH=$PWD/target/azul-lib \
  cargo test --release -p AzDrive --lib

# The e2e (libazul with the debug server, as for the AzCalendar / AzMeet scripts).
python3 examples/azul-drive/scripts/browse.py --bin target/release/AzDrive [--keep-logs]
#   default: the form as an in-window sheet (AZDRIVE_DIALOGS=inline)
#   --window-dialogs: the modal Dialog window, addressed through list_doms / dom_id

# Manual: a local S3 and the app.
python3 examples/azul-drive/scripts/s3_server.py --root /tmp/s3 --bucket azdrive --port 9000
#   Add drive: endpoint http://127.0.0.1:9000, region us-east-1, bucket azdrive,
#   access key azdrive-test, secret key azdrive-test-secret, path style on.
```

## Open questions / what is left

1. **Dialog windows under the headless debug server.** A `Dialog` is a `<transient-window>`, a
   real child window. Only the root window gets a debug timer, and a popup's DOM is only reachable
   through the root window's `dom_id` envelope; whether `click` / `text_input` there reach the
   popup's callbacks is unproven. So `browse.py` drives the in-window sheet by default
   (`AZDRIVE_DIALOGS=inline`, same content function), and `--window-dialogs` is the experiment.
2. The e2e and the Rust tests were not run (no compiling). The Python server was exercised with
   requests shaped exactly like `S3Drive`'s (path style, canonical query in the URL, the same signed
   headers) through urllib: list, get, ranged get, put, head, delete all answered correctly.
3. `ListView::on_lazy_load_scroll` is not wired inside the widget, so paging is a "Load more"
   button (200 per page) rather than scroll-triggered.
4. Not built: folder delete / rename / new folder, drag-and-drop upload, progress and cancel for
   big transfers, multipart upload (> 5 GB), presigned URLs, STS credentials, an "Edit drive" for
   endpoint changes (Remove + Add works), moto backend verification.
5. Other storage layers (asked for in the ruling as a report only): Apache OpenDAL (Rust, one
   `Operator` over S3 / GCS / Azure / WebDAV / local; would replace `S3Drive` + `LocalDrive` but
   brings reqwest / tokio and its own signer, i.e. an async runtime and many new crates) and fsspec
   (Python, the same idea for the Python side: `s3fs`, `gcsfs`). The `Drive` trait is deliberately
   the small common subset of both, so an `OpenDalDrive` adapter could be added behind the same
   trait later without touching AzDrive or AzMail. Designed S3-first as asked.

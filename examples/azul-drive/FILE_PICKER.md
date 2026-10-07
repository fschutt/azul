# AzDrive as the file picker of azul apps (design)

Status: design only (2026-10-07). Nothing here is implemented yet; the phases at the end say
what lands where and in which order.

Product decision (user, 2026-10-07): an azul app's file picker PREFERS AzDrive when AzDrive is
installed and falls back to the platform's dialog otherwise. AzDrive's picker is where the
azcloud integration lives: the user's cloud drives (S3 / R2 / MinIO / the Azlin cloud), their
pins and recent places, cloud file handles and access links. The native dialog knows none of
that.

## 1. What an app sees

Nothing changes for an app that calls the existing API:

```rust
FileDialog::open_file(title, default_path, filters, data, on_result)      // FileOpenResult
FileDialog::open_multiple_files(title, default_path, filters, data, cb)  // FileOpenMultiResult
FileDialog::open_directory(title, default_path, data, cb)                // FileOpenResult
FileDialog::save_file(title, suggested_name, data, cb)                   // SaveTargetResult
```

These keep their result types: a PATH. When the user picks a cloud object in AzDrive's
picker for one of these calls, AzDrive fetches it into its cache first and answers the cached
copy's path (a snapshot, see 5.3). `save_file` offers local folders only (a path is what the
app writes to); cloud saves need the new API.

A cloud-aware app (AzWriter, AzSheets, AzMail's attachments, AzPhoto) uses ONE new request:

```rust
FileDialog::pick(options: FilePickerOptions, data: RefAny, on_result: ResumeCallback) -> RequestId
// resumes with FilePickResult (downcast like FileOpenResult)

FilePickerOptions {
    mode: FilePickerMode,          // Open | OpenMultiple | OpenDirectory | Save
    title: AzString,
    default_location: OptionString, // a path, or "azdrive://<drive id>/<prefix>"
    suggested_name: OptionString,   // Save
    filters: OptionFileTypeList,
    accept: FilePickerAccept,       // LocalOnly | LocalAndCloud (default) | CloudOnly
    cloud: CloudPickMode,           // Handle (no download) | Copy (fetch to the cache) | Both
    want_link: bool,                // mint an access link for the picked object(s), see 6
}

FilePickResult {
    status: FilePickStatus,         // Selected | Cancelled | Error(message)
    items: PickedItemVec,
    provider: AzString,             // "AzDrive" or "native": who answered
}

PickedItem {
    path: OptionFilePath,           // a local file, or the cached copy of a cloud object
    cloud: OptionCloudFileRef,      // set when the user picked a cloud object
}

CloudFileRef {
    drive_id: AzString,             // the id in drives.json (stable across devices, see 6)
    drive_name: AzString,           // "Photos (R2)"
    key: AzString,                  // "2026/beach.jpg" (a folder ends in '/')
    size: OptionU64,
    etag: OptionString,
    modified: OptionU64,            // unix seconds
    link: OptionString,             // the access link, when asked for and allowed
}
```

For a Save in a cloud folder the result is a `CloudFileRef` of the TARGET (drive, folder, name)
plus a local staging `path`; the app writes the staging file and calls
`FileDialog::commit_save(item, data, on_result)`, which hands the staging file to the
provider for the upload (the transfer queue, with its conflict question) and resumes with the
uploaded object's `CloudFileRef`. With the native fallback `commit_save` is a no-op that answers
the path that was written.

How the app opens a `CloudFileRef` later: through `azul-storage` (the same drives file and OS
keyring AzDrive and AzMail use: `DrivesFile::load`, `Credentials::from_keyring_secret`,
`Drive::get`), or, without keys (another user, a sandboxed build), through `link`.

## 2. Which picker answers (the preference order)

`FileDialog` (engine, `layout/src/desktop/dialogs.rs`) decides per request:

1. the app's own override: `AppConfig::file_picker = Native | PreferProvider` (default
   `PreferProvider`), and a per-request `FilePickerOptions::force_native`;
2. the environment: `AZUL_FILE_PICKER=native` (tests, kiosks), `AZUL_FILE_PICKER=<path to a
   registration file>` (a provider under test);
3. the registered provider (below) when its registration is valid and its executable exists;
4. the platform dialog (tfd on the desktop, the registered `FilePickerBackend` on iOS / Android,
   the browser's picker on the web).

The e2e mock store (`request::mock::take_file_open`) stays FIRST, before all of these: a test
never starts a real picker of either kind.

A sandboxed macOS build (App Store) always uses the native panel: only the Powerbox grants it
access to a file the user picked; a path printed by another process is not readable to it.

## 3. "AzDrive is installed": the registration

AzDrive writes ONE per-user file on every start (and removes it on uninstall), next to the
drives file both AzDrive and AzMail already share (`azul_storage::config`):

`<config dir>/azul-storage/file-picker.json`

```json
{
  "format": "azul-storage.file-picker",
  "version": 1,
  "provider": "AzDrive",
  "exec": "/Applications/AzDrive.app/Contents/MacOS/AzDrive",
  "args": ["--pick"],
  "protocol": 1,
  "modes": ["open", "open-multiple", "open-directory", "save"],
  "cloud": true
}
```

- Per user, so another account cannot point this user's apps at its binary.
- The engine only runs `exec` when it is an absolute path to an existing file that is not
  writable by group or others (and owned by the user or root on Unix); it is started with
  `std::process::Command` and an argument list, never through a shell.
- `protocol` is the request / answer format of 4; an engine that does not speak a provider's
  protocol version falls back to the native dialog (and logs why once).
- A stale file (AzDrive moved or deleted) is the same as none: the engine falls back.

Why a file and not only the OS's handler registry: it is the same on macOS, Windows and Linux,
it needs no installer privileges, an app can check it without starting anything, and tests can
swap it (`AZUL_FILE_PICKER=<file>`). The OS registry is used for what it is good at, starting
AzDrive by URL: AzDrive also registers the `azdrive:` URL scheme (`CFBundleURLTypes` in
Info.plist, `HKCU\Software\Classes\azdrive` on Windows, `x-scheme-handler/azdrive` in its
`.desktop` file on Linux) so a link `azdrive://<drive id>/<key>` opens the item in AzDrive
(the share links of 6 point there for a user who has AzDrive).

## 4. The request and the answer (protocol 1)

One child process per request, like one native panel per request:

```
<exec> --pick            (the args of the registration)
stdin:  one JSON line, the request
stdout: AzDrive's other lines (its AZDRIVE_* script markers) and exactly ONE answer line,
        prefixed "AZPICK "; then the process exits
```

Request:

```json
{"v":1, "op":"open", "multiple":false, "title":"Insert picture",
 "default_location":"/Users/me/Pictures",
 "filters":[{"name":"Pictures","patterns":["*.png","*.jpg"]}],
 "accept":"local-and-cloud", "cloud":"copy", "want_link":false,
 "app":"AzWriter", "parent":{"x":120,"y":80,"width":1200,"height":760}}
```

Answers:

```
AZPICK {"v":1,"status":"selected","items":[{"path":"/Users/me/Pictures/a.png"}]}
AZPICK {"v":1,"status":"selected","items":[{"path":"<cache>/AzDrive/picked/photos/2026/beach.jpg",
        "cloud":{"drive_id":"photos","drive_name":"Photos (R2)","key":"2026/beach.jpg",
                 "size":182733,"etag":"\"9b2c\"","modified":1759800000,"link":null}}]}
AZPICK {"v":1,"status":"cancelled"}
AZPICK {"v":1,"status":"error","message":"The keys of \"Photos (R2)\" are not in the keyring."}
```

- stdin / stdout of a child the requester started: no listening socket, no port, nothing
  another process can connect to. The request carries no secret; the answer carries no secret
  (keys stay in the keyring; a `link` is a capability the user asked for, see 6).
- `parent` is the requester's window frame: the picker opens centred over it, always on top
  of it while it runs (a window of another process cannot be a true child window on macOS;
  on Windows the HWND is passed as well and used as the owner).
- Engine side: the child runs on a worker thread; `FileDialog::*` returns
  `request::defer(data, on_result, poll)` (the machinery the mobile pickers use), so the UI
  thread never blocks; `poll` turns the answer line into the result the API promises.
- Fallback rules: the process fails to start, exits without an `AZPICK` line (a crash), or
  answers a protocol it does not know -> the NATIVE dialog opens for the same request (and the
  reason is logged). `cancelled` is final (no second dialog). `error` resumes a `pick` request
  with `FilePickStatus::Error(message)` (the app shows it); for the path-only API, which has no
  way to carry an error, it opens the native dialog instead.
- Cancel from the requester (the request is dropped, the window closes): the engine kills the
  child.

## 5. The picker in AzDrive (`--pick`)

### 5.1 The window

The same Explorer window (the navigation pane with Quick access, This PC, Network; the
navigation row with the breadcrumb and the search box; Details or Large icons), smaller
(900 x 600), titled with the request's title, plus Explorer's picker strip at the bottom:

```
File name: [ beach.jpg                    ]  [ Pictures (*.png, *.jpg) v ]
                                              [  Open  ]  [ Cancel ]
```

- the type filter hides what does not match (folders always show);
- Open on a folder opens it; on a file (or Enter, a double-click) answers it; OpenMultiple
  takes the selection; OpenDirectory answers the open folder ("Select folder");
- Save: the file name field and the folder shown are the target; a name taken asks
  "Replace?" (the conflict question of the transfer queue);
- the command bar shows New folder and the view switches only (no Cut / Paste / Delete);
- `accept: local-only` hides the cloud drives (Network and the cloud tiles of This PC);
  `cloud-only` hides the local drives;
- Escape or the window's close button answers `cancelled`.

In code: `DriveState` gets `picker: Option<PickerRequest>` (read from stdin at start, before
the window opens - no callback waits); `ui_commands` and `ui_view` read it; the answer is one
`println!("AZPICK {}", serde_json::to_string(&answer))` and `info.close_window()`.

### 5.2 Cloud objects

- `cloud: handle` answers at once (no download): the `CloudFileRef` from the listing (key,
  size, ETag, modified).
- `cloud: copy` downloads the object into `<cache dir>/AzDrive/picked/<drive id>/<key>` through
  the transfer queue (the progress dialog for a long one; Cancel answers `cancelled`), then
  answers the path and the ref. A cached copy whose ETag matches is not fetched again.
- OpenDirectory on a cloud folder answers the ref of the folder (its `path` is unset unless
  `copy`, which is refused for folders over a size limit).

### 5.3 What a cached copy is

A SNAPSHOT: an app that writes it changes the cache, not the bucket. A later phase may offer
"keep in sync" (AzDrive watches the copy while it runs and uploads a change, OneDrive's "always
keep on this device"); until then a cloud-aware app saves back with `pick(Save)` +
`commit_save`, or through `azul-storage`.

## 6. The azcloud integration

The point of preferring AzDrive. The cloud architecture ruling (durable data = files in S3 per
user; the database holds only minting, transient state and ACCESS LINKS: who may view or edit
which S3 path):

- **The user's cloud drives appear in every app's picker.** Signing in with the azcloud kit
  writes the account's drives into `drives.json` (ids stable across devices, no secrets) and
  their keys into the keyring - AzDrive lists them under This PC and Network. Pins of Quick
  access and the recent places come from AzDrive's `drive/view.json` in the Azlin data tree,
  which itself syncs, so the same pins show on every device.
- **Handles instead of copies.** A `CloudFileRef` (drive id + key + ETag) is what a document
  can store ("this sheet's picture is photos:2026/beach.jpg") and open on another device of the
  same account without anyone copying bytes around.
- **Access links.** `want_link: true` makes the picker mint an access link for the picked
  object through the azcloud service (a row in the access table: path, who - "anyone with the
  link" or listed accounts -, view or edit, expiry), after the user confirms it in the picker
  ("Anyone with the link can view beach.jpg until 2026-11-07"). The answer's `link` is that
  URL; for a recipient with AzDrive it opens `azdrive://...`. AzMail's "attach as link" and
  AzWriter's "insert from the cloud" are the first users.
- **Who may pick what** stays the bucket's business: the picker lists what the user's keys
  (or the account's access links) can read; the requesting app never sees a key.

## 7. Beyond azul apps (later)

Preferring AzDrive works for azul apps only. For every other app, the OS-native route makes the
azcloud drives appear INSIDE the system's own picker:

- macOS / iOS: a File Provider extension (`NSFileProviderReplicatedExtension`) in the AzDrive
  bundle: the drives show in Finder's sidebar and every Open panel; files download on demand;
- Windows: a Cloud Files API sync root (`cfapi`, the OneDrive mechanism): the drives show in
  Explorer's navigation pane and every common file dialog, placeholders hydrate on open;
- Android: a `DocumentsProvider`: the drives show in the system picker (`ACTION_OPEN_DOCUMENT`);
- Linux: a FUSE mount (or a GVfs backend) under the user's drives, plus the
  `xdg-desktop-portal` FileChooser backend for sandboxed (Flatpak) apps.

These are big platform projects each; the provider protocol above is small, works for every
azul app on every desktop now, and the engine's fallback keeps the native dialog for
everything else.

## 8. Phases

1. **AzDrive `--pick` and the registration** (app only, no engine change): `file-picker.json`
   written on start (`azul_storage::config::picker_file()` beside `drives_file()`), the picker
   strip, the `AZPICK` answer; a script (`scripts/azdrive_pick_e2e.py`) feeds a request on
   stdin, drives the window through the debug server and asserts the answer line, for a local
   file, a cloud object (handle and copy, against `examples/azul-drive/scripts/s3_server.py`)
   and Cancel.
2. **Engine: prefer the provider** (central session, RED first): in
   `layout/src/desktop/dialogs.rs`, the four `FileDialog` requests try the registered provider
   (2 / 3) on a worker thread through `request::defer`, and fall back to tfd. RED tests: a fake
   provider (a 5-line script) answering `selected` resumes `FileOpenResult` with its path; one
   that exits 1 without an answer opens the native dialog (the mock store records it); a
   registration whose `exec` is group-writable, missing or of protocol 2 is ignored;
   `AZUL_FILE_PICKER=native` never starts it.
3. **The cloud-aware API**: `FileDialog::pick`, `commit_save` and the types of 1 in api.json
   (bindings generated; the doc's examples in Rust, C and Python); AzWriter's Insert picture and
   AzMail's attachments adopt it.
4. **Access links**: the azcloud service's access table and the picker's "share" confirmation
   (`want_link`).
5. **OS-native providers** (7), one platform at a time.

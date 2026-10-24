# AzDrive's E2E commands

AzDrive registers its own E2E ops with the engine (`AppConfig::add_e2e_command`). A scenario
names them like any engine op, in an `AZ_E2E=<file|dir>` run or over the debug server
(`AZ_DEBUG=<port>`, `POST /` with the op as JSON). They drive AzDrive's real folder sync: the
same pairing, passes, problem list and conflict answers the window uses, against a "remote"
that is a folder on this computer behind a fault layer. They exist so every sync bug can be
reproduced as one `*.json` scenario.

The commands are registered only when `AZ_E2E` or `AZ_DEBUG` is set. They never touch the
user's own drives: every path they create lies in the run's sandbox.

## How to run

```sh
# every scenario in e2e/sync (each file runs in a process of its own), or one file or folder
examples/azul-drive/scripts/e2e_sync.sh [examples/azul-drive/e2e/sync/<id>.json] [path/to/AzDrive]
```

The script runs AzDrive headless with `AZ_E2E` and points everything it reads and writes at one
temporary folder (the data root, the drives file, Home, the caches, the sandboxes), with
`AZDRIVE_SYNC_POLL=3600` (a scenario starts its passes itself) and `AZDRIVE_SYNC_SETTLE_MS=0`.
libazul must be built with the debug server, as for every `AZ_E2E` run. The runner passes no
command line to its per-file processes, so all of it is environment. `AZDRIVE_E2E_WORK` sets
the sandboxes' parent folder; `AZDRIVE_E2E_KEEP=1` keeps them.

## The engine side (every app gets these)

- **An unknown op goes to the app.** `{"op": "drive_sync", "device": "a"}` is dispatched to the
  app's command named `drive_sync` with the step's other fields as its arguments (a JSON
  object). An engine op of the same name wins. A name no command answers fails the step.
  `{"op": "custom_op", "name": "drive_sync", "args": {...}}` is the long spelling.
- **`"success": false` fails the step.** Every command answers a JSON object with a
  `success` boolean. `false` fails the step with the answer's `error` as the message. The
  whole answer is the step's response either way.
- **`"pending": true` runs the step again.** A command that waits for work on another thread
  (a sync pass) answers `{"success": true, "pending": true}`. The runner yields one turn of the
  event loop and calls it again with the same arguments plus `"_attempt": <n>` (0 on the first
  call). The step fails after `timeout_ms` (a field of the step, default 60000) of wall clock.
  Over the debug server the caller re-sends the op itself.
- **`assert_response` reads a path.** After a step, `{"op": "assert_response", "path":
  "result.files.a.txt.size", "equals": 3}` checks one value of the previous step's response.
  The command's answer is under `result`. Comparisons: `equals` (any JSON), `not_equals`,
  `at_least`, `at_most` (numbers), `len` (the length of an array, object or string),
  `contains` / `not_contains` (a substring of the value's JSON text). A path segment is an
  object key or an array index; a key holding a dot is written `["a.txt"]`
  (`result.files["a.txt"].size`). A missing path fails.
- **`list_app_ops`** answers every command the app registered: name, summary, description,
  arguments, examples (the `E2eOpDef` shape).

## Words

- A **remote** is a folder in the sandbox (`<run>/remote/<name>`) that plays the cloud. Every
  device opens it through its own fault layer, so a partition can cut one device off.
- A **device** is one AzDrive drive (a source-list entry with the id `e2e-<device>`) on a
  remote, paired with its own folder (`<run>/devices/<device>/root`). Several devices on one
  remote are several computers syncing one drive. Each has its own device name (its conflict
  copies say it), state folder, clock offset, disk budget and crash point.
- **Paths** in arguments are relative to the device's folder unless absolute. Every string
  argument may use `{run}`, `{outside}`, `{secret}`, `{root:<device>}`, `{state:<device>}` and
  `{remote:<remote>}`.
- The **outside** folder (`<run>/outside`) is a fake home outside every root:
  `.ssh/authorized_keys`, `Library/LaunchAgents/`, `secret.txt` (holding `{secret}`),
  `target.txt`, `victim/`. `drive_check` `outside_untouched` compares it with its state at
  `drive_harness`.
- **Crash and pause points** are the places the sync engine names in a pass, in the order a
  pass meets them: `start`, `after-scan`, `before-upload`, `mid-read`, `after-blob`,
  `after-commit`, `download-start`, `before-rename`, `after-rename`, `before-trash`,
  `conflict-start`, `conflict-renamed`, `before-base-save`. An E2E device's sessions hear them
  through `SyncSession::with_points`; the commands arm them in `azcloud_kit::sync::faults`. A
  crash stops the pass thread there by unwinding it: nothing after the point runs, no error handler
  runs, only destructors (so a temp file a guard owns is removed, unlike after `kill -9`; the
  sync-chaos `engine` adapter covers a real `kill -9`). The device's in-memory sync state is
  dropped and read again from its state folder, as a restarted app would. A pause blocks the
  pass thread there until `drive_cont`.

Every answer below is the `result` object; `success` is left out where it is simply `true`.

## The sandbox

| op | arguments | result |
|---|---|---|
| `drive_harness` | `work` (folder, default `$AZDRIVE_E2E_WORK` or the temp folder), `keep` (bool) | `run`, `outside`, `secret` |
| `drive_cleanup` | - | `removed`, `run` |

Makes the run's sandbox: `outside/`, `remote/`, `devices/`, `cache/` (AzDrive's cache folder for
this run: the pairings' state folders). It adds a local drive "e2e" whose root is the sandbox,
so pairing a folder in it is allowed. Any other command calls it first with the defaults when
no scenario did. Every scenario ends with `drive_cleanup`: it stops the devices' passes and
removes the sandbox (unless `keep`), since the process does not tell the commands it ends.

## Remotes and devices

| op | arguments | result |
|---|---|---|
| `drive_create` | `remote` (name), `encrypted` (bool, default true), `prefix` (default `""`) | `remote`, `path`, `encrypted` |
| `drive_open` | `device`, `remote`, `device_name` (default the device), `settle_ms` (default 0), `clock` (seconds the sync clock is off, default 0) | `device`, `drive_id` |
| `drive_pair` | `device`, `folder` (default `{root:<device>}`), `prefix` (default the remote's) | `folder`, `state_dir` |
| `drive_unpair` | `device` | - |
| `drive_restart` | `device` | - |

`drive_create` makes a new remote; an existing name fails. `encrypted: true` makes it as AzDrive
makes a new Azlin drive: the drive key and the recovery wrap, compressed and encrypted objects.
The key lives in a keyring in memory that every device of this remote shares (the devices are
"already joined"). `encrypted: false` is the plain index and blobs (an older plaintext drive).

`drive_pair` is AzDrive's own pairing (Share > Sync with a folder): it creates the folder and
starts the first pass (wait for it with `drive_wait_idle`). A folder inside another pairing's
folder fails, as in the window. `drive_restart` drops the device's in-memory sync state and
reads it again from its state folder, like quitting and starting AzDrive.

## Passes

| op | arguments | result |
|---|---|---|
| `drive_sync` | `device`, `expect` (`ok` default, `fail`, `crash`, `any`), `crash_at`, `pause_at`, `timeout_ms` | `outcome` (`done`, `failed`, `crashed`, `paused`), `error`, `up`, `down`, `deleted`, `conflicts`, `cloud_only`, `held`, `newer_format`, `problems` (as `drive_problems`) |
| `drive_sync_bg` | `device`, `pause_at` | `started` |
| `drive_cont` | `device` | - |
| `drive_wait` | `device`, `expect`, `timeout_ms` | as `drive_sync` |
| `drive_wait_idle` | `device` (absent: every device), `timeout_ms` | `passes` (passes ended while it waited) |
| `drive_status` | `device` | `idle`, `running`, `queued`, `passes`, `crashes`, `status` (the status line's key), `last` (the last pass's result, as `drive_sync`) |
| `drive_answer` | `device`, `question` (`mass_delete`, `burst`), `delete_too` (bool, `mass_delete` only) | - |
| `drive_resolve` | `device`, `key`, `choice` (`mine`, `theirs`, `both`) | - |

AzDrive holds a file changed on both sides for the user (the window asks keep mine / take
theirs / keep both) instead of making a conflict copy by itself: the pass counts it in
`conflicts`, `drive_problems` lists it as `conflict-held`, and `drive_resolve` answers it the way the
dialog does (`both` makes the conflict copy on the next pass).

`drive_sync` asks AzDrive for a pass (the same request as Sync now), then answers `pending`
until that pass ended: a pass already running finishes first and the asked one follows it.
`success` is whether the outcome matches `expect`. With `crash_at` the pass crashes at that
point (expect `crash`). With `pause_at` the answer comes once the pass is paused there
(`outcome: "paused"`): steps run while it waits, `drive_cont` lets it go on, `drive_wait`
answers when it ended. `drive_sync_bg` starts a pass and answers at once.

## What a device knows

| op | arguments | result |
|---|---|---|
| `drive_problems` | `device` | `problems`: the engine's list, `[{code, key, detail, severity, since}]` |
| `drive_states` | `device` | `files`: `{key: {state, size, cloud_only, pinned, error}}`, `read_only`, `last_problem` |
| `drive_base` | `device` | `entries`: `{key: {hash, size, mtime}}`, `generation` |
| `drive_index` | `device` or `remote` | `generation`, `features`, `files`: `{key: {hash, size, mtime, device}}`, `deleted`: `{key: {hash, at, device}}` |
| `drive_counters` | `device` (absent: the remote's total) | `ops`: `{get, put, put_if, head, list, delete}`, `bytes_up`, `bytes_down`, `refused`, `puts`: `{key: n}`, `passes`, `crashes` |
| `drive_list` | `device` | `files`: `{path: {kind, size, hash, text, mode, mtime_ns}}` (the folder as it is, the engine's bookkeeping left out; `hash` is the BLAKE3) |
| `drive_read` | `path` (+ `device`) | `exists`, `kind` (`file`, `dir`, `link`, `special`), `size`, `text` (when UTF-8), `hash`, `mode`, `mtime_ns`, `target` (a link's) |

`drive_problems` is the pairing's problem list exactly as the engine published it
(`SyncSession::problems`, `problems.json` in the state folder), with the engine's codes:
`symlink-skipped`, `special-file`, `bad-name`, `bad-remote-name`, `invalid-here`,
`portability`, `display-risk`, `name-collision`, `type-conflict`, `through-link`,
`link-refused`, `mount-point`, `hardlink`, `unreadable`, `changed-during-sync`, `settling`,
`save-in-progress`, `locked`, `too-large`, `disk-reserve`, `disk-full`, `quota-exceeded`,
`transient`, `rate-limited`, `remote-offline`, `sign-in-needed`, `update-needed`,
`remote-entry-unreadable`, `remote-index-unreadable`, `remote-rollback`, `server-restored`,
`hash-mismatch`, `decompression-bomb`, `mass-delete-held`, `loop-frozen`,
`deleted-elsewhere-kept`, `root-missing`, `root-replaced`, `state-reset`, `ignore-change`,
`renamed-to-invalid`, `busy`, `io-error`, `conflict-held`, `uploads-paused`, `held-back`,
`other`. A key is the file's key as the engine keeps it (escaped only for display).

`drive_index` with a `device` reads a plain remote's index through that device's fault layer;
with a `remote` (or neither) it reads past every fault. An encrypted remote has no plain index:
there `drive_index` answers only `files`, `{key: {size}}`, listed through the first device's
key.

## Files on a device

| op | arguments |
|---|---|
| `drive_write` | `path`, one of `text` / `hex` / `size` + `seed` / `fill` + `size`; `mtime` (ns), `exec` (bool) |
| `drive_overwrite_in_place` | `path`, content as `drive_write` |
| `drive_append` | `path`, content |
| `drive_mkdir` | `path` |
| `drive_rm` | `path`, or `glob` (over the folder's files) |
| `drive_rmtree` | `path` |
| `drive_rename` | `from`, `to` |
| `drive_symlink` | `path`, `target` |
| `drive_hardlink` | `path`, `target` |
| `drive_mkfifo` | `path` |
| `drive_chmod` | `path`, `mode` (octal string, `"644"`) |
| `drive_utime` | `path`, `mtime` (ns) or `like` (a path) |
| `drive_many` | `dir`, `count`, `size` (default 64), `seed`, `text_prefix` |
| `drive_office_save` | `path`, `text`, `keep_lock`, `keep_temp` (Word's save: temp file, old renamed aside, new renamed in) |
| `drive_editor_save` | `path`, `text`, `style` (`backup`), `keep_swap` (a sibling renamed over the file) |
| `drive_move_root` | `device`, `suffix` (default `-moved`): later steps use the new place |
| `drive_remove_root` | `device` |
| `drive_replace_root` | `device` (an empty folder in its place) |
| `drive_wipe_state` | `device` (the pairing's state files, as a lost cache) |
| `drive_corrupt_state` | `device` (its base cut off mid-write) |

Each takes `device` (default: the first device opened) and answers `{"success": true}` or the
OS error. `text` content also counts as a "live text" for `kept_live`.

## Faults

| op | arguments |
|---|---|
| `drive_fault` | `device` (absent: every device of the remote) or `remote`; any of `partition` (bool), `error_rate` (0..1) + `seed`, `rate_limit_every` (n) + `retry_after` (s), `cut_after_ops` (n), `quota_bytes`, `fail_keys` (globs), `refuse_ops` (`get`, `put`, `put_if`, `head`, `list`, `delete`), `auth_failed` (bool), `latency_ms`; `clear` (bool, first) |
| `drive_crash_at` | `device`, `point` (armed for the next pass; `drive_sync` `crash_at` is the same) |
| `drive_disk` | `device`, `free_bytes` (what the engine's free-space check sees) or `full` (bool: writes in the folder and the state folder fail with "no space"); `clear` |
| `drive_clock` | `device`, `offset` (seconds the device's sync clock is off) or `jump` (seconds added to the offset); answers `offset` |

The fault layer sits between the device's engine and the remote: a partitioned device gets a
connection error for every request, `error_rate` a 503 for that share of requests (seeded,
repeatable), `rate_limit_every` a 429 with `Retry-After` for every n-th, `cut_after_ops` a
connection error for every request after the n-th from now, `quota_bytes` a "quota exceeded"
for a PUT that would pass it, `fail_keys` and `refuse_ops` a 500 for the matching requests,
`auth_failed` a 401. The clock is the device's sync engine clock (`SyncSession::with_clock`, as
the command line's `AZSYNC_CLOCK_OFFSET`), from the device's next pass on. The disk fault is
what the engine's free-space check and local writes see through the fault seam
(`azcloud_kit::sync::faults::free_bytes` and `check_write`).

## Tampering with a remote

| op | arguments |
|---|---|
| `drive_tamper` | `remote`, `action`, and the action's fields (below) |
| `drive_snapshot` | `remote`, `name`: keeps the remote's index as it is now |

Actions (a hostile or broken server, past every device's fault layer): `put` (`key`, content,
`exec`, `writer` (device name)) adds a file the engine's way; `delete` (`key`) a tombstone;
`delete_all`; `rollback` (`to`: a snapshot's name, or `generations`) puts an older index back
with no announcement; `restore` (`to`) the same as an announced restore; `corrupt_index`;
`corrupt_blob` (`key`); `truncate_blob` (`key`, `keep` bytes); `raw_entry` (`key`,
`entry_json`); `raw_key` (`key_hex`, content; a key that is not UTF-8 cannot be written in a
JSON index and fails); `features` (a list: the index names format features this version does
not know). `restore` and the actions on stored objects are Group B's (below).
On an encrypted remote only `put`, `delete` and `delete_all` work (through the remote's key,
the engine's way); the actions that damage one stored object fail there, since an encrypted
remote does not name its objects by the file's key, and so does `drive_snapshot`.

## Checks

`{"op": "drive_check", "check": "<name>", ...}` answers `ok` and `detail`; `success` is `ok`,
so a failed check fails the step with its detail. The names and fields are sync-chaos's:

| check | fields |
|---|---|
| `file` | `path`, `absent`, `text`, `one_of`, `size`, `same_as`, `exec`, `mode_max` |
| `link` | `path`, `absent` |
| `files_count` | `glob` (default `*`), `count` |
| `conflict_copies` | `path`, `count` (`<stem> (conflict *)<ext>` beside it) |
| `trash_has` / `trash_empty` | `text` (the device's trash) |
| `no_temp_files` | - (none of the engine's temp files left in the folder) |
| `converged` | `devices` (default all): every folder holds the same files |
| `kept_live` | `texts`: each is in some folder or on the remote |
| `kept` | `texts`: each is in some folder, trash or stored version |
| `outside_untouched` | - |
| `remote_never_has` | `marker` (default `{secret}`): no stored object holds it (plaintext; on an encrypted remote also no ciphertext equals it) |
| `remote_file` | `key`, `absent`, `text`, `one_of`, `uniform`, `same_as`, `exec` |
| `remote_count` | `glob`, `count` |
| `remote_consistent` | - (every indexed file's object is there and matches its hash) |
| `problem` | `code` or `code_any`, `key` (glob), `absent`, `when` (`last`: the last pass's list, else every pass's) |
| `max_ops` | `key`, `change` (`trash`, `delete`, `overwrite`, `conflict`, `move`), `max` (destructive changes in the device's operations log, `ops.jsonl`; sync-chaos names the kind `op`, which a step cannot carry: its `op` is the command) |
| `remote_puts` | `max` (the device's successful PUTs) |
| `retry_after_respected` | - |
| `state_private` | - (the state folder is not readable by others) |
| `mtime` | `path`, `ns`, `tolerance_ns` |
| `last_pass_quiet` | - (the device's last pass changed nothing) |

`free_at_least`, `kept_soak` and `run_ok` stay sync-chaos checks (they need the real disk, a
soak, a shell) and fail here with that reason.

## Group C: keys, recovery, share links

### A device's drive key

| op | arguments | result |
|---|---|---|
| `drive_open` | also `joined` (bool, default true), `key_from` (`recovery`), `recovery_code` | as above |
| `drive_recover` | `device`, `recovery_code` (default the remote's), `expect` (`ok` default, `refused`) | `recovered`, `why` (refused) |
| `drive_join` | `device`, `approver` (a device that holds the key), `pause_at` (`approve`) | `joined`, `error`; `paused` with `pause_at` |
| `drive_rotate_key` | `remote` | `rewrapped`, `members_removed`, `shares_revoked`, `drive_key` |
| `drive_send_again` | `device` | - |

A device opened the usual way shares the remote's keyring with the remote's owner (one user's
computers, "already joined"). `joined: false` gives it a keyring of its own that starts empty,
as a new or reinstalled computer or a forgotten passphrase leaves it: on an encrypted remote its
passes fail with `sign-in-needed` and it reads and writes nothing. `key_from: "recovery"` is
such a device that unlocked with the recovery code at once. The remote's recovery code is the
one `drive_create` made (the recovery sheet), the new one after a rotation; a step writes it as
`{recovery:<remote>}`, the one before the last rotation as `{recovery_old:<remote>}`.

`drive_recover` is "Unlock with the recovery code" (azul-storage `device::recover`, through the
device's fault layer): the device enrols itself and keeps the key. `drive_join` is "Add a
device": the approver seals the drive key to a one-time invite in the bucket, the device opens it
and enrols. With `pause_at: "approve"` the approval waits until `drive_cont` of the joining
device, which answers `joined` (false, with the `error`, when the approver could not seal the
key - cut off, say); `success` either way. `drive_rotate_key` is the owner's "I was hacked"
(`rotation::rotate`): the index is sealed under a new key, every file key re-wrapped, every other
member and every share link removed, a new recovery code made. The joined devices hold the new
key at once; a device with a keyring of its own is locked out until it unlocks with the new code.
It refuses while a pass of the remote's devices runs. `drive_send_again` is AzDrive's "Send them
again" for the files the problem list keeps after the drive deleted them elsewhere
(`deleted-elsewhere-kept`): they go up with the next pass.

### Share links

| op | arguments | result |
|---|---|---|
| `drive_share_link` | `device`, `path` (a file, or a folder and all in it), `name` (default the path), `expires_s` | `link`, `host`, `id`, `files` |
| `drive_open_link` | `link` (its name, URL or path) | `available`, `why`, `files` (`{name: {size, readable, error}}`), `unreadable` |
| `drive_share_expire` | `remote` | `expired` |

A link is azul-storage's share of an encrypted drive (`sharing::share`): a manifest in the bucket
sealed with a share key only the link carries (`https://share.e2e.invalid/s/<id>#k=<key>`: the
key in the fragment, the host never the app's), the shared files' keys wrapped by it, the drive
key never. A share is a snapshot: a file added to a shared folder later is not in it.
`drive_open_link` visits it as someone without an account, through the stand-in share server's
own connection (a fault on every device reaches it, one on a single device does not).
`drive_share_expire` lets the expiry of every link made with `expires_s` pass;
`drive_tamper` `takedown_link` (`path` or `link`) takes one link down. Neither deletes anything; a
revoked share (a rotation revokes all) opens nothing either. Check `share_link`: `path` or `link`,
`readable` (true: every file reads back as the share names it; false: the link opens nothing).

Account-to-account folder shares (`drive_share` with `to`, `drive_unshare`, `drive_open` with
`as_user` and `share`, `drive_accept_share`, the checks `share_keys`, `share_reads`,
`share_link_sees`, `drive_tamper` `replace_share_key`) are not commands yet: the engine has no
folder keys in the drive index, and the scenarios that need them are pending with that reason.

### The runner and the scenario files

`examples/azul-drive/scripts/e2e_sync.sh` and every `AZ_E2E=<folder>` run take the folder's
subfolders too (`e2e/sync/catalogue/`). A test marked `"pending": "<what it waits for>"` runs no
step and reports PENDING with that text; `"not_applicable": "<why>"` reports SKIP; neither fails
the run. `"expect": "fail"` is a known failure (XFAIL), and a pass of it fails the run (XPASS).
`max_ops` names the kind of change `change`, not `op`: a step's `op` is its command.

## An example

```json
{
  "name": "an edit on two devices keeps both versions",
  "steps": [
    { "op": "drive_create", "remote": "r" },
    { "op": "drive_open", "device": "a", "remote": "r" },
    { "op": "drive_open", "device": "b", "remote": "r" },
    { "op": "drive_pair", "device": "a" },
    { "op": "drive_pair", "device": "b" },
    { "op": "drive_wait_idle" },
    { "op": "drive_write", "device": "a", "path": "f.txt", "text": "one" },
    { "op": "drive_sync", "device": "a" },
    { "op": "drive_sync", "device": "b" },
    { "op": "drive_write", "device": "a", "path": "f.txt", "text": "a's" },
    { "op": "drive_write", "device": "b", "path": "f.txt", "text": "b's" },
    { "op": "drive_sync", "device": "a" },
    { "op": "drive_sync", "device": "b" },
    { "op": "assert_response", "path": "result.conflicts", "equals": 1 },
    { "op": "drive_resolve", "device": "b", "key": "f.txt", "choice": "both" },
    { "op": "drive_sync", "device": "b" },
    { "op": "drive_sync", "device": "a" },
    { "op": "drive_check", "check": "conflict_copies", "device": "b", "path": "f.txt", "count": 1 },
    { "op": "drive_check", "check": "kept_live", "texts": ["a's", "b's"] },
    { "op": "drive_check", "check": "converged" },
    { "op": "drive_cleanup" }
  ]
}
```

Scenarios live in `examples/azul-drive/e2e/sync/`, one file per sync-chaos scenario or catalogue
entry, named after it (`res-crash-points.json`), with the scenario's id and sources in its
`description`.

## Group B: hostile servers, networks and disks; tampering

### More faults between a device and its remote

`drive_fault` also takes these fields (with `device`, `remote` and `clear` as above; a device's
own entry replaces the one for every device, as for the basic faults):

| field | what the device meets |
|---|---|
| `read_only` | `true` or why (`payment-failed` / `unpaid`, `lockdown`, `operator`, `banned`): a write of new content (a blob, an encrypted data object, a plain file) answers 403 with that `x-azlin-error`; reads, deletes and the drive's bookkeeping (its index, its encrypted metadata) still work, so the owner's deletes commit (rule V7) |
| `moved` | `true` or the new address: every request answers 301 `PermanentRedirect` (problem `server-moved`) |
| `hang` / `hang_ms` | every request is taken and never answered; the transport's timeout (60 s, or `hang_ms`) ends it with a connection error. Clearing the fault lets a hanging request give up at once |
| `tls_untrusted` | every connection meets a certificate the device does not trust (an intercepting proxy): a connection error that names the certificate |
| `short_body_after_bytes` + `report_ok` | a GET's body ends after that many bytes: as a connection error, or (`report_ok: true`) as a short success, the way a client that ignores Content-Length takes it |
| `fail_files` | globs over FILE keys (under the remote's prefix): every request for those files' objects answers 500. On a plain remote a file's blob is found by its BLAKE3, in the index or in a device's folder; on an encrypted remote the calls are failed above the encryption, where they still name the file |
| `answer_from` | another remote's name: it answers every request of the device instead (a wrong or hijacked address) |
| `max_files` | a plain index commit naming more files than this answers 507 `QuotaExceeded` (encrypted remotes refuse the field: their commits are sealed) |
| `max_object_bytes` | a content object over this many bytes answers 413 `EntityTooLarge` (`object_too_large`) |

The engine names them `read-only`, `server-moved`, `too-large`, `quota-exceeded`,
`remote-offline` (a timeout, a certificate, a cut body), `hash-mismatch` (a short success).

### A folder that cannot be listed

| op | arguments |
|---|---|
| `drive_local_fault` | `device`, `path` (a folder of the device; absent: its top), `readdir_error` (`EIO`, `ENOTDIR`, `EACCES` or an error number), `passes` (default 1; 0 ends it) |

The scans of the device's next `passes` passes meet that OS error when they list the folder
(`azcloud_kit::sync::faults::set_readdir_error`): the error of a failing disk or network
share, a folder swapped for a file, a permission. The engine lists the folder as `unreadable`
and takes nothing in it for deleted.

### A crash inside a download

The engine names one more point: `mid-download`, inside a big download's body (one in more
than one part, over 16 MiB), once its first part is in the spool and before `download-start`
(which follows the fetch). A crash there stops the download as a killed process would: no
further part is fetched, and the parts in the spool stay for the next pass to resume
(`drive_sync` `crash_at: "mid-download"`). A download in one part never meets it.

### Extended attributes and flags

| op | arguments |
|---|---|
| `drive_xattr` | `path` (a file or folder of the device), `name`, the value as `drive_write`'s content (`text`, `hex`, `size` + `seed`) |
| `drive_chflags` | `path`, `flags`: `chflags`'s names (`uchg`, `uappnd`, `hidden`, `nodump`, `opaque`, `arch`, `schg`, `sappnd`), comma-separated, `no` before one clears it, `""` clears all (macOS only) |

The sync carries neither (rule P6): a pass lists each folder whose items carry a user's
attribute or flag once, as the warning `xattrs-not-synced` (the folder's key), and syncs their
content as usual. The system's own attributes (quarantine, provenance, sandbox grants, SELinux
labels) are not counted. `drive_cleanup` clears every flag first, so an immutable file never
keeps its sandbox from going.

### Raw names and times before 1970

`drive_write` also takes `path_hex` instead of `path`: the relative path as hex bytes, for a
name that is no UTF-8 (Linux and the BSDs hold one; a JSON string cannot). `mtime` (in
`drive_write` and `drive_utime`) and the `mtime` check's `ns` are signed: a time before 1970 is
a negative number of nanoseconds, and `drive_read` / `drive_list` answer one as such.

### Files hashed

`drive_counters` with a `device` also answers `hashed`: the files the device's scans read and
hashed since `drive_open` or `drive_restart` (an unchanged file keeps its base's hash and is not
counted). A file written within 2 s of a scan is read again by the next one (the racy window), so
a scenario that counts lets its files age first (`{"op": "wait", "ms": 2500, "real": true}`).

### A hostile server's changes to stored objects

`drive_tamper` takes these actions too (`remote`; `key` is a file's key under the remote's
prefix). A server holds no drive key: on an encrypted remote they change whole stored objects
and their bytes, as a keyless server can. To aim at one file's object the harness reads the
index with the remote's key, past every fault; nothing it changes needs the key.

| action | fields | what changes |
|---|---|---|
| `raw_put` | `key`, content | an object written into the bucket past the drive's key (a web or WebDAV upload into an encrypted folder); the drive's index does not name it, so no device lists or writes it |
| `drop_blob` | `key` | the file's stored object removed; the index still names it |
| `purge` | `key` | a permanent delete on the server: every object any version of the file had (found in the drive's history; a plain remote's entry and tombstone too); a pack holding other files stays |
| `graft_blob` | `key`, `from_remote`, `from_key` | the file's object replaced by another remote's (another account's) object of `from_key` |
| `corrupt_object` | `key`, `byte` | one byte of the file's object flipped (`byte`: its offset; default the middle of the file's first segment) |
| `truncate_object` | `key`, `keep_chunks` | an encrypted object cut after that many AZL1 segments |
| `drop_chunk` | `key`, `chunk` | one segment taken out |
| `swap_chunks` | `key`, `first`, `second` | two segments exchanged |
| `reorder_chunks` | `key` | the segments in reverse |
| `replay_chunk` | `key`, `chunk` | an earlier segment put in that one's place |
| `splice` | `key`, `from_key` | the other file's first segment in place of this one's |
| `swap_names` | `key`, `with` | the two files' objects exchanged (files under 512 KiB share packs: write bigger ones) |
| `replace_drive_key` | - | every member's key file replaced by one that seals another drive key to the same member: a device that keeps its key goes on under the true one; one that unlocks from its key file is refused (it is no member under that key) |
| `restore` | `to` (a snapshot) or `generations` | an announced restore of a plain remote: the stand-in server's announcement with the next epoch first (`<remote>/announcements/restore.json`, the token server's `restore` field, unsigned here), then the older index. Every device's passes take it (`SyncSession::with_restore`) as that restore: `server-restored`, the files the restored index lacks sent again, nothing deleted |

`corrupt_entry` and `drop_folder` stay refused on an encrypted remote: its entries live in
sealed packs of its metadata repository (a pack per change, sealed per chunk), so a keyless
server damages a chunk, never one entry, and the stand-in cannot yet tell which chunk holds a
path.

| op | arguments | result |
|---|---|---|
| `drive_mark_objects` | `remote`, `name` | `objects`: records every content object (AZL1 objects and packs, plain blobs) and its BLAKE3, for the `objects_fresh` check |

### Checks of what is stored and what is left readable

| check | fields | holds when |
|---|---|---|
| `nonces_unique` | `remote` | no nonce of an AZL1 object's public header (its segments' STREAM prefix, its info's, its trailer's) is used twice across the remote's objects; fails on a remote with no AZL1 object (nothing tested) |
| `remote_objects_disjoint` | `remotes` (two or more) | no content object of one remote has the bytes of one of another (no deduplication across accounts) |
| `objects_fresh` | `remote`, `since` (a `drive_mark_objects` name) | no content object stored now has the key or the bytes of one recorded then (a purge removed them; an upload of the same bytes is new ciphertext) |
| `logs_never_have` | `marker` | no file of AzDrive's own folders (`AZLIN_DATA`, `AZDRIVE_CACHE`, `AZDRIVE_HOME`, the drives file, the run's cache folder) that others can read holds `marker` in its name or bytes; private files (no group or other permission), the pairings' state folders, the devices' folders, the remotes and the outside folder are not searched |
| `logs_clean` | `device` | the same for the name of every file (five bytes or more) in the device's folder |

`argv_clean` (no child process carries a secret) stays sync-chaos's: AzDrive has no seam that
records the processes it starts.
## Group A

Cloud-only files, a pairing's modes, the window's actions around a pass and the drive, and the
computer around AzDrive (`src/e2e/a_*.rs`).

### A pairing's modes

| op | arguments | result |
|---|---|---|
| `drive_pair` | as above, plus `mode` (`two-way` default, `backup`, `camera-upload`), `remote_deletes` (`keep`: a backup's rule), `on_demand` (bool), `exclude` (globs) | as above, plus `backup`, `on_demand`, `exclude` |
| `drive_select` | `device`, `exclude` (globs, `.azcloudignore` lines; `[]` takes everything in again) | `exclude` |
| `drive_on_demand` | `device`, `on` (bool) | `auto_download` |

A **backup** pairing (`mode: backup`, `camera-upload`, or `remote_deletes: keep`) syncs one
way, from the folder to the drive (azcloud-kit's `sync::oneway`): a file deleted here stays on
the drive and shows as cloud only here; a file the drive deleted or changed is sent again from
here; a file new on the drive stays there (cloud only); nothing here is ever trashed,
overwritten or renamed by its passes. `links` other than `skip` fails: the sync keeps no links
yet (they are listed as `symlink-skipped`).

`exclude` is selective sync: what this device leaves out is neither sent, fetched nor deleted
on either side; taking it in again (`drive_select` with `[]`) moves nothing that is the same on
both. `on_demand: true` (or `drive_on_demand` on) is "nothing comes down until it is opened";
off is "everything", and the next pass brings every cloud-only file down except the ones the
user freed up.

### Cloud-only files

| op | arguments | result |
|---|---|---|
| `drive_evict` | `device`, `path` (a file, a folder with all in it, `.` the whole folder) or `glob` | `evicted` (keys in the cloud only now), `refused` (keys still here), `reasons` (`{key: why}`) |
| `drive_hydrate` | `device`, `path` or `glob` | `hydrated`, `failed` (keys) |
| `drive_write_through_placeholder` | `device`, `path`, content as `drive_write` | `written` (bool), `reason`, `key` |
| `drive_corrupt_placeholder` | `device`, `path` (a cloud-only file) | `key` |
| `drive_scan_read` | `device`, `glob` (default `*`) | `read` (files), `bytes` |

`drive_evict` is the ribbon's "Free up space": a file's bytes leave this device only once the
drive holds that very version whole - its index names it and its copy is read back and checked
against its BLAKE3. A file not synced yet, changed since, or whose drive copy is missing or
damaged is refused (the damaged one listed as `hash-mismatch`); `success` is true either way.
`drive_hydrate` is opening the files (they come down). `drive_write_through_placeholder` is an
app saving into an on-demand pairing: a cloud-only file comes down first; a write that would
leave less than the disk's reserve free (`drive_disk` `free_bytes` counts) is refused with
`written: false` and listed as `disk-reserve`. `drive_corrupt_placeholder` damages what the base
keeps of a cloud-only file (its content's name and size), as a crash mid-write does: the next
pass takes the drive's version as cloud only again, with no download and no delete.
`drive_scan_read` reads every matching file of the folder end to end, as a virus scanner does
(a cloud-only file has no bytes here to read).

`drive_states` answers each file's `modified` too (seconds: the drive's time of the version it
knows), so a cloud-only file's new size and time can be checked without a download.

### The window's actions and the computer around

| op | arguments | result |
|---|---|---|
| `drive_setting` | `device`, `name`, `value` | `settings` (the pairing's, as the settings file keeps them) |
| `drive_settings` | `device`, `set` (an object of name: value, optional) | `settings` |
| `drive_cancel` | `device` | `cancelled` (a pass was running) |
| `drive_ops` | `device` | `ops`: `[{at, op, key, to, hash, reason}]`, oldest first |
| `drive_untrash` | `device`, `path` (a file or folder; absent: everything) | `restored` (keys) |
| `drive_network` | `metered`, `constrained`, `hotspot` (bools, default false), `connected` (default true) | the same |
| `drive_wake` | - | `passes` (drives asked) |
| `drive_lock` | `device`, `path` (optional) | `locked` (the key), or `key_here: false` |
| `drive_unlock` | `device`, `path` (optional) | `unlocked`, or `key_here: true` |

A setting's `name` is the settings file's: `auto_download` (`"everything"`, `{"new_under": 25}`,
`"pinned_only"`, `"nothing"`), `keep_gb`, `paused`, `sync_on_metered`, `backup`, `exclude`,
`local_copies`, plus `mode` and `remote_deletes` as `drive_pair` takes them; the pairing's
drive and folders are no setting. `drive_cancel` is Cancel while a pass runs: it stops before
its next file and commits nothing half done (a pass held at a pause point is let go so it can
stop). `drive_ops` is the operations log with each step's `reason`. `drive_untrash` puts back
the newest trashed version of each file (found through the log), never over a file that is
there; the next pass sends it up again. `drive_network` is the network as AzDrive reads it from
the system: on a metered or low-data one, files over the auto-download size wait (both ways)
while small ones go on, and the status line says `azdrive-sync-status-metered`. `drive_wake` is
the system waking up: every synced drive that is not paused gets a full pass now.

`drive_lock` with a `path` holds that file with an exclusive lock, as another app mid-write
does (`flock` on macOS and Linux, `LockFileEx` on Windows) until `drive_unlock` with the same
path: a pass leaves a changed file it finds locked for later, lists it as `locked`, and never
takes it for deleted. Without a `path` the drive's key leaves the device's keyring (an encrypted
remote's device only; kept aside): its passes fail as `sign-in-needed`, changing nothing on
either side, until `drive_unlock` puts the key back (or `drive_recover` does, with the recovery
code). Joined devices share the remote's keyring, as one user's computers share one system
keyring here: they lock together. A `passphrase` fails: AzDrive keeps the key in the system's
keyring and has no passphrase of its own (a wrong recovery code is `drive_recover` with
`expect: refused`).

### The read-back

| op | arguments | result |
|---|---|---|
| `drive_verify` | `device` | `checked`, `mismatches`, `healed`, `rotted`, `restored` (keys) |
| `drive_sync` | as above, plus `verify` (bool): the read-back first, then the pass | as above |
| `drive_bitrot` | `device`, `path`, `offset`, `hex` or `byte` (or content as `drive_write`) | `changed` (bytes) |

`drive_verify` runs AzDrive's read-back of the pairing (azcloud-kit's `sync::verify`): every
file the drive's index names is read back and checked against its BLAKE3 - a copy that does not
read back whole is a mismatch, sent again from here when this device holds that version
(`healed`) - and every synced file here whose size and time are as its last sync left them but
whose bytes are not (`rotted`) gets the drive's version back (`restored`), the damaged bytes
going into the folder's trash (and its operations log). Each finding is listed as
`hash-mismatch`; the report is kept as `verify.json` in the state folder. `drive_bitrot` changes
bytes in place as a failing disk does: the size and the modification time stay (the change time
moves - only a raw write to the disk keeps that).

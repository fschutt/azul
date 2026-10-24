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
# one scenario, or a folder of them (each file runs in a process of its own)
AZ_E2E=examples/azul-drive/e2e/sync AZLIN_DATA=$(mktemp -d) AZUL_DRIVES=$(mktemp -d)/drives.json \
  AZDRIVE_DIALOGS=inline AZDRIVE_SYNC_POLL=3600 AZDRIVE_SYNC_SETTLE_MS=0 ./AzDrive
```

The runner passes no command line to the per-file processes, so everything is set by the
environment. `AZDRIVE_SYNC_POLL=3600` keeps the poll timer out of the way: a scenario starts its
passes itself. `AZDRIVE_E2E_WORK` sets the sandbox's parent folder (default: the system's
temporary folder); `AZDRIVE_E2E_KEEP=1` keeps the sandbox after the run.

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
- **Crash and pause points** are named places in a pass: `after-scan`, `after-blob`,
  `after-commit`, `before-rename`, `conflict-start`, `before-trash`, `before-base-save`. A crash
  stops the pass thread there by unwinding it: nothing after the point runs, no error handler
  runs, only destructors (so a temp file a guard owns is removed, unlike after `kill -9`; the
  sync-chaos `engine` adapter covers a real `kill -9`). The device's in-memory sync state is
  dropped and read again from its state folder, as a restarted app would. A pause blocks the
  pass thread there until `drive_cont`.

Every answer below is the `result` object; `success` is left out where it is simply `true`.

## The sandbox

| op | arguments | result |
|---|---|---|
| `drive_harness` | `work` (folder, default `$AZDRIVE_E2E_WORK` or the temp folder), `keep` (bool) | `run`, `outside`, `secret` |

Makes the run's sandbox: `outside/`, `remote/`, `devices/`, `cache/` (AzDrive's cache folder for
this run: the pairings' state folders). It adds a local drive "e2e" whose root is the sandbox,
so pairing a folder in it is allowed. Any other command calls it first with the defaults when
no scenario did.

## Remotes and devices

| op | arguments | result |
|---|---|---|
| `drive_create` | `remote` (name), `encrypted` (bool, default true), `prefix` (default `""`) | `remote`, `path`, `encrypted` |
| `drive_open` | `device`, `remote`, `device_name` (default the device), `settle_ms` (default 0), `clock` (seconds, default 0) | `device`, `drive_id` |
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
`conflicts`, `drive_problems` lists it as `conflict`, and `drive_resolve` answers it the way the
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
| `drive_problems` | `device` | `problems`: `[{code, key, text}]` |
| `drive_states` | `device` | `files`: `{key: {state, size, cloud_only, pinned, error}}`, `read_only`, `last_problem` |
| `drive_base` | `device` | `entries`: `{key: {hash, size, mtime}}`, `generation` |
| `drive_index` | `device` or `remote` | `generation`, `features`, `files`: `{key: {hash, size, mtime, device}}`, `deleted`: `{key: {hash, at, device}}` |
| `drive_counters` | `device` (absent: the remote's total) | `ops`: `{get, put, put_if, head, list, delete}`, `bytes_up`, `bytes_down`, `refused`, `puts`: `{key: n}`, `passes`, `crashes` |
| `drive_list` | `device` | `files`: `{path: {kind, size, sha256, mode}}` (the folder as it is, AzDrive's bookkeeping left out) |
| `drive_read` | `path` (+ `device`) | `exists`, `kind` (`file`, `dir`, `link`, `special`), `size`, `text` (when UTF-8), `sha256`, `mode`, `target` (a link's) |

Problem codes are the sync engine's (`symlink-skipped`, `special-file`, `bad-name`,
`name-collision`, `unreadable`, `changed-during-sync`, `mass-delete-held`, `burst-held`,
`root-missing`, `disk-full`, `quota-full`, `read-only`, `newer-format`, `rollback-refused`,
`loop-frozen`, `quarantined`, `conflict`, ...). `drive_index` with a `device` reads the index as
that device's engine sees it (through its fault layer and, on an encrypted drive, its key);
with a `remote` it reads past every fault. On an encrypted remote `hash` is the plaintext
BLAKE3 the engine recorded, never a stored object's name.

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
| `drive_clock` | `device`, `offset` (seconds from now, as the engine's clock reads it) or `jump` (seconds added to the offset) |

The fault layer sits between the device's engine and the remote: a partitioned device gets a
connection error for every request, `error_rate` a 503 for that share of requests (seeded,
repeatable), `rate_limit_every` a 429 with `Retry-After` for every n-th, `cut_after_ops` a
connection error for every request after the n-th from now, `quota_bytes` a "quota exceeded"
for a PUT that would pass it, `fail_keys` and `refuse_ops` a 500 for the matching requests,
`auth_failed` a 401. The disk and clock faults are read by the engine itself, through the sync
module's fault seam (`azcloud_kit::sync::faults`).

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
`entry_json`); `raw_key` (`key_hex`, content); `features` (a list: the index names format
features this version does not know). On an encrypted remote `put` and `delete` go through the
remote's key; `corrupt_blob` and `truncate_blob` damage the stored object.

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
| `max_ops` | `key`, `op`, `max` (destructive changes in the device's operations log) |
| `remote_puts` | `max` (the device's successful PUTs) |
| `retry_after_respected` | - |
| `state_private` | - (the state folder is not readable by others) |
| `mtime` | `path`, `ns`, `tolerance_ns` |
| `last_pass_quiet` | - (the device's last pass changed nothing) |
| `free_at_least` | `bytes` |

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
    { "op": "drive_check", "check": "converged" }
  ]
}
```

Scenarios live in `examples/azul-drive/e2e/sync/`, one file per sync-chaos scenario or catalogue
entry, named after it (`res-crash-points.json`), with the scenario's id and sources in its
`description`.

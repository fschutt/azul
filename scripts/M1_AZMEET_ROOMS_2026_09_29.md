# M1 AzMeet rooms: report (2026-09-29)

The user's goal: "request room, send someone the link, other AzMeet client can join". Built in two repositories:
the `meet` Cloudflare Worker with a local mock in azul-apps, and "New meeting / Join with a link" in AzMeet.

## A. The `meet` Worker (azul-apps)

Worktree `/Users/fschutt/Development/azul-apps-m1`, branch `cf-workers-meet` (from azul-apps `main`; nothing
committed to `main`, the user's checkout untouched). Directory `cf-workers/meet/`. No dependencies.

- `src/handler.js`: the one request handler (the Worker's `fetch`), written against the storage interface.
  - `POST /rooms` -> `201 {room, code, link, url, expires}`. `room` = 26 chars of lower-case Crockford base32
    (130 random bits, the credential); `code` = short alias `xq4-8kd-2nm` (31^9); `link` = `azlin://meet/<room>`.
    Rate limited per client IP (SHA-256 of `CF-Connecting-IP`, fixed windows, counted through the store).
  - `POST /rooms/<id>/peers` <- `{node_id, ticket, name}`: validated (node id `[0-9a-z]{32,128}`, lower-cased;
    ticket printable ASCII <= 2048; name cleaned of control / bidi / zero-width chars, <= 64 code points,
    default "Guest"). Re-announce replaces. At most `MAX_PEERS_PER_ROOM` (409 `room_full`), members may always
    re-announce. Announcing extends the room.
  - `GET /rooms/<id>/peers?except=<node_id>` -> `{room, peer_ttl_seconds, peers: [{node_id, ticket, name, updated}]}`.
  - `GET /rooms/<id or code>` -> landing page (open `azlin://meet/<id>`, or download); JSON with
    `Accept: application/json` or `?format=json`. The only route with CORS. Code lookups rate limited; the peers
    API accepts only the full id. `no-referrer`, `no-store`, `noindex`, a script-free CSP (the URL is the key).
  - `GET /health`. POST bodies must be `application/json` (CSRF: no simple cross-origin POST), <= 4 KiB, read with
    a cap (a longer stream is cancelled, not read).
  - TTL: rows past `expires_at` are invisible at once; `sweep` deletes rooms, orphaned/expired peers and old
    rate rows at most once a minute per isolate (`ctx.waitUntil` on Cloudflare).
- `src/store.js`: the storage interface (JSDoc typedef) implemented once in SQL, plus the schema (created on
  first use). Two database adapters, the same SQL:
  - production `src/db/libsql-http.js`: libSQL over HTTP (Hrana v2 pipeline, JSON) via `fetch` - Turso or a
    self-hosted `sqld`; `libsql://` mapped to `https://`; bearer token. (D1 not done; it would be a ~15-line adapter.)
  - dev `src/db/node-sqlite.js`: `node:sqlite` (Node 26 has it, no warning).
- `src/worker.js`: Worker entry (secrets `LIBSQL_URL`, `LIBSQL_AUTH_TOKEN`; a clear 500 without them).
- `dev-server.mjs`: the same handler on `http://127.0.0.1:8787` over node:sqlite (`--memory`, `--db`, `--port`);
  the client address comes from the socket (a forged `cf-connecting-ip` is replaced).
- `wrangler.toml`, `README.md` (API, local run, deploy commands for the user: turso db create / wrangler secret
  put / wrangler deploy; never run here), `.gitignore`, `package.json` (`npm test`).
- Tests (`node --test "test/*.test.mjs"`, 41 pass): handler (mint, uniqueness, rate limit + reset, 415, announce /
  list / except, re-announce, lower-casing, 404s, malformed bodies, name cleaning, streaming 413, room cap, peer
  TTL, room TTL + extension, physical sweep, landing HTML / JSON / code / code rate limit / 404 page, CORS only on
  the landing route, health / 404 / 405); the store conformance suite against BOTH adapters (libSQL against
  `test/fake-hrana.mjs`, a Hrana v2 server over node:sqlite); libSQL URL mapping, token, error propagation; the
  Worker entry end to end over HTTP; the dev server as a child process.

## B. AzMeet (`examples/azul-meet`)

- `src/rooms.rs` (pure, no azul types; 15 unit tests): `parse_room_link` / `parse_room_key` (`azlin://meet/<key>`,
  `http(s)://<host>/<base>/rooms/<key>` with query/fragment ignored, bare key; ids in the Worker alphabet, codes
  normalised, case and dashes optional), `dials` (the lower endpoint id dials), `diff_peers` (joined / moved /
  renamed / left, without self or repeats), `plan_dials` (dial who we dial, not connected, new or moved or silent
  for `REDIAL_AFTER_POLLS`), `relay_choice` (`AZMEET_RELAY`; default off for a loopback server, n0 relays else).
- `src/lib.rs`:
  - Meeting server = `AZMEET_WORKER`, else `PRODUCTION_WORKER` (`option_env!("AZMEET_DEFAULT_WORKER")`, empty by
    default; the Worker README tells the user to bake it in). Before any window, a TCP probe (azul `Url` for
    host/port, <= 2.5 s on a std thread) decides; without a reachable server the old in-process two-window demo
    runs, and both windows show a notice saying why.
  - Rooms mode: one window, one iroh endpoint. Start screen: "New meeting" (Button, `POST /rooms`) and "Join with
    a link" (TextInput + Join; looked up with `GET /rooms/<key>?format=json`, which also resolves codes). In the
    meeting: header with the code, notice line, invite bar (link + Copy link via `set_clipboard_content`), people
    row ("Ada (you)", "Ben · connected" / "connecting" / "waiting for them to connect"), then the existing grid,
    toolbar and devices panel.
  - Announce `{node_id, ticket, name}` once the `Ready` ticket exists and every 20 s (10 polls); read peers every
    2 s (`room_tick` timer); dial per `plan_dials`. A 404 ends the room (back to Start, or `Ended` keeping the
    connected peers). Server errors show in the notice and clear on the next success. Stuck requests are dropped
    after 8 polls.
  - HTTP: every request runs in an azul `Thread` (`http_thread` calls `HttpRequestConfig::http_request`, which
    blocks that worker thread and queues the result in the process-wide resume queue); the UI thread resumes
    `on_room_opened` / `on_announced` / `on_peers` on its next pump (the 15 ms link timer). No callback blocks.
  - Per peer: `MeetState.remotes` (handle, node id, tracks) replaces the single `remote`; tiles are keyed
    `azmeet-peer-<handle>-track-<track>`; frames route by `IrohEvent.peer`; stats line per peer. The demo uses the
    same path.
  - `AZMEET_AUTOCREATE=1` / `AZMEET_JOIN=<link>` start without a click; stdout gets `AZMEET_ROOM`, `AZMEET_LINK`,
    `AZMEET_CODE` lines; `AZMEET_NAME` sets the name.
- `Cargo.toml`: description only.

## C. `examples/azul-meet/scripts/two-clients.mjs`

Starts the dev server (in memory), Ada (`AZ_BACKEND=headless AZ_DEBUG=8765 AZMEET_AUTOCREATE=1`), reads her
`AZMEET_LINK`, starts Ben (`AZ_DEBUG=8766 AZMEET_JOIN=<link>`), then passes when the dev server lists both and each
app's `get_node_hierarchy` has a text "<other> · connected" (also calls `get_state`). Finds the binary and the Worker
dir itself (worktree-aware), or `--bin` / `--worker-dir`; logs kept on failure. Dry-run against a Node stand-in
for the app: PASS (so the orchestration and the Worker side are proven; the Rust app is not).

## Commits

azul, branch `wt/m1-azmeet-rooms`:
- `1f8719619` test(azmeet): what a meeting link is, who dials, and how the peers list changes (RED)
- `45891d7d2` feat(azmeet): read meeting links, pick the dialer, diff and plan the peers
- `07cbd6a10` test(azmeet): two AzMeet processes meet through the local meet Worker (RED)
- `f6468fd94` feat(azmeet): new meeting and join with a link, through the meet Worker
- `27acb81a8` refactor(azmeet): read the meeting server's address with azul's Url
- `aaa4df36c` fix(azmeet): the demo's notice reads as one sentence for each case
- plus this report and the progress file

azul-apps, branch `cf-workers-meet` (worktree `/Users/fschutt/Development/azul-apps-m1`):
- `a75497c` test(meet): what the meet Worker must do, before it exists (RED)
- `047c7de` feat(meet): the rendezvous Worker for AzMeet rooms, and its local mock

## api.json

No change. AzMeet uses existing API only: `HttpRequestConfig::create/with_timeout/with_user_agent/with_header/
http_request`, `HttpGetResult::downcast`, `HttpResponse::body_as_string`, `Thread::create`, `ThreadId::unique`,
`CallbackInfo::add_thread/add_timer/set_clipboard_content`, `Json::parse/object/string/get_key/get_index/len/
as_string/to_string`, `JsonKeyValue::create`, `Url::parse/is_http/is_https/effective_port`, `TextInput`, `Button`.

## Least sure to compile (lib.rs was not compiled; rooms.rs was type-checked alone)

1. `on_announced` / `on_peers`: `meeting_gone(s)` in the 404 arm while `room` (a borrow of `s.room`) is live in
   the other arms. Relies on NLL per-path liveness; if rejected, move the 404 handling after the match.
2. `http_answer`: `answer.result.into_result()` moves a field out of `HttpGetResult` (same as
   `examples/rust/src/resume.rs`, so it should be fine).
3. `http_error_text`: or-pattern over `&HttpError` variants binding `s: &AzString` in each.
4. `HttpJob::announce`: `body.to_string()` must pick `Json`'s inherent `to_string() -> AzString` (then
   `.as_str().to_string()`), not `ToString`.
5. `Thread::create(init, RefAny::new(()), http_thread)`: `RefAny::new(())` as the unused write-back data.
6. `show_remote_frame`: `data.downcast_mut::<MeetState>().and_then(|mut s| ...)` returning `Option<bool>`.
7. `PRODUCTION_WORKER`: `const` `match` on `option_env!`.
8. Glob `prelude::*` plus explicit `task::{Thread, ThreadId, ...}` imports of the same items (should be fine).

Runtime assumptions to watch in the E2E: the resume queue is fed from a worker thread (documented as callable from
anywhere) and pumped by the 15 ms timer; with relays off the iroh `Ready` event needs a local interface address.

## Test commands for the parent

```sh
# the Worker (green here: 41 tests)
cd /Users/fschutt/Development/azul-apps-m1/cf-workers/meet && node --test "test/*.test.mjs"

# AzMeet unit tests (rooms.rs, 15 tests)
cargo test -p AzMeet --lib

# build AzMeet and a libazul with the debug server (AZ_DEBUG / e2e-server), then
node examples/azul-meet/scripts/two-clients.mjs \
  --worker-dir /Users/fschutt/Development/azul-apps-m1/cf-workers/meet \
  [--bin <path to AzMeet>] [--timeout 90] [--keep-logs]
```

By hand: `node .../cf-workers/meet/dev-server.mjs`, then `AZMEET_WORKER=http://127.0.0.1:8787 AZMEET_NAME=Ada AzMeet`
-> New meeting -> Copy link; a second `AZMEET_NAME=Ben AzMeet` -> paste -> Join; both rows turn "connected";
"Start video" in one shows a tile in the other. Without `AZMEET_WORKER` the two-window demo opens with a notice.

## What is left

From the ledger's AzMeet pipeline:
1. Real video: still MJPEG 320x180 q75 per frame, latest-wins. Inter-frame codecs (H.264 / HEVC / AV1) need
   GOP-aware dropping, keyframe requests on loss, a simulcast ladder, native encoders (VideoToolbox, Media
   Foundation, VAAPI / Vulkan Video) and decode through the `<video>` pipeline (fix "first frame only" first).
2. Audio: AzMeet sends NO audio over iroh. The MicrophoneWidget only drives the level meter; there is no audio
   track, no `AudioSink` playback of remote audio, no Opus, no echo cancellation; mic and speaker are still
   unverified on every OS.
3. `IrohLoadBalancer` is not wired: every peer broadcasts to every peer (full mesh). Needs capacity reports over
   `send_message`, `select_backbone`, tile-role culling (`IrohTileRole::rendition_height`) -> rendition requests,
   and forwarding.
4. Worker: signed announcements (Ed25519 via WebCrypto, as the design says; today anyone with the room id can
   announce any node id), an explicit leave, deploy (the user), the `azlin://` scheme registration (installer),
   AzCalendar minting links (ledger item 5), D1 if wanted.
5. App: a Leave button; only the lower id dials (with relays off an asymmetric network could keep a pair apart;
   the n0 relays are on by default for a remote server); `doc/guide/en/system/realtime-media.md` still describes
   the old UDP loopback azul-meet (stale before this change).

## Notes

- NO DUPLICATION: the server address uses `azul::url::Url` (my first `host_port` was removed). `parse_room_link`
  keeps its own small scheme/path split so `rooms.rs` stays azul-free and unit-testable; it only needs the path
  segments and the `azlin://` form.
- Files touched: only `examples/azul-meet/**`, `scripts/M1_AZMEET_ROOMS*`. No shared files.

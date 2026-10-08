# AZMEET15 progress (production AzMeet: no demo, E2E-encrypted chatrooms, meetings, key exchange)

Worktrees: azul `.claude/worktrees/agent-a6f8c1ad0d3e80fa3` (branch worktree-agent-a6f8c1ad0d3e80fa3,
based on fix/input-bugs-2026-09-19 @ 5d78255a4, merged with it again @ 0a7356d37 for azlin_config's
endpoints); azul-apps `../azul-apps-wt/meet15` (branch local/meet15 @ be92b09; the lead merged it
into local/infra15). Nothing was compiled or run here (no cargo / npm test / wrangler): every Rust
file was reviewed by reading (three read-only reviews: no compile error found, the warnings fixed).

## DONE (azul)
- c01509edf CRYPTO.md (the design first); b4b877b32 its AzCalendar paragraph.
- 05b488378 crypto.rs; 118e44be5 chatroom.rs (state machine + FakeWorker tests); b19326d38
  roomlist.rs + identity.rs.
- 999e99822, fe0e02b28 lib.rs: no in-process demo (an unreachable or missing server is an error
  with Retry), links carry the invite secret, signed requests, identity from the keyring /
  --identity-file, chat through the Worker (sealed), signed announcements + pending connections,
  rooms list, room view, knock / admit, meeting times.
- 414ac358a ui.rs (room view, Your rooms, Schedule, knock, safety codes); 742639c86 args.rs
  (--identity-file, --open, --chat-room, --starts-at, --ends-at); 8fbd3f2c5 Cargo.toml text.
- 1d78b7576 scripts/azmeet_e2e.py phase `crypto` (+ --worker-url / --sqld-url / --sqld-token-file /
  --db-file / --relay-url; identity files in every phase; the DB scan).
- e953029e4 endpoints from azul-appkit azlin_config (meet + relay); rooms.rs keeps
  parse_room_link -> Option<RoomKey> for AzCalendar, read_room_link reads the secret.
- 158356313 AzCalendar links carry an invite secret (src/invite.rs shared by path), registered with
  invite_key; its scripts follow; review fixes. e65274ff6 "Join meeting" passes --worker/--join.
- ab19b025e node scripts count signed announcements; azmeet_cpu.py's in-memory dev server.
- 678d35e52 docs/HARDCODED.md AzMeet rows; d101d80a0 doc/guide realtime-media.md.

## DONE (azul-apps local/meet15)
- 3fc8002 RED tests, 6aff9cc encrypted rooms (members, sealed keys, ciphertext history, signed
  peers, schema + ADDED_COLUMNS), 005cc5d RED + be92b09 the departed list.

## For the lead (build / run)
- azul: `cargo test --release -p AzMeet`, `cargo test --release -p AzCalendar`,
  `cargo build --release -p azul-dll --features build-dll && cargo build --release -p AzMeet -p AzCalendar`
- azul-apps: `cd cf-workers/meet && node --test "test/*.test.mjs"`
- e2e (own dev server): `python3 scripts/azmeet_e2e.py --phases direct,relay,crypto`
- e2e (local stack): `python3 scripts/azmeet_e2e.py --worker-url http://127.0.0.1:8790
  --sqld-url http://127.0.0.1:8082 --sqld-token-file ../azul-apps/local/state/keys/apps-db/rw.jwt
  --relay-url http://127.0.0.1:3340 --relay-metrics-url http://127.0.0.1:3341/metrics`

## Open questions (for the user)
- Should AZMEET_WORKER outrank the server saved on the start screen? (kept: saved wins, as before)
- chat.jsonl in the data tree is plaintext (the user's own record); encrypt at rest?
- Member removal, a new link for a room, a signed roster: not done (CRYPTO.md 12).
- The landing page's "Open in AzMeet" drops the fragment (a knock instead of a join).

# M1_AZMEET_ROOMS progress

Branch `wt/m1-azmeet-rooms` (base 4b3eae56a). Worker: `/Users/fschutt/Development/azul-apps-m1`, branch
`cf-workers-meet` (worktree of azul-apps `main`).

## DONE
- azul-apps `a75497c` test(meet): the Worker's behaviour, RED (modules missing).
- azul-apps `047c7de` feat(meet): handler + SQL store + libSQL-over-HTTP and node:sqlite adapters, Worker entry,
  dev-server.mjs, wrangler.toml, README. `node --test "test/*.test.mjs"`: 41 pass.
- azul `1f8719619` test(azmeet): rooms.rs tests, stubs (RED).
- azul `45891d7d2` feat(azmeet): rooms.rs pure logic (GREEN; type-checked alone with rustc --emit=metadata --test).
- azul `07cbd6a10` test(azmeet): examples/azul-meet/scripts/two-clients.mjs (RED until the app has rooms;
  orchestration dry-run PASS against a Node stand-in for the app).
- azul `f6468fd94` feat(azmeet): start screen, new meeting / join, announce + poll on azul Threads, per-peer tiles,
  demo fallback with a notice.
- azul `27acb81a8` refactor(azmeet): server address via azul::url::Url (removed the duplicate host_port parser).
- azul `aaa4df36c` fix(azmeet): demo notice wording.
- Report `scripts/M1_AZMEET_ROOMS_2026_09_29.md`.

## IN PROGRESS
- nothing

## NEXT (for the parent)
- `cargo test -p AzMeet --lib`; build AzMeet + libazul with the debug server; run two-clients.mjs (see the report).

## Open questions
- Short codes resolve on the landing route (rate limited) only; the peers API takes the full id. OK?
- libSQL (Turso) chosen for production; D1 not implemented.

# AZCLOUD15 progress: examples/azcloud-api (the client foundation) + the azcloud CLI

Task: azul-apps iso/docs/AZDRIVE-INTEGRATION.md §3 - `Account`, `Drive` (iroh first, HTTPS fallback),
`Sync` (§13 / PLAN §12.3-lite: CAS index + BLAKE3 blobs + local index), `Share`, the `azcloud` CLI,
an `endpoints` section in azul-appkit's shared Azlin config, scripts/azcloud_e2e.py.
Worktree: .claude/worktrees/agent-ac6b5f2a41bfa4152, branch worktree-agent-ac6b5f2a41bfa4152
(fast-forwarded from 626aa43c6 to the stated tip 5d78255a4 before any edit). No compiles (house rule).

## DONE (commit hashes)
- 1d51ad756 this file
- 91d3bb6ab azul-appkit azlin_config.rs: `endpoints` section, profiles, env names, layered resolution + tests
- 07bc14de6 crate (own [workspace] + rust-toolchain 1.99, root `exclude`), settings, state dir, secrets
- bd83b2c53 token API (routes incl. /credentials) + Account (signup, join/invite, refresh lock, lockdown)
- 77c419a84 Drive: transport preference (probe, transport.json, 5 min retry, fallback re-probe)
- aae993ff2 Sync: rules, remote index, local index + scan, three-way merge, MemStore, CAS loop, GC, tests
- aa3d368ac Share + bin/azcloud.rs
- Every file parses (rustfmt --check of the copied crate tree: 0 diff hunks). NOTHING COMPILED YET.

## IN PROGRESS
- scripts/azcloud_e2e.py

## NEXT (in order)
1. scripts/azcloud_e2e.py (signup, 1 MiB + 50 MiB up/down, sync twice, .azlin round trip A <-> B,
   transports https + iroh)
2. final review pass of the Rust for compile errors (by reading; no cargo); report

## Findings so far (for the report)
- azlin-client `TokenServer::refresh` posts to `/v1/drives/{id}/refresh`; the token server only routes
  `POST /v1/drives/{id}/credentials` -> every refresh through azlin-client 404s (no test calls it).
- The token server's node list (`drives::node_list`) has no `sign_pubkey` / `iroh_id` / `iroh_addrs`:
  a client cannot learn a node's iroh id from the bundle.
- signup's `drive.location.auth` = `{"type":"azlin",...}`: azul-storage's `DriveAuth` cannot parse it
  (only keyring / access_link) -> writing the bundle's drive into drives.json as-is breaks AzDrive.
- `azctl test gui` sets `AZLIN_TOKEN_SERVER`, `AZLIN_E2E_DRIVE_JSON`, `AZLIN_E2E_HOME`: nothing in azul
  reads them; it looks for `target/release/azdrive` (the bin is `AzDrive`: works only on a
  case-insensitive FS) and tests in `../azul/apps/azdrive/e2e` (does not exist).
- Two env names for the token server: `AZLIN_TOKEN_URL` (azlin-client tests, `azctl test client`)
  and `AZLIN_TOKEN_SERVER` (`azctl test gui`).
- AzMeet's `LOCAL_WORKER` is `http://127.0.0.1:8787` (dev-server.mjs / wrangler default); the lead
  runs the meet Worker on 8790. AzMeet's order is flag > saved > env > built-in (saved beats env).
- azinit's iroh endpoint and azlin-client's `iroh_transport::endpoint()` both use `presets::N0`
  (n0's public relays + DNS discovery, hard-coded): the local relay (:3340) is never used.
- azul-storage (azul, blocking, SigV4) and azlin-client (azul-apps, async reqwest) are two S3 clients.

## Open questions for the user
- Where azlin-client lives (sibling path dep today; azul CI cannot build azcloud-api).
- What the .azlin sync includes (see the report's table).

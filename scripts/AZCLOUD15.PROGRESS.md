# AZCLOUD15 progress: examples/azcloud-api (the client foundation) + the azcloud CLI

Task: azul-apps iso/docs/AZDRIVE-INTEGRATION.md §3 - `Account`, `Drive` (iroh first, HTTPS fallback),
`Sync` (§13 / PLAN §12.3-lite: CAS index + BLAKE3 blobs + local index), `Share`, the `azcloud` CLI,
an `endpoints` section in azul-appkit's shared Azlin config, scripts/azcloud_e2e.py.
Worktree: .claude/worktrees/agent-ac6b5f2a41bfa4152, branch worktree-agent-ac6b5f2a41bfa4152
(fast-forwarded from 626aa43c6 to the stated tip 5d78255a4 before any edit). No compiles (house rule).

## DONE (commit hashes, oldest first)
- 1d51ad756 this file
- 91d3bb6ab azul-appkit azlin_config.rs: `endpoints` section, profiles, env names, layered resolution + tests
- 07bc14de6 crate (own [workspace] + rust-toolchain 1.99, root `exclude`), settings, state dir, secrets
- bd83b2c53 token API (routes incl. /credentials) + Account (signup, join/invite, refresh lock, lockdown)
- 77c419a84 Drive: transport preference (probe, transport.json, 5 min retry, fallback re-probe)
- aae993ff2 Sync: rules, remote index, local index + scan, three-way merge, MemStore, CAS loop, GC, tests
- aa3d368ac Share + bin/azcloud.rs
- (progress) ; e2e script ; slice fix ; e2e info/share/gc checks
- Every Rust file parses: rustfmt --check of the copied crate tree reports 0 diff hunks.
  NOTHING WAS COMPILED - the lead compiles and runs the tests.

## How the lead builds and tests (release only)
    cd examples/azcloud-api            # rustup picks 1.99.0 from this folder's rust-toolchain.toml
    cargo build --release              # target/release/azcloud (feature iroh on by default)
    cargo build --release --no-default-features   # the HTTPS-only build must compile too
    cargo test --release               # unit tests (config, state, account, transport, rules, index, merge, CAS loop)
    cd ../.. && cargo test --release -p azul-appkit          # the shared config (azul workspace, 1.91)
    # cluster up (azul-apps/iso: azctl dev up --processes), then from the azul checkout:
    AZLIN_TOKEN_URL=http://127.0.0.1:8081 python3 scripts/azcloud_e2e.py
    # iroh leg: a node built with `cargo build -p azinit --features dev,iroh`, started with
    # AZLIN_AZINIT=<that binary> azctl dev up --processes; then
    AZLIN_DEV_STATE=../azul-apps/iso/dev/state python3 scripts/azcloud_e2e.py --require-iroh

## NEXT (not done)
- compile + fix whatever the compiler finds (most likely spots: drive.rs `run` closure lifetimes,
  the iroh 1.3 builder calls, async fn in the RemoteStore trait)
- azlin-client fixes in azul-apps (cannot edit from here): `TokenServer::refresh` -> `/credentials`;
  `iroh_transport::endpoint()` should take the relay from config
- token server: list `iroh_id` + `iroh_addrs` in the node list; a link route over `public_links`
- AzDrive: read `DriveAuth::Azlin` (refresh), the OS keyring entries; AzMeet: read `endpoints`
- node-kill step in the e2e (azctl chaos), streaming uploads for files > memory

## Findings (in the report)
- azlin-client `TokenServer::refresh` posts to `/v1/drives/{id}/refresh`; the router only has
  `POST /v1/drives/{id}/credentials` -> every refresh through azlin-client 404s (no test calls it).
- The token server's node list (`drives::node_list`) has no `sign_pubkey` / `iroh_id` / `iroh_addrs`.
- signup's `drive.location.auth` = `{"type":"azlin",...}`: azul-storage's `DriveAuth` cannot parse it.
- `azctl test gui` sets `AZLIN_TOKEN_SERVER`, `AZLIN_E2E_DRIVE_JSON`, `AZLIN_E2E_HOME`: nothing in azul
  reads them; it looks for `target/release/azdrive` (the bin is `AzDrive`) and `../azul/apps/azdrive/e2e`.
- AzMeet's `LOCAL_WORKER` is `http://127.0.0.1:8787`; the lead runs the meet Worker on 8790.
- azinit's iroh endpoint and azlin-client's `iroh_transport::endpoint()` use `presets::N0` (n0's relays).
- The lockdown answer names the drive "Azlin Storage" (the server stores no drive name).
- azul-storage (blocking SigV4) and azlin-client (async reqwest) are two S3 clients.
